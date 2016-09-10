extern crate x11;
extern crate num_iter;

mod context;
mod decoder;
mod encoder;
mod entropy;
mod tables;
mod util;
mod xinterface;
mod udp;
mod pending_acks;
mod messages;

use std::time::{SystemTime, Duration};

use std::sync::mpsc::{channel, RecvTimeoutError};

use messages::{MainMessage, ContextMessage};

fn main () {
    let width = 640u32;
    let offset_x = 0;
    let height = 368u32;
    let offset_y = 0;
    let raw_bbp = 4;

    //divide fr by 1.003, result is closer to wanted framerate.
    let mut fr = 10u64;
    fr = 1_000_000_000_000u64 / (1003u64 * fr);
    let frame_duration = Duration::new(0, fr as u32);

    // Create channels.
    let (main_sender, main_receiver) = channel();
    let (context_sender, context_receiver) = channel();
    let (encoder_sender, encoder_receiver) = channel();
    let (pending_ack_sender, pending_ack_receiver) = channel();
    let (udp_sender_sender, udp_sender_receiver) = channel();

    // Start threads
    context::start_context_thread(width, offset_x, height, offset_y,
                                  encoder_sender, context_receiver);

    encoder::start_encoder_thread(width as isize, height as isize,
                                           raw_bbp, udp_sender_sender,
                                           encoder_receiver);

    pending_acks::start_pending_ack_thread(context_sender.clone(),
                                           pending_ack_receiver);

    udp::init_udp_sockets(pending_ack_sender, udp_sender_receiver, main_sender);

    match main_receiver.recv() {
        Ok(MainMessage::Init) => context_sender.send(ContextMessage::Init),
        _ => panic!("Did not initialize successfully.")
    };

    let mut last_render = SystemTime::now();
    let mut time_left = frame_duration;

    loop {
        match main_receiver.recv_timeout(time_left) {
            Ok(MainMessage::ChangeView(u8)) => {}, // Todo: change view to other part of screen
            Ok(MainMessage::Init) => {}, // Todo: re-init image
            Ok(MainMessage::Close) => {
                context_sender.send(ContextMessage::Close);
                break;
            },
            Err(RecvTimeoutError::Timeout) => {
                context_sender.send(ContextMessage::NewScreenshot);
                last_render = SystemTime::now();
                time_left = frame_duration;
            },
            _ => panic!()
        }
    }

    println!("Closed.");
}