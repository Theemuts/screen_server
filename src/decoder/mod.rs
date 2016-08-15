mod tree;

use std::rc::Rc;

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

    pointer: Tree<u8>,
    decoded: Vec<Vec<i32>>,
    current_decode: Vec<i32>,
    data_pointer: usize,
    buffer: u32,
    index: usize,
    trailing: u16,
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
    pub fn new() -> Self {
        let ld = build_huff_lut(&STD_LUMA_DC_CODE_LENGTHS, &STD_LUMA_DC_VALUES);
        let la = build_huff_lut(&STD_LUMA_AC_CODE_LENGTHS, &STD_LUMA_AC_VALUES);

        let cd = build_huff_lut(&STD_CHROMA_DC_CODE_LENGTHS, &STD_CHROMA_DC_VALUES);
        let ca = build_huff_lut(&STD_CHROMA_AC_CODE_LENGTHS, &STD_CHROMA_AC_VALUES);

        let ld_lookup = prepare_lookup_table(&ld);
        let la_lookup = prepare_lookup_table(&la);
        let cd_lookup = prepare_lookup_table(&cd);
        let ca_lookup = prepare_lookup_table(&ca);

        let ld_tree = Tree::generate_tree(&ld_lookup);

        let decoded = Vec::new();

        Decoder {
            ld_tree: ld_tree.clone(),
            la_tree: Tree::generate_tree(&la_lookup),
            cd_tree: Tree::generate_tree(&cd_lookup),
            ca_tree: Tree::generate_tree(&ca_lookup),

            pointer: ld_tree.get(0, 0).clone(),
            buffer: 0u32,
            data_pointer: 0usize,
            trailing: 0u16,
            decoded: decoded,
            current_decode: vec![0; 768],
            index: 0,
            table: Table::Ld,
            size: 0,
            block: 0
        }
    }

    pub fn decode(&mut self, sink: &Sink) {
        // Clear currently decoded data.
        self.decoded.clear();

        for macroblock in &sink.data {
            self.decode_macroblock(macroblock);
            self.decoded.push(self.current_decode.clone());
            self.current_decode = vec![0; 768];
        }
    }

    pub fn decode_macroblock(&mut self, macroblock: &Vec<u8>) {
        self.init_buffer(macroblock);
        self.block = 0;

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
        self.pointer = self.ca_tree.get(0, 0).clone();
    }

    fn set_chroma_dc(&mut self) {
        self.table = Table::Cd;
        self.pointer = self.cd_tree.get(0, 0).clone();
    }

    fn set_luma_ac(&mut self) {
        self.table = Table::La;
        self.pointer = self.la_tree.get(0, 0).clone();
    }

    fn set_luma_dc(&mut self) {
        self.table = Table::Ld;
        self.pointer = self.ld_tree.get(0, 0).clone();
    }

    fn move_pointer(&mut self, direction: u16) {
        let new_pnt = self.pointer.get(1, direction).clone();

        match new_pnt {
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