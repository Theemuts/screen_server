#![allow(dead_code)]

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

use std::time::{SystemTime, Duration};

fn main () {
    let width = 640u32;
    let offset_x = 0;
    let height = 368u32;
    let offset_y = 0;
    let raw_bbp = 4;

    let mut fr = 10u64;
    fr = 1_000_000_000_000u64 / (1003u64 * fr);
    let frame_duration = Duration::new(0, fr as u32);

    // Init UDP sockets
    let (data_channel, packet_ack_channel) = udp::Udp::init_udp_sockets();

    //divide fr by 1.003, result is closer to wanted framerate.

    let mut encoder = encoder::Encoder::new(width as isize,
                                            height as isize,
                                            raw_bbp,
                                            data_channel);
    let mut context = context::Context::new(width, offset_x, height, offset_y, packet_ack_channel);

    let _ = encoder.initial_encode_rgb(&context);
    let mut decoder = decoder::Decoder::new(width as usize, height as usize);

    let n = 10000;
    let mut sizes = Vec::with_capacity(n);
    let mut times = Vec::with_capacity(n);

    let t4 = SystemTime::now();
    for _ in 0..1 {
        decoder.decode(&encoder.sink);
    }
    let mut t5;
    let mut sent;

    for _ in 0..n {
        context.handle_ack();
        t5 = SystemTime::now();
        context.get_new_screenshot();
        context.set_block_errors();
        sent = encoder.update_encode_rgb(&context);
        context.update_client_state(sent);
        sizes.push(encoder.sink_size());

        if frame_duration > t5.elapsed().unwrap() {
            std::thread::sleep((frame_duration - t5.elapsed().unwrap()));
        }

        times.push(t5.elapsed().unwrap().subsec_nanos() as f64 / 1000000000f64);
    }

    let t4 = t4.elapsed().unwrap();
    //println!("{:?}", t4);
    let _ = context.store_client_state();


    /*let av_s = av(&sizes);
    let std_s = std(&sizes, av_s);
    let framerate = framerate(&times);
    let av_f = av(&framerate);
    let std_f = std(&framerate, av_f);


    println!("Av update size: {} pm {} ({} in {}.{} ({} pm {}))",
             av_s, std_s, n, t4.as_secs(), t4.subsec_nanos(), av_f, std_f);

    let mbit = mbit_data(&sizes, 24);
    let av_mb = av(&mbit);
    let std_mb = std(&mbit, av_mb);
    println!("{} pm {} mbps", av_mb, std_mb);*/

    context.close();
    println!("");
}

fn av(values: &Vec<f64>) -> f64 {
    let mut av = 0f64;

    for v in values {
        av += *v;
    }

    av / values.len() as f64
}

fn std(values: &Vec<f64>, av: f64) -> f64 {
    let mut std = 0f64;

    for v in values {
        let del = *v - av;
        std += del*del;
    }

    (std / values.len() as f64).sqrt()
}

fn framerate(t: &Vec<f64>) -> Vec<f64> {
    let mut dt = Vec::with_capacity(t.len() - 1);

    for i in 0..t.len() {
        dt.push(1.0 / t[i]);
    }

    dt
}

fn mbit_data(sizes: &Vec<f64>, framerate: usize) -> Vec<f64> {
    let size = if framerate <= sizes.len() {
        sizes.len() - framerate
    } else {
        0
    };

    let mut mbit = Vec::with_capacity(size);

    for i in 0..size {
        mbit.push(0.0);

        for j in 0..framerate {
            mbit[i] += sizes[i + j];
        }

        mbit[i] *= 8f64 / 1048576f64;
    }

    mbit
}