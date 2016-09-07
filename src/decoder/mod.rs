mod ifdct;
mod tree;

use std::rc::Rc;

use std::io;
use std::io::prelude::*;
use std::io::BufWriter;
use std::fs::File;

use self::tree::Tree;
use super::entropy::build_huff_lut;
use super::tables::*;
use super::encoder::sink::Sink;

#[derive(Debug)]
pub struct Decoder {
    ld_tree: Rc<Tree<u8>>,
    la_tree: Rc<Tree<u8>>,
    cd_tree: Rc<Tree<u8>>,
    ca_tree: Rc<Tree<u8>>,

    pointer: Rc<Tree<u8>>,
    decoded: Vec<(usize, Vec<i32>)>,
    current_decode: Vec<i32>,
    data_pointer: usize,
    buffer: u32,
    index: usize,
    trailing: u16,
    width: usize,
    height: usize,
    table: Table,
    size: u8,
    block: usize,
}

#[derive(Debug, PartialEq)]
enum Table {
    Ld,
    La,
    Cd,
    Ca
}

impl Decoder {
    pub fn new(width: usize, height: usize) -> Self {
        let ld = build_huff_lut(&STD_LUMA_DC_CODE_LENGTHS, &STD_LUMA_DC_VALUES);
        let la = build_huff_lut(&STD_LUMA_AC_CODE_LENGTHS, &STD_LUMA_AC_VALUES);

        let cd = build_huff_lut(&STD_CHROMA_DC_CODE_LENGTHS, &STD_CHROMA_DC_VALUES);
        let ca = build_huff_lut(&STD_CHROMA_AC_CODE_LENGTHS, &STD_CHROMA_AC_VALUES);

        let ld_lookup = prepare_lookup_table(&ld);
        let la_lookup = prepare_lookup_table(&la);
        let cd_lookup = prepare_lookup_table(&cd);
        let ca_lookup = prepare_lookup_table(&ca);

        //let _ = generate_java_tests(&ld_lookup, "StdDCLuminance");
        //let _ = generate_java_tests(&la_lookup, "StdACLuminance");
        //let _ = generate_java_tests(&cd_lookup, "StdDCChrominance");
        //let _ = generate_java_tests(&ca_lookup, "StdACChrominance");

        let ld_tree = Tree::generate_tree(&ld_lookup);

        let decoded = Vec::new();

        Decoder {
            ld_tree: ld_tree.clone(),
            la_tree: Tree::generate_tree(&la_lookup),
            cd_tree: Tree::generate_tree(&cd_lookup),
            ca_tree: Tree::generate_tree(&ca_lookup),

            pointer: ld_tree,
            buffer: 0u32,
            data_pointer: 0usize,
            trailing: 0u16,
            decoded: decoded,
            current_decode: vec![0; 768],
            index: 0,
            table: Table::Ld,
            size: 0,
            block: 0,
            height: height,
            width: width
        }
    }

    pub fn decode(&mut self, sink: &Sink) {
        // Clear currently decoded data.
        self.decoded = Vec::with_capacity(sink.blocks.len());

        for i in 0..sink.blocks.len() {
            let macroblock = &sink.data[i];
            let block = sink.blocks[i];

            self.decode_macroblock(macroblock);
            self.decoded.push((block, self.current_decode.clone()));
            self.current_decode = vec![0; 768];
        }

        let result = self.dct_to_rgb(sink);
        let _ = self.store_client_state(&result);
    }

    fn dct_to_rgb(&mut self, sink: &Sink) -> Vec<u8> {
        let mut result = vec![0u8; self.height * self.width * 3];

        for j in 0..sink.blocks.len() {
            let (block, v) = self.decoded[j].clone();
            //println!("{}", block);

            for n in 0..4 {
                let mut dct_yblock = [0i16; 64];
                let mut dct_cb_block = [0i16; 64];
                let mut dct_cr_block = [0i16; 64];

                let mut yblock = [0u8; 64];
                let mut cb_block = [0u8; 64];
                let mut cr_block = [0u8; 64];

                for i in 0..64 {
                    dct_yblock[i] = v[192 * n + i] as i16;
                    dct_cb_block[i] = v[192 * n + 64 + i] as i16;
                    dct_cr_block[i] = v[192 * n + 128 + i] as i16;
                }

                ifdct::dequantize_and_idct_block(&dct_yblock, &STD_LUMA_QTABLE_U16,  &mut yblock);
                ifdct::dequantize_and_idct_block(&dct_cb_block, &STD_CHROMA_QTABLE_U16, &mut cb_block);
                ifdct::dequantize_and_idct_block(&dct_cr_block, &STD_CHROMA_QTABLE_U16, &mut cr_block);

                for i in 0..64 {
                    let index = get_index(block, n, self.width, i);

                    let (r, g, b) = ycbcr_to_rgb(yblock[i], cb_block[i], cr_block[i]);
                    result[index] = r;
                    result[index + 1] = g;
                    result[index + 2] = b;

                    //println!("{} {} {}",r,g,b);
                }
            }
        }

        result
    }

    pub fn store_client_state(&self, data: &Vec<u8>) -> io::Result<()> {
        let mut buffer = BufWriter::new(try!(File::create("foo2.txt")));
        for x in data {
            try!(write!(buffer, "{} ", x));
        }

        try!(buffer.flush());

        Ok(())
    }

    fn decode_macroblock(&mut self, macroblock: &Vec<u8>) {
        self.init_buffer(macroblock);
        self.block = 0;

        self.get_bits(10, macroblock);

        for _ in 0..4 {
            self.decode_luma_block(macroblock);
            for _ in 0..2 { self.decode_chroma_block(macroblock); }
        }

        /*for i in 0..self.current_decode.len() {
            if (i % 8) == 0 {
                println!("");
                if (i % 64) == 0 {
                    println!("");
                }
            }

            print!("{}, ", self.current_decode[i]);
        }*/
    }

    fn decode_luma_block(&mut self, macroblock: &Vec<u8>) {
        self.set_luma_dc();
        self.decode_dc(macroblock);
        self.set_luma_ac();
        self.decode_ac(macroblock);
        self.block += 1;
    }

    fn decode_chroma_block(&mut self, macroblock: &Vec<u8>) {
        self.set_chroma_dc();
        self.decode_dc(macroblock);
        self.set_chroma_ac();
        self.decode_ac(macroblock);
        self.block += 1;
    }

    fn decode_dc(&mut self, macroblock: &Vec<u8>) -> i32 {
        self.index = 1;
        self.size = 0;

        if (self.buffer >> 30) == 0x00  {
            self.move_buffer(2, macroblock);
            return 0;
        }

        let size = self.get_symbol(macroblock);
        let value = self.get_bits(size, macroblock);
        let decoded_value = decode_value(size, value);

        self.current_decode[64 * self.block] = decoded_value;
        decoded_value
    }

    fn decode_ac(&mut self, macroblock: &Vec<u8>) {
        while self.index < 64 {
            self.size = 0;

            // Check for end of block.
            if (self.table == Table::La) & ((self.buffer >> 28) == 0b1010) {
                self.move_buffer(4, macroblock);
                break;
            } else if (self.table == Table::Ca) & ((self.buffer >> 30) == 0)  {
                self.move_buffer(2, macroblock);
                break;
            }

            self.parse_zero_runs(macroblock);

            let symbol = self.get_symbol(macroblock);
            let size = symbol & 0x0F;

            self.index += ((symbol & 0xF0) >> 4) as usize;

            let value = self.get_bits(size, macroblock);
            let decoded_value = decode_value(size, value);
            self.current_decode[64 * self.block + UNZIGZAG[self.index] as usize] = decoded_value;

            self.index += 1;
        }
    }

    fn parse_zero_runs(&mut self, macroblock: &Vec<u8>) {
        if self.table == Table::La {
            while (self.buffer >> 21) as u16 == 2041 { // L_AC 0xF0 => (11, 2041)
                self.move_buffer(11, macroblock);
                self.index += 16;
            }
        } else {
            while (self.buffer >> 22) as u16 == 1018 { // C_AC 0xF0 => (10, 1018)
                self.move_buffer(10, macroblock);
                self.index += 16;
            }
        }
    }

    fn move_buffer(&mut self, shift: u32, macroblock: &Vec<u8>) {
        self.buffer <<= shift;
        self.trailing += shift as u16;

        if (self.trailing >= 8) & (self.data_pointer < macroblock.len()) {
            self.buffer |= (macroblock[self.data_pointer] as u32) << (self.trailing as u32 - 8);
            self.data_pointer += 1;
            self.trailing -= 8;
        }

    }

    fn get_symbol(&mut self, macroblock: &Vec<u8>) -> u8 {
        let mut code;

        while self.size == 0 {
            code = self.get_bits(1, macroblock);
            self.move_pointer(code as u16);
        }

        self.size
    }

    fn init_buffer(&mut self, macroblock: &Vec<u8>) {
        self.trailing  = 0;
        self.data_pointer = if macroblock.len() < 4 { macroblock.len() } else { 4 };

        for i in 0..self.data_pointer {
            self.buffer |= (macroblock[i] as u32) << (24 - i*8);
        }
    }

    fn get_bits(&mut self, n_bits: u8, macroblock: &Vec<u8>) -> u32 {
        if n_bits > 32 {
            panic!("can get at most 4 byte at a time");
        }

        if n_bits == 0 {
            panic!("must get nonzero amount of bits");
        }

        let mask = MASK[n_bits as usize - 1];
        let return_val = ((self.buffer & mask) >> (32 - n_bits)) as u32;
        self.move_buffer(n_bits as u32, macroblock);

        return_val
    }

    fn set_chroma_ac(&mut self) {
        self.table = Table::Ca;
        self.pointer = self.ca_tree.clone();
    }

    fn set_chroma_dc(&mut self) {
        self.table = Table::Cd;
        self.pointer = self.cd_tree.clone();
    }

    fn set_luma_ac(&mut self) {
        self.table = Table::La;
        self.pointer = self.la_tree.clone();
    }

    fn set_luma_dc(&mut self) {
        self.table = Table::Ld;
        self.pointer = self.ld_tree.clone();
    }

    fn move_pointer(&mut self, direction: u16) {
        let new_pnt = self.pointer.get(1, direction);

        match *new_pnt {
            Tree::Leaf(v) => {
                self.size = v;

                match self.table {
                    Table::La => self.set_luma_ac(),
                    Table::Ca => self.set_chroma_ac(),
                    _ => (),
                }
            },
            Tree::None => panic!("invalid code"),
            _ => self.pointer = new_pnt
        }
    }
}

fn prepare_lookup_table(table: &Vec<(u8, u16)>)
    -> Vec<(u8, u16, Rc<Tree<u8>>)> {
    let mut scv = Vec::with_capacity(table.len());

    for (i, value) in table.iter().enumerate() {
        if value.0 < 17 {
            scv.push((value.0, value.1, Rc::new(Tree::Leaf(i as u8))));
        }
    }

    println!("");

    scv
}

fn decode_value(size: u8, value: u32) -> i32 {
    let positive_mask = 1 << (size - 1);

    if value & positive_mask == 0 {
        let x = if size == 0 { 0 } else {(0xFFFF >> (16 - size))};
        - ((!value & x) as i32) // negative value
    } else {
        value as i32 // positive value
    }
}

fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let y = y as f32;
    let cb = cb as f32;
    let cr = cr as f32;

    let r  =  y + 1.402 * (cr - 128f32);
    let g = y - 0.344136 * (cb - 128f32) - 0.714136 * (cr - 128f32);
    let b =  y + 1.772 * (cb - 128f32);

    (r as u8, g as u8, b as u8)
}

fn get_index(macroblock: usize, subblock: usize, width: usize, index: usize) -> usize {
    let n_blocks_x = width / 16;

    let block_x = macroblock % n_blocks_x;
    let block_y = macroblock / n_blocks_x;

    let ind_x = block_x * 16 + (subblock % 2) * 8 + (index % 8);
    let ind_y = block_y * 16 + (subblock / 2) * 8 + index / 8;

    3 * (ind_y * width + ind_x)
}

// This function generates a test for a huffman tree implemented for the Android client.
fn generate_java_tests(lookup_table: &Vec<(u8, u16, Rc<Tree<u8>>)>, name: &'static str) -> io::Result<()> {
    let name2 = format!("Huffman{}Test.java", name);

    let mut buffer = BufWriter::new(try!(File::create(&name2)));

    try!(write!(buffer, "package com.theemuts.remotedesktop;\n\n"));

    try!(write!(buffer, "import com.theemuts.remotedesktop.huffman.HuffmanNode;\n"));
    try!(write!(buffer, "import com.theemuts.remotedesktop.huffman.IHuffmanNode;\n"));
    try!(write!(buffer, "import com.theemuts.remotedesktop.huffman.JPEGHuffmanTable;\n\n"));

    try!(write!(buffer, "import org.junit.Test;\n"));
    try!(write!(buffer, "import static org.junit.Assert.assertEquals;\n"));
    try!(write!(buffer, "import static org.junit.Assert.assertTrue;\n\n"));

    try!(write!(buffer, "/* This file has been auto-generated. Do not change it. */\n\n"));

    try!(write!(buffer, "public class Huffman{}Test {{\n", &name));
    try!(write!(buffer, "    @Test\n"));
    try!(write!(buffer, "    public void tree_isCorrect() throws Exception {{\n"));
    try!(write!(buffer, "        IHuffmanNode tree = new HuffmanNode(JPEGHuffmanTable.{});\n", &name));
    try!(write!(buffer, "        IHuffmanNode pointer;\n"));

    for val in lookup_table {
        let (size, value, ref leaf) = *val;

        let index = leaf.unwrap();
        try!(write!(buffer, "\n"));

        let direction = ((value >> (size - 1)) & 1) == 1;
        try!(write!(buffer, "        pointer = tree.move({});\n", direction));

        for i in 1..size {
            let direction = ((value >> (size - 1 - i)) & 1) == 1;
            try!(write!(buffer, "        pointer = pointer.move({});\n", direction));
        }

        try!(write!(buffer, "        assertTrue(pointer.isLeaf());\n"));
        try!(write!(buffer, "        assertEquals(pointer.getSize(), {});\n", index & 0x0F));
        try!(write!(buffer, "        assertEquals(pointer.getZeroRun(), {});\n", (index & 0xF0) >> 4));
    }

    try!(write!(buffer, "    }}\n"));
    try!(write!(buffer, "}}\n"));

    try!(buffer.flush());

    Ok(())
}

#[test]
fn index() {
    assert_eq!(get_index(0, 0, 64, 0), 0);
    assert_eq!(get_index(0, 1, 64, 0), 24);
    assert_eq!(get_index(0, 2, 64, 0), 1536);
    assert_eq!(get_index(0, 3, 64, 0), 1560);

    assert_eq!(get_index(0, 0, 64, 1), 3);
    assert_eq!(get_index(0, 1, 64, 1), 27);
    assert_eq!(get_index(0, 2, 64, 1), 1539);
    assert_eq!(get_index(0, 3, 64, 1), 1563);

    assert_eq!(get_index(0, 0, 64, 8), 192);
    assert_eq!(get_index(0, 1, 64, 8), 216);
    assert_eq!(get_index(0, 2, 64, 8), 1728);
    assert_eq!(get_index(0, 3, 64, 8), 1752);

    assert_eq!(get_index(1, 0, 64, 0), 48);
    assert_eq!(get_index(1, 1, 64, 0), 72);
    assert_eq!(get_index(1, 2, 64, 0), 1584);
    assert_eq!(get_index(1, 3, 64, 0), 1608);

    assert_eq!(get_index(1, 0, 64, 1), 51);
    assert_eq!(get_index(1, 1, 64, 1), 75);
    assert_eq!(get_index(1, 2, 64, 1), 1587);
    assert_eq!(get_index(1, 3, 64, 1), 1611);

    assert_eq!(get_index(1, 0, 64, 8), 240);
    assert_eq!(get_index(1, 1, 64, 8), 264);
    assert_eq!(get_index(1, 2, 64, 8), 1776);
    assert_eq!(get_index(1, 3, 64, 8), 1800);

    assert_eq!(get_index(4, 0, 64, 0), 3072);
    assert_eq!(get_index(4, 1, 64, 0), 3096);
    assert_eq!(get_index(4, 2, 64, 0), 4608);
    assert_eq!(get_index(4, 3, 64, 0), 4632);

    assert_eq!(get_index(4, 0, 64, 8), 3264);
    assert_eq!(get_index(4, 1, 64, 8), 3288);
    assert_eq!(get_index(4, 2, 64, 8), 4800);
    assert_eq!(get_index(4, 3, 64, 8), 4824);
}