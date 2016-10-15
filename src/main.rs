extern crate x11;
extern crate num_iter;
extern crate regex;
extern crate libxdo;

mod context;
mod encoder;
mod heartbeat;
mod monitor_info;
mod mouse;
mod pending_acks;
mod protocol;
mod tables;
mod udp;
mod util;
mod xinterface;

use monitor_info::MonitorInfo;

use protocol::
{
    ContextMessage,
    MainMessage,
    SenderMessage,
};

use std::str;

use std::sync::mpsc::
{
    channel,
    Sender,
    Receiver,
    RecvTimeoutError,
};

use std::thread::JoinHandle;

use std::time::Duration;

const MIN_SUPPORTED_PROTOCOL_VERSION: u8 = 1;
const MAX_SUPPORTED_PROTOCOL_VERSION: u8 = 1;

fn main ()
{
    let xdo_session = mouse::new_session();
    let monitor_info = MonitorInfo::get_all();

    //divide fr by 1.003, result is closer to wanted framerate.
    let mut fr = 10u64;
    fr = 1_000_000_000_000u64 / (1003u64 * fr);
    let frame_duration = Duration::new(0, fr as u32);

    // Outer loop.
    // Break from this loop to exit program.
    'outer: loop {
        let mut src = None;
        let mut has_init = false;
        let mut protocol_version;

        println!("Start threads.");
        let (handles, context_sender, udp_sender_sender, main_receiver) =
            start_threads(&monitor_info, 10);

        // Inner loop.
        // Update image once per frame duration.
        // Breaking from this loop will reset all threads.
        'inner: loop {
            match main_receiver.recv_timeout(frame_duration) {
                Ok(MainMessage::Handshake(new_src, min, max))
                    if src.as_ref().is_none()
                     & (max >= MIN_SUPPORTED_PROTOCOL_VERSION)
                     & (min <= MAX_SUPPORTED_PROTOCOL_VERSION) =>
                {
                    println!("Main: Accept handshake");

                    // Pick the highest supported protocol version
                    protocol_version = if max >= MAX_SUPPORTED_PROTOCOL_VERSION {
                        MAX_SUPPORTED_PROTOCOL_VERSION
                    } else {
                        max
                    };

                    // Set the source to reject future handshake requests.
                    src = Some(new_src);

                    // Acknowledge handshake
                    let msg = SenderMessage::AcceptHandshake(new_src, protocol_version);
                    udp_sender_sender.send(msg).unwrap();
                },
                Ok(MainMessage::Handshake(new_src, _, _)) => {
                    println!("Main: Reject handshake");
                    // reject, there is another active connection or unsupported protocol version.
                    let msg = SenderMessage::RejectHandshake(new_src);
                    udp_sender_sender.send(msg).unwrap();
                },
                Ok(MainMessage::RequestScreenInfo) => {
                    if src.as_ref().is_some() {
                        println!("Main: Request screen info");
                        let msg = SenderMessage::ScreenInfo(MonitorInfo::serialize_vec(&monitor_info));
                        udp_sender_sender.send(msg).unwrap();
                    }
                },
                Ok(MainMessage::RequestView(screen, segment)) => {
                    if src.as_ref().is_some() {
                        println!("Main: Request view");
                        let msg = ContextMessage::RequestView(screen, segment);
                        context_sender.send(msg).unwrap();

                        has_init = true;
                    }
                },
                Ok(MainMessage::Refresh) => {
                    if src.as_ref().is_some() {
                        println!("Main: Refresh");
                        let msg = ContextMessage::Refresh;
                        context_sender.send(msg).unwrap();
                    }
                },
                Ok(MainMessage::Close) => {
                    if src.as_ref().is_some() {
                        println!("Main: Close");
                        context_sender.send(ContextMessage::Close).unwrap();

                        join_threads(handles);

                        break 'inner; // Start waiting for connection
                    }
                },
                Ok(MainMessage::Exit) => {
                    if src.as_ref().is_some() {
                        println!("Main: Exit");
                        context_sender.send(ContextMessage::Close).unwrap();

                        join_threads(handles);

                        break 'outer; // Exit server.
                    }
                },
                Ok(MainMessage::LeftClick(x, y)) => {
                    if src.as_ref().is_some() {
                        xdo_session.move_mouse(x as i32, y as i32, 0).unwrap();
                        xdo_session.click(1).unwrap();
                    }
                },
                Ok(MainMessage::RightClick(x, y)) => {
                    if src.as_ref().is_some() {
                        xdo_session.move_mouse(x as i32, y as i32, 0).unwrap();
                        xdo_session.click(3).unwrap();
                    }
                },
                Ok(MainMessage::DoubleClick(x, y)) => {
                                        if src.as_ref().is_some() {
                        xdo_session.move_mouse(x as i32, y as i32, 0).unwrap();
                        xdo_session.click(1).unwrap();
                        xdo_session.move_mouse(x as i32, y as i32, 0).unwrap();
                        xdo_session.click(1).unwrap();
                    }
                },
                Ok(MainMessage::Drag(x0, y0, x1, y1)) => {
                    if src.as_ref().is_some() {
                        xdo_session.move_mouse(x0 as i32, y0 as i32, 0).unwrap();
                        xdo_session.mouse_down(1).unwrap();
                        xdo_session.move_mouse(x1 as i32, y1 as i32, 0).unwrap();
                        xdo_session.mouse_up(1).unwrap();
                    }
                },
                Ok(MainMessage::Keyboard(data)) => {
                    if src.as_ref().is_some() {
                        let msg = str::from_utf8(&(data[1..])).unwrap();
                        xdo_session.send_keysequence(&msg, 10).unwrap();
                    }
                },
                Err(RecvTimeoutError::Timeout) if has_init => {
                    if src.as_ref().is_some() {
                        let msg = ContextMessage::NewScreenshot;
                        context_sender.send(msg).unwrap();
                    }
                },
                Err(_) => (),
            }
        }
    }

    println!("Closed.");
}

fn start_threads(monitor_info: &Vec<MonitorInfo>,
                 heartbeat_timeout: u64)
    -> (Vec<JoinHandle<()>>,
        Sender<ContextMessage>,
        Sender<SenderMessage>,
        Receiver<MainMessage>)
{
    let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(6);
    // Create channels.
    let (context_sender, context_receiver) = channel();
    let (encoder_sender, encoder_receiver) = channel();
    let (main_sender, main_receiver) = channel();
    let (pending_ack_sender, pending_ack_receiver) = channel();
    let (udp_sender_sender, udp_sender_receiver) = channel();
    let (heartbeat_sender, heartbeat_receiver) = channel();

    // Start threads
    handles.push(
        context::start_context_thread(monitor_info.clone(),
                                      encoder_sender,
                                      context_receiver));

    handles.push(
        encoder::start_encoder_thread(monitor_info.clone(),
                                      udp_sender_sender.clone(),
                                      encoder_receiver));

    handles.push(
        pending_acks::start_pending_ack_thread(context_sender.clone(),
                                               pending_ack_receiver));

    let (sender_handle, receiver_handle) =
        udp::init_udp_sockets(pending_ack_sender,
                              udp_sender_receiver,
                              main_sender.clone(),
                              heartbeat_sender.clone());

    handles.push(sender_handle);
    handles.push(receiver_handle);

    handles.push(
        heartbeat::start_heartbeat_thread(main_sender.clone(),
                                          heartbeat_receiver,
                                          heartbeat_timeout));

    (handles,
     context_sender,
     udp_sender_sender,
     main_receiver)
}

fn join_threads(handles: Vec<JoinHandle<()>>)
{
    for handle in handles {
        handle.join().unwrap();
    }
}