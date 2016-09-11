extern crate x11;
extern crate num_iter;

mod context;
mod encoder;
mod tables;
mod util;
mod xinterface;
mod udp;
mod pending_acks;
mod messages;

use messages::
{
    ContextMessage,
    MainMessage,
};

use std::sync::mpsc::
{
    channel,
    Sender,
    Receiver,
    RecvTimeoutError,
};

use std::thread::JoinHandle;

use std::time:: Duration;

fn main ()
{
    let width = 640u32;
    let offset_x = 0;
    let height = 368u32;
    let offset_y = 0;
    let raw_bpp = 4;

    //divide fr by 1.003, result is closer to wanted framerate.
    let mut fr = 10u64;
    fr = 1_000_000_000_000u64 / (1003u64 * fr);
    let frame_duration = Duration::new(0, fr as u32);

    // Outer loop.
    'outer: loop {
        let (context_handle, encoder_handle, pending_handle, sender_handle,
            receiver_handle, context_sender, main_receiver) =
            start_threads(width, offset_x, height, offset_y, raw_bpp);

        // Await connection, then send initial image
        match main_receiver.recv() {
            Ok(MainMessage::Init) => context_sender.send(ContextMessage::Init).unwrap(),
            _ => panic!("Did not initialize successfully.")
        };

        // Inner loop. Update image once per frame duration.
        'inner: loop {
            match main_receiver.recv_timeout(frame_duration) {
                Ok(MainMessage::ChangeView(v)) => {
                    context_sender.send(ContextMessage::ChangeView(v)).unwrap();
                },
                Ok(MainMessage::Init) => {
                    context_sender.send(ContextMessage::Init).unwrap();
                },
                Ok(MainMessage::Close) => {
                    context_sender.send(ContextMessage::Close).unwrap();
                    join_threads(receiver_handle, context_handle, encoder_handle,
                                 sender_handle, pending_handle);

                    break 'inner; // Start waiting for connection
                },
                Ok(MainMessage::Exit) => {
                    context_sender.send(ContextMessage::Close).unwrap();
                    join_threads(receiver_handle, context_handle, encoder_handle,
                                 sender_handle, pending_handle);
                    break 'outer; // Exit server.
                },
                Err(RecvTimeoutError::Timeout) => {
                    context_sender.send(ContextMessage::NewScreenshot).unwrap();
                },
                _ => panic!()
            }
        }
    }

    println!("Closed.");
}

fn start_threads(width: u32,
                 offset_x: i32,
                 height: u32,
                 offset_y: i32,
                 raw_bpp: isize)
    -> (JoinHandle<()>,
        JoinHandle<()>,
        JoinHandle<()>,
        JoinHandle<()>,
        JoinHandle<()>,
        Sender<ContextMessage>,
        Receiver<MainMessage>)
{
    // Create channels.
    let (context_sender, context_receiver) = channel();
    let (encoder_sender, encoder_receiver) = channel();
    let (main_sender, main_receiver) = channel();
    let (pending_ack_sender, pending_ack_receiver) = channel();
    let (udp_sender_sender, udp_sender_receiver) = channel();

    // Start threads
    let context_handle =
        context::start_context_thread(width,
                                      offset_x,
                                      height,
                                      offset_y,
                                      encoder_sender,
                                      context_receiver);

    let encoder_handle =
        encoder::start_encoder_thread(width as isize,
                                      height as isize,
                                      raw_bpp,
                                      udp_sender_sender,
                                      encoder_receiver);

    let pending_handle =
        pending_acks::start_pending_ack_thread(context_sender.clone(),
                                               pending_ack_receiver);

    let (sender_handle, receiver_handle) =
        udp::init_udp_sockets(pending_ack_sender,
                              udp_sender_receiver,
                              main_sender);

    (context_handle,
     encoder_handle,
     pending_handle,
     sender_handle,
     receiver_handle,
     context_sender,
     main_receiver)
}

fn join_threads(receiver_handle: JoinHandle<()>,
                context_handle: JoinHandle<()>,
                encoder_handle: JoinHandle<()>,
                sender_handle: JoinHandle<()>,
                pending_handle: JoinHandle<()>)
{
    receiver_handle.join().unwrap();
    context_handle.join().unwrap();
    encoder_handle.join().unwrap();
    sender_handle.join().unwrap();
    pending_handle.join().unwrap();
}