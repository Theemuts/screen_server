/*
This jpeg encoder implementation is largely copied from
https://raw.githubusercontent.com/PistonDevelopers/image/
fbe7a0c5dd90a69a3eb2bd18edd583f2156aa08f/src/jpeg/encoder.rs
*/

mod fdct;
mod entropy;
mod sink;

use super::context::Context;
use super::decoder::Tree;
use self::fdct::fdct;
use self::sink::Sink;
use self::entropy::build_huff_lut;
use super::util::get_data;

use num_iter::range_step;

// section K.1
// table K.1
static STD_LUMA_QTABLE: [u8; 64] = [
    16, 11, 10, 16,  24,  40,  51,  61,
    12, 12, 14, 19,  26,  58,  60,  55,
    14, 13, 16, 24,  40,  57,  69,  56,
    14, 17, 22, 29,  51,  87,  80,  62,
    18, 22, 37, 56,  68, 109, 103,  77,
    24, 35, 55, 64,  81, 104, 113,  92,
    49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103,  99,
];

// table K.2
static STD_CHROMA_QTABLE: [u8; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99,
    18, 21, 26, 66, 99, 99, 99, 99,
    24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99
];

// section K.3
// Code lengths and values for table K.3
static STD_LUMA_DC_CODE_LENGTHS: [u8; 16] = [
    0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
];

static STD_LUMA_DC_VALUES: [u8; 12] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x08, 0x09, 0x0A, 0x0B
];

// Code lengths and values for table K.4
static STD_CHROMA_DC_CODE_LENGTHS: [u8; 16] = [
    0x00, 0x03, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00
];

static STD_CHROMA_DC_VALUES: [u8; 12] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x08, 0x09, 0x0A, 0x0B
];

// Code lengths and values for table k.5
static STD_LUMA_AC_CODE_LENGTHS: [u8; 16] = [
    0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03,
    0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D
];

static STD_LUMA_AC_VALUES: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
    0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5,
    0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
    0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];

// Code lengths and values for table k.6
static STD_CHROMA_AC_CODE_LENGTHS: [u8; 16] = [
    0x00, 0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04,
    0x07, 0x05, 0x04, 0x04, 0x00, 0x01, 0x02, 0x77,
];
static STD_CHROMA_AC_VALUES: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0,
    0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34, 0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26,
    0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
    0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3,
    0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA,
    0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];

/// The permutation of dct coefficients.
static UNZIGZAG: [u8; 64] = [
    0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

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

    accumulator: u32,
    nbits: u8,

    size_accumulator: u64,
    sink: Sink
}

impl Encoder {
    pub fn new(width: isize, height: isize, bpp: isize) -> Self {
        let ld = build_huff_lut(&STD_LUMA_DC_CODE_LENGTHS, &STD_LUMA_DC_VALUES);
        let la = build_huff_lut(&STD_LUMA_AC_CODE_LENGTHS, &STD_LUMA_AC_VALUES);

        let cd = build_huff_lut(&STD_CHROMA_DC_CODE_LENGTHS, &STD_CHROMA_DC_VALUES);
        let ca = build_huff_lut(&STD_CHROMA_AC_CODE_LENGTHS, &STD_CHROMA_AC_VALUES);

        let mut tables = Vec::new();
        tables.extend(STD_LUMA_QTABLE.iter().map(|&v| v));
        tables.extend(STD_CHROMA_QTABLE.iter().map(|&v| v));

        let mut scv = Vec::new();

        for (i, value) in ld.iter().enumerate() {
            if value.0 < 17 {
                scv.push((value.0, value.1, Box::new(Tree::Leaf(i as u8))));
            }
        }

        let sc = Tree::generate_tree(&scv);
        let v = sc.get(6, 62);

        println!("{:?}", v);

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

            sink: Sink::new((height*width/256) as usize, 768) // 16*16*3 subpixels per macroblock
        }
    }

    pub fn initial_encode_rgb(&mut self, context: &Context) {
        let data = get_data(context.image_pointer);
        self.sink.clear();

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
                // RGB -> YCbCr

                self.sink.new_block(bl);

                for j in 0..2 {
                    for i in 0..2 {
                        x = x0 + 8*i;
                        y = y0 + 8*j;
                        // just base this on initial index.
                        let index = self.bpp * (y * self.width + x);
                        copy_blocks_ycbcr(data, index, self.width, self.bpp, self.size, &mut yblock, &mut cb_block, &mut cr_block);

                        // Level shift and fdct
                        // Coeffs are scaled by 8
                        fdct(&yblock, &mut dct_yblock);
                        fdct(&cb_block, &mut dct_cb_block);
                        fdct(&cr_block, &mut dct_cr_block);

                        // Quantization
                        for i in 0usize..64 {
                            dct_yblock[i]   = ((dct_yblock[i] / 8)   as f32 / self.tables[i] as f32).round() as i32;
                            dct_cb_block[i] = ((dct_cb_block[i] / 8) as f32 / self.tables[64..][i] as f32).round() as i32;
                            dct_cr_block[i] = ((dct_cr_block[i] / 8) as f32 / self.tables[64..][i] as f32).round() as i32;
                        }

                        self.write_block(&dct_yblock, &ld, &la);
                        self.write_block(&dct_cb_block, &cd, &ca);
                        self.write_block(&dct_cr_block, &cd, &ca);
                    }
                }
                bl += 1;
                self.write_final_bits();
            }
        }
    }

    pub fn sink_size(&self) -> f64 {
        self.sink.len() as f64
    }

    pub fn update_encode_rgb(&mut self, context: &Context) -> usize {
        let data = get_data(context.image_pointer);

        self.sink.clear();
        let mut sent = 0;

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

        for error in &context.errors {
            let (err, block) = *error;
            if err == 0 {
                // errors are sorted, 0 => no more changed blocks
                // TODO: don't add zero-error blocks to the error vec?
                break
            }
            self.sink.new_block(block);

            // Speed up with lookup table?

            let n_blocks_x: isize = if self.width % self.macroblock_size == 0 {
                self.width / self.macroblock_size
            } else {
                (self.width+8) / self.macroblock_size
            };

            // Todo: find out why n errors sent stays constant rather than go to 0 when video playback paused.

            let block_mod: isize = block as isize % n_blocks_x;
            let x0: isize = 64*block_mod;
            let y0: isize = (block as isize - block_mod)/n_blocks_x;

            for y in range_step(y0, y0+16, 8) {
                for x in range_step(x0, x0+64, 32) { // 2 * 4 * 8
                    let index = self.bpp * y * self.width + x;
                    copy_blocks_ycbcr(data, index, self.width, self.bpp, self.size, &mut yblock, &mut cb_block, &mut cr_block);

                    // Level shift and fdct
                    // Coeffs are scaled by 8
                    fdct(&yblock, &mut dct_yblock);
                    fdct(&cb_block, &mut dct_cb_block);
                    fdct(&cr_block, &mut dct_cr_block);

                    // Quantization
                    for i in 0usize..64 {
                        dct_yblock[i]   = ((dct_yblock[i] / 8)   as f32 / self.tables[i] as f32).round() as i32;
                        dct_cb_block[i] = ((dct_cb_block[i] / 8) as f32 / self.tables[64..][i] as f32).round() as i32;
                        dct_cr_block[i] = ((dct_cr_block[i] / 8) as f32 / self.tables[64..][i] as f32).round() as i32;
                    }

                    self.write_block(&dct_yblock, &ld, &la);
                    self.write_block(&dct_cb_block, &cd, &ca);
                    self.write_block(&dct_cr_block, &cd, &ca);
                }
            }

            self.write_final_bits();
            sent += 1;
        }

        sent
    }

    fn huffman_encode(&mut self, val: u8, table: &[(u8, u16)]) {
        let (size, code) = table[val as usize];

        if size > 16 {
            panic!("bad huffman value");
        }

        self.write_bits(code, size)
    }

    fn write_block(
        &mut self,
        block: &[i32],
        dctable: &[(u8, u16)],
        actable: &[(u8, u16)]) -> i32 {

        // Differential DC encoding
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
                    // Final element. Write f(0x00) to indicate end of block
                    self.huffman_encode(0x00, actable);
                    break
                }

                zero_run += 1;
            } else {
                while zero_run > 15 {
                    // Write f(0xF0) for every 16 consecutive zeros
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

    fn write_bits(&mut self, bits: u16, size: u8) {
        if size == 0 {
            return
        }

        self.accumulator |= (bits as u32) << (32 - (self.nbits + size)) as usize;
        self.nbits += size;

        while self.nbits >= 8 {
            let byte = (self.accumulator & (0xFFFFFFFFu32 << 24)) >> 24;
            self.sink.write(byte as u8);

            if byte == 0xFF {
                self.sink.write(0x00);
            }

            self.nbits -= 8;
            self.accumulator <<= 8;
        }
    }

    fn write_final_bits(&mut self) {
        if self.nbits == 0 {
            self.accumulator = 0;
            return
        }

        while self.nbits >= 8 {
            let byte = (self.accumulator & (0xFFFFFFFFu32 << 24)) >> 24;
            self.sink.write(byte as u8);

            if byte == 0xFF {
                self.sink.write(0x00);
            }

            self.nbits -= 8;
            self.accumulator <<= 8;
        }

        if self.nbits != 0 {
            let byte = (self.accumulator & (0xFFFFFFFFu32 << 24)) >> 24;
            self.sink.write(byte as u8);

            if byte == 0xFF {
                self.sink.write(0x00);
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
                     crb: &mut [u8; 64]) {

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

fn encode_coefficient(coefficient: i32) -> (u8, u16) {
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

fn rgb_to_ycbcr(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r = r as f32;
    let g = g as f32;
    let b = b as f32;

    let y  =  0.299f32  * r + 0.587f32  * g + 0.114f32  * b;
    let cb = -0.1687f32 * r - 0.3313f32 * g + 0.5f32    * b + 128f32;
    let cr =  0.5f32    * r - 0.4187f32 * g - 0.0813f32 * b + 128f32;

    (y as u8, cb as u8, cr as u8)
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
