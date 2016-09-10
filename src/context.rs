use super::xinterface;
use super::x11::xlib;
use super::util::{get_data, DataBox};

use std::fs::File;

use std::io;
use std::io::prelude::*;
use std::io::BufWriter;

use std::sync::mpsc::{Sender, Receiver, channel};

use std::thread;

use std::time::{SystemTime, Duration};

use num_iter::range_step;

use super::messages::{ContextMessage, EncoderMessage};

#[derive(Debug)]
pub struct Context {
    pub image_pointer: Option<*mut xlib::XImage>,
    display: *mut xlib::Display,
    window: u64,
    width: u32,
    height: u32,
    offset_x: i32,
    offset_y: i32,
    client_state: Vec<u8>,
    bpp: u32,
    raw_bpp: u32,
    size: usize,
    state_size: u32,
    raw_size: u32,
    block_size: u32,
    macroblock_size: u32,
    n_blocks: usize,
    n_blocks_x: u32,
    n_blocks_y: u32,
    errors: Vec<(i64, usize)>,
    block_table: Vec<usize>,
    timestamp: u32,
    current_version: Vec<u32>,
    most_recent_version: Vec<u32>
}

pub fn start_context_thread(width: u32,
                            offset_x: i32,
                            height: u32,
                            offset_y: i32,
                            to_encoder: Sender<EncoderMessage>,
                            receiver: Receiver<ContextMessage>)
{

    thread::spawn(move || {
        let mut context = Context::new(width, offset_x, height, offset_y);

        loop {
            match receiver.recv() {
                Ok(ContextMessage::Init) => {
                    context.get_new_screenshot();
                    context.set_initial_state();
                    to_encoder.send(EncoderMessage::FirstImage(get_data(context.image_pointer)));
                },
                Ok(ContextMessage::Close) => {
                    context.close();
                    to_encoder.send(EncoderMessage::Close);
                }
                Ok(ContextMessage::NewScreenshot) => {
                    context.get_new_screenshot();
                    context.set_block_errors();
                    context.update_client_state();
                    let pnt = context.get_image_pointer();
                    let msg = EncoderMessage::DataAndErrors(pnt, context.errors.clone());
                    to_encoder.send(msg);
                },
                Ok(ContextMessage::AckPackets(timestamp, ids)) => {
                    context.handle_ack(timestamp, &ids);
                },
                _ => panic!()
            };
        }
    });
}

impl Context {
    pub fn new(width: u32, offset_x: i32, height: u32, offset_y: i32) -> Self {
        if (height % 16 != 0) | (width % 16 != 0) {
            panic!("height and width must be divisible by 16")
        }

        let display = xinterface::open_display();

        let block_size = 8;
        let n_blocks_x = width / block_size;
        let n_blocks_y = height / block_size;
        let n_blocks = (n_blocks_x * n_blocks_y) as usize;

        let bpp = 3;

        let mut c = Context {
            image_pointer: None,
            display: display,
            window: xinterface::get_root_window(display),
            width: width,
            height: height,
            offset_x: offset_x,
            offset_y: offset_y,
            client_state: vec![0u8; (height*width*bpp) as usize],
            bpp: bpp,
            raw_bpp: 4,
            block_size: 8,
            macroblock_size: 16,
            size: (height*width) as usize,
            state_size: height*width*bpp,
            raw_size: height*width*4,
            n_blocks: n_blocks,
            n_blocks_x: n_blocks_x,
            n_blocks_y: n_blocks_y,
            errors: vec![(0i64, 0usize); n_blocks],
            block_table: vec![0usize; (width*height) as usize],
            timestamp: 0,
            current_version: vec![0u32; (n_blocks_x * n_blocks_y / 4) as usize],
            most_recent_version: vec![0u32; (n_blocks_x * n_blocks_y / 4) as usize]
        };

        c.generate_block_lookup_table();
        c
    }

    fn get_image_pointer(&self) -> DataBox {
        get_data(self.image_pointer)
    }

    // TODO: return pointer
    fn get_new_screenshot(&mut self) {
        // Delete old (is None if uninitialized)
        if let Some(im_pointer) = self.image_pointer {
            xinterface::destroy_image(im_pointer);
            self.image_pointer = None;
        }

        // Use a modern GPU...
        let image = xinterface::get_image(self.display, self.window,
                                          self.width, self.offset_x,
                                          self.height, self.offset_y);

        self.image_pointer = Some(image);
    }

    fn set_initial_state(&mut self) {
        xinterface::copy_image(self.image_pointer.unwrap(), &mut self.client_state, self.width, self.height);
    }

    fn close(&self) {
        xinterface::close_display(self.display);
    }

    fn set_block_errors(&mut self) {
        let DataBox(data) = get_data(self.image_pointer);
        // Define here for speed
        let mut r;
        let mut g;
        let mut b;
        let mut d_b;
        let mut d_g;
        let mut d_r;

        // Reset errors.
        for n in 0..self.errors.len() {
            self.errors[n] = (0, n);
        }

        // Get all pixels and errors
        let it = (&self.block_table).iter().enumerate();
        let mut raw_ind;
        let mut state_ind;


        for (ind, block) in it {
            raw_ind = ind as isize * 4;
            state_ind = ind * 3;

            unsafe {
                b = *data.offset(raw_ind) as u8;
                g = *data.offset(raw_ind + 1) as u8;
                r = *data.offset(raw_ind + 2) as u8;
            }

            d_r = r as i64 - self.client_state[state_ind] as i64;
            d_g = g as i64 - self.client_state[state_ind + 1] as i64;
            d_b = b as i64 - self.client_state[state_ind + 2] as i64;

            self.errors[*block].0 += d_r * d_r + d_g * d_g + d_b * d_b;
        }

        // Sort errors for consumption, largest errors first.
        self.errors.sort_by(|a, b| b.cmp(a));
    }

    fn generate_block_lookup_table(&mut self) {
        let macroblocks_x = (self.width / self.macroblock_size) as usize;
        let mut col;
        let mut row;

        for n in 0..self.size {
            col = n % self.width as usize;
            row = (n - col) / self.width as usize;
            row /= self.macroblock_size as usize;
            row *= macroblocks_x;
            col /= self.macroblock_size as usize;
            self.block_table[n] = row + col;
        }
    }

    fn store_client_state(&self) -> io::Result<()> {
        let mut buffer = BufWriter::new(try!(File::create("foo.txt")));
        for x in &self.client_state {
            try!(write!(buffer, "{} ", x));
        }

        try!(buffer.flush());

        Ok(())
    }

    fn update_client_state(&mut self) {
        let mut r;
        let mut g;
        let mut b;
        let mut dest_ind;

        self.timestamp += 1;

        let DataBox(data) = get_data(self.image_pointer);
        let blocks_x = self.width as isize / 16;

        for err in &self.errors {
            let (error, block) = *err;
            if error == 0 { break }

            self.most_recent_version[block] = self.timestamp;

            // Calculate initial index
            let x_block = block as isize % blocks_x;
            let x0 = x_block*64;
            let y0 = (block as isize - x_block)*16/blocks_x;
            let ind0 = y0 * 4 * self.width as isize + x0;

            for row_ind in range_step(ind0, ind0 + 16*4*self.width as isize, 4*self.width as isize) {
                for ind in range_step(row_ind, row_ind + 16*4, 4) {
                    if ind >= (4 * self.width * self.height) as isize {
                        break
                    }

                    b = value_at(data, ind, (self.width*self.height) as isize * 4);
                    g = value_at(data, ind + 1, (self.width*self.height) as isize * 4);
                    r = value_at(data, ind + 2, (self.width*self.height) as isize * 4);

                    dest_ind = ind as usize*3/4;

                    self.client_state[dest_ind] = r;
                    self.client_state[dest_ind+1] = g;
                    self.client_state[dest_ind+2] = b;
                }
            }
        }
    }

    fn handle_ack(&mut self, timestamp: u32, ids: &Vec<u16>) {
        for id in ids {
            if self.current_version[*id as usize] < timestamp {
                self.current_version[*id as usize] = timestamp;
            }
        }
    }
}

fn value_at(s: *mut i8, index: isize, size: isize) -> u8 {
    unsafe {
        if index < size {
            *s.offset(index) as u8
        } else {
            *s.offset(size - 1) as u8
        }
    }
}