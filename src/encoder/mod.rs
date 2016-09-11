/*
This jpeg encoder implementation is largely copied from
https://raw.githubusercontent.com/PistonDevelopers/image/
fbe7a0c5dd90a69a3eb2bd18edd583f2156aa08f/src/jpeg/encoder.rs
*/

mod fdct;
mod entropy;

use self::fdct::fdct;

use self::entropy::build_huff_lut;

use super::tables::*;

use super::util::
{
    value_at,
    DataBox
};

use std::thread::{
    self,
    JoinHandle
};

use super::messages::
{
    SenderMessage,
    EncoderMessage
};

use std::sync::mpsc::
{
    Sender,
    Receiver
};

use num_iter::range_step;

#[derive(Debug)]
pub struct Encoder {
    tables: Vec<u8>,
    luma_dctable: Vec<(u8, u16)>,
    luma_actable: Vec<(u8, u16)>,
    chroma_dctable: Vec<(u8, u16)>,
    chroma_actable: Vec<(u8, u16)>,
    width: isize,
    height: isize,
    size: isize,
    bpp: isize,
    macroblock_size: isize,

    udp_channel: Sender<SenderMessage>,

    accumulator: u32,
    nbits: u8,

    buffer: Vec<u8>,

    size_accumulator: u64,
    timestamp: u32
}

pub fn start_encoder_thread(width: isize,
                            height: isize,
                            bpp: isize,
                            udp_sender: Sender<SenderMessage>,
                            receiver: Receiver<EncoderMessage>)
    -> JoinHandle<()>
{
    thread::spawn(move || {
        let mut encoder = Encoder::new(width, height, bpp, udp_sender.clone());

        loop {
            match receiver.recv() {
                Ok(EncoderMessage::FirstImage(DataBox(data))) => {
                    encoder.initial_encode_rgb(data);
                },
                Ok(EncoderMessage::DataAndErrors(DataBox(data), errors)) => {
                    encoder.update_encode_rgb(data, &errors);
                },
                Ok(EncoderMessage::Close) => {
                    udp_sender.send(SenderMessage::Close).unwrap();
                    break;
                },
                _ => panic!()
            };
        };
    })
}

impl Encoder {
    fn new(width: isize,
               height: isize,
               bpp: isize,
               sender: Sender<SenderMessage>) -> Self
    {
        let ld = build_huff_lut(&STD_LUMA_DC_CODE_LENGTHS, &STD_LUMA_DC_VALUES);
        let la = build_huff_lut(&STD_LUMA_AC_CODE_LENGTHS, &STD_LUMA_AC_VALUES);

        let cd = build_huff_lut(&STD_CHROMA_DC_CODE_LENGTHS, &STD_CHROMA_DC_VALUES);
        let ca = build_huff_lut(&STD_CHROMA_AC_CODE_LENGTHS, &STD_CHROMA_AC_VALUES);

        let mut tables = Vec::new();
        tables.extend(STD_LUMA_QTABLE.iter().map(|&v| v));
        tables.extend(STD_CHROMA_QTABLE.iter().map(|&v| v));

        Encoder {
            tables: tables,
            luma_dctable: ld,
            luma_actable: la,
            chroma_dctable: cd,
            chroma_actable: ca,
            width: width,
            height: height,
            size: height * width * 4,
            macroblock_size: 16,
            bpp: bpp,
            accumulator: 0,
            size_accumulator: 0,
            nbits: 0,
            buffer: Vec::with_capacity(768),
            udp_channel: sender,
            timestamp: 0
        }
    }

    fn initial_encode_rgb(&mut self, data: *mut i8)
    {
        let mut dct_yblock   = [0i32; 64];
        let mut dct_cb_block = [0i32; 64];
        let mut dct_cr_block = [0i32; 64];

        let mut yblock   = [0u8; 64];
        let mut cb_block = [0u8; 64];
        let mut cr_block = [0u8; 64];

        let la = self.luma_actable.clone();
        let ld = self.luma_dctable.clone();
        let cd = self.chroma_dctable.clone();
        let ca = self.chroma_actable.clone();

        let mut x;
        let mut y;

        let mut bl = 0;

        for y0 in range_step(0, self.height, self.macroblock_size) {
            for x0 in range_step(0, self.width, self.macroblock_size) {
                self.write_bits(bl as u16, 10);

                for j in 0..2 {
                    for i in 0..2 {
                        x = x0 + 8*i;
                        y = y0 + 8*j;
                        let index = self.bpp * (y * self.width + x);
                        copy_blocks_ycbcr(data, index, self.width, self.bpp, self.size, &mut yblock, &mut cb_block, &mut cr_block);

                        // Level shift and fdct
                        // Coeffs are scaled by 8
                        fdct(&yblock, &mut dct_yblock);
                        fdct(&cb_block, &mut dct_cb_block);
                        fdct(&cr_block, &mut dct_cr_block);

                        // Quantization
                        for k in 0usize..64 {
                            dct_yblock[k]   = ((dct_yblock[k] / 8)   as f32 / self.tables[k] as f32).round() as i32;
                            dct_cb_block[k] = ((dct_cb_block[k] / 8) as f32 / self.tables[64..][k] as f32).round() as i32;
                            dct_cr_block[k] = ((dct_cr_block[k] / 8) as f32 / self.tables[64..][k] as f32).round() as i32;
                        }

                        self.write_block(&dct_yblock, &ld, &la);
                        self.write_block(&dct_cb_block, &cd, &ca);
                        self.write_block(&dct_cr_block, &cd, &ca);


                    }
                }
                self.write_final_bits();

                if self.buffer.len() > 0 {
                    self.buffer.shrink_to_fit();
                    self.udp_channel.send(SenderMessage::Macroblock(self.timestamp, self.buffer.clone())).unwrap();
                    self.buffer = Vec::with_capacity(768);
                }

                bl += 1;
            }
        }

        self.udp_channel.send(SenderMessage::EndOfData(self.timestamp)).unwrap();
    }

    fn update_encode_rgb(&mut self,
                         data: *mut i8,
                         errors: &Vec<(i64, usize)>)
        -> usize
    {
        self.timestamp += 1;

        let mut dct_yblock   = [0i32; 64];
        let mut dct_cb_block = [0i32; 64];
        let mut dct_cr_block = [0i32; 64];

        let mut yblock   = [0u8; 64];
        let mut cb_block = [0u8; 64];
        let mut cr_block = [0u8; 64];

        let la = self.luma_actable.clone();
        let ld = self.luma_dctable.clone();
        let cd = self.chroma_dctable.clone();
        let ca = self.chroma_actable.clone();

        let mut sent = 0;

        for error in errors {
            let (err, block) = *error;
            if err == 0 {
                // errors are sorted, 0 => no more changed blocks
                // TODO: don't add zero-error blocks to the error vec?
                break
            }

            self.write_bits(block as u16, 10);

            // Speed up with lookup table?

            let n_blocks_x = self.width / self.macroblock_size;

            let x0 = (block as isize % n_blocks_x) * 16;
            let y0 = (block as isize / n_blocks_x) * 16;


            for y in range_step(y0, y0+16, 8) {
                for x in range_step(x0, x0+16, 8) { // 2 * 4 * 8
                    let index = self.bpp * (y * self.width + x);
                    copy_blocks_ycbcr(data, index, self.width, self.bpp, self.size, &mut yblock, &mut cb_block, &mut cr_block);

                    // Level shift and fdct
                    // Coeffs are scaled by 8
                    fdct(&yblock, &mut dct_yblock);
                    fdct(&cb_block, &mut dct_cb_block);
                    fdct(&cr_block, &mut dct_cr_block);

                    // Quantization
                    for k in 0usize..64 {
                        dct_yblock[k]   = ((dct_yblock[k] / 8)   as f32 / self.tables[k] as f32).round() as i32;
                        dct_cb_block[k] = ((dct_cb_block[k] / 8) as f32 / self.tables[64..][k] as f32).round() as i32;
                        dct_cr_block[k] = ((dct_cr_block[k] / 8) as f32 / self.tables[64..][k] as f32).round() as i32;
                    }

                    self.write_block(&dct_yblock, &ld, &la);
                    self.write_block(&dct_cb_block, &cd, &ca);
                    self.write_block(&dct_cr_block, &cd, &ca);
                }
            }

            self.write_final_bits();

            if self.buffer.len() > 0 {
                self.buffer.shrink_to_fit();
                self.udp_channel.send(SenderMessage::Macroblock(self.timestamp, self.buffer.clone())).unwrap();
                self.buffer = Vec::with_capacity(768);
            }

            sent += 1;
        }

        self.udp_channel.send(SenderMessage::EndOfData(self.timestamp)).unwrap();
        sent
    }

    fn huffman_encode(&mut self, val: u8, table: &[(u8, u16)])
    {
        let (size, code) = table[val as usize];

        if size > 16 {
            panic!("bad huffman value");
        }

        self.write_bits(code, size)
    }

    fn write_block(&mut self,
                   block: &[i32],
                   dctable: &[(u8, u16)],
                   actable: &[(u8, u16)])
        -> i32
    {

        // TODO: Differential DC encoding for macroblocks.
        let dcval = block[0];
        let diff  = dcval;
        let (size, value) = encode_coefficient(diff);
        self.huffman_encode(size, dctable);
        self.write_bits(value, size);

        // Figure F.2
        let mut zero_run = 0;
        let mut k = 0usize;

        loop {
            k += 1;

            if block[UNZIGZAG[k] as usize] == 0 {
                if k == 63 {
                    self.huffman_encode(0x00, actable);
                    break
                }

                zero_run += 1;
            } else {
                while zero_run > 15 {
                    self.huffman_encode(0xF0, actable);
                    zero_run -= 16;
                }

                let (size, value) = encode_coefficient(block[UNZIGZAG[k] as usize]);
                let symbol = (zero_run << 4) | size;

                self.huffman_encode(symbol, actable);
                self.write_bits(value, size);

                zero_run = 0;

                if k == 63 {
                    break
                }
            }
        }

        dcval
    }

    fn write_bits(&mut self, bits: u16, size: u8)
    {
        if size == 0 {
            return
        }

        self.accumulator |= (bits as u32) << (32 - (self.nbits + size)) as usize;
        self.nbits += size;

        while self.nbits >= 8 {
            let byte = (self.accumulator & 0xFF000000u32) >> 24;

            self.buffer.push(byte as u8);
            self.nbits -= 8;
            self.accumulator <<= 8;

        }
    }

    fn write_final_bits(&mut self)
    {
        if self.nbits == 0 {
            self.accumulator = 0;
            return
        }

        while self.nbits >= 8 {
            let byte = (self.accumulator & (0xFFFFFFFFu32 << 24)) >> 24;
            self.buffer.push(byte as u8);

            self.nbits -= 8;
            self.accumulator <<= 8;
        }

        if self.nbits != 0 {
            let byte = (self.accumulator & (0xFFFFFFFFu32 << 24)) >> 24;
            self.buffer.push(byte as u8);

            if byte == 0xFF {
                self.buffer.push(0x00);
            }
        }

        self.nbits = 0;
        self.accumulator = 0;
    }
}

fn copy_blocks_ycbcr(source: *mut i8,
                     index: isize,
                     width: isize,
                     bpp: isize,
                     size: isize,
                     yb: &mut [u8; 64],
                     cbb: &mut [u8; 64],
                     crb: &mut [u8; 64])
{
    for y in 0isize..8 {
        for x in 0isize..8 {
            let ind = index + (y * width + x) * bpp;


            let b = value_at(source, ind, size);
            let g = value_at(source, ind + 1, size);
            let r = value_at(source, ind + 2, size);


            let (yc, cb, cr) = rgb_to_ycbcr(r, g, b);

            yb[(y * 8 + x) as usize]  = yc;
            cbb[(y * 8 + x) as usize] = cb;
            crb[(y * 8 + x) as usize] = cr;
        }
    }
}

fn encode_coefficient(coefficient: i32) -> (u8, u16)
{
    let mut magnitude = coefficient.abs() as u16;
    let mut num_bits  = 0u8;

    while magnitude > 0 {
        magnitude >>= 1;
        num_bits += 1;
    }

    let mask = (1 << num_bits as usize) - 1;

    let val  = if coefficient < 0 {
        (coefficient - 1) as u16 & mask
    } else {
        coefficient as u16 & mask
    };

    (num_bits, val)
}

fn rgb_to_ycbcr(r: u8, g: u8, b: u8) -> (u8, u8, u8)
{
    let r = r as f32;
    let g = g as f32;
    let b = b as f32;

    let y  =  0.299f32  * r + 0.587f32  * g + 0.114f32  * b;
    let cb = -0.1687f32 * r - 0.3313f32 * g + 0.5f32    * b + 128f32;
    let cr =  0.5f32    * r - 0.4187f32 * g - 0.0813f32 * b + 128f32;

    (y as u8, cb as u8, cr as u8)
}