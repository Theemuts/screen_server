mod tree;

use std::rc::Rc;

use self::tree::Tree;
use super::entropy::build_huff_lut;
use super::tables::*;

#[derive(Debug)]
pub struct Decoder<'a> {
    ld_tree: Rc<Box<Tree<u8>>>,
    la_tree: Rc<Box<Tree<u8>>>,
    cd_tree: Rc<Box<Tree<u8>>>,
    ca_tree: Rc<Box<Tree<u8>>>,

    pointer: Tree<u8>,
    data: &'a Vec<u8>,
    decoded: Vec<i32>,
    data_pointer: usize,
    buffer: u64,
    trailing: u16,
    table: Table,
    size: u8
}

#[derive(Debug)]
enum Table {
    Ld,
    La,
    Cd,
    Ca
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a Vec<u8>) -> Self {
        let ld = build_huff_lut(&STD_LUMA_DC_CODE_LENGTHS, &STD_LUMA_DC_VALUES);
        let la = build_huff_lut(&STD_LUMA_AC_CODE_LENGTHS, &STD_LUMA_AC_VALUES);

        let cd = build_huff_lut(&STD_CHROMA_DC_CODE_LENGTHS, &STD_CHROMA_DC_VALUES);
        let ca = build_huff_lut(&STD_CHROMA_AC_CODE_LENGTHS, &STD_CHROMA_AC_VALUES);

        let ld_lookup = prepare_lookup_table(&ld);
        let la_lookup = prepare_lookup_table(&la);
        let cd_lookup = prepare_lookup_table(&cd);
        let ca_lookup = prepare_lookup_table(&ca);

        let ld_tree = Tree::generate_tree(&ld_lookup);

        Decoder {
            ld_tree: ld_tree.clone(),
            la_tree: Tree::generate_tree(&la_lookup),
            cd_tree: Tree::generate_tree(&cd_lookup),
            ca_tree: Tree::generate_tree(&ca_lookup),

            pointer: ld_tree.get(0, 0).clone(),
            data: data,
            buffer: 0u64,
            data_pointer: 0usize,
            trailing: 0u16,
            decoded: Vec::with_capacity(768*3),
            table: Table::Ld,
            size: 0
        }
    }

    pub fn decode(&mut self) {
        self.init_buffer();

        self.decode_dc();

        while self.decoded.len() < 64 {
            self.decode_ac();
        }
    }

    fn decode_dc(&mut self) {
        self.get_size();

        let size = self.size;
        let value = self.get_bits(size);
        let decoded_value = decode_value(size, value);

        self.decoded.push(decoded_value);
    }

    fn decode_ac(&mut self) {
        self.size = 0;

        self.parse_zero_runs();

        self.get_size();
        let sz = self.size;

        let zero_run = (sz & 240) >> 4;
        for _ in 0..zero_run {
            self.decoded.push(0);
        }

        let size = sz & 15;
        let value = self.get_bits(size);
        let decoded_value = decode_value(size, value);

        self.decoded.push(decoded_value);
    }

    fn parse_zero_runs(&mut self) {
        while (self.buffer >> 56) as u8 == 0xF0 {
            self.buffer <<= 8;

            if self.data_pointer < self.data.len() {
                self.buffer |= self.data[self.data_pointer] as u64;
                self.data_pointer += 1;
            } else {
                self.buffer |= 0u64;
            }

            for _ in 0..16 {
                self.decoded.push(0);
            }
        }
    }

    fn get_size(&mut self) {
        let mut code;
        while self.size == 0 {
            code = self.get_bits(1);
            self.move_pointer(code as u16);
        }
    }

    fn init_buffer(&mut self) {
        self.buffer =   ((self.data[self.data_pointer] as u64) << 56) |
                        ((self.data[self.data_pointer + 1] as u64) << 48) |
                        ((self.data[self.data_pointer + 2] as u64) << 40) |
                        ((self.data[self.data_pointer + 3] as u64) << 32) |
                        ((self.data[self.data_pointer + 4] as u64) << 24) |
                        ((self.data[self.data_pointer + 5] as u64) << 16) |
                        ((self.data[self.data_pointer + 6] as u64) << 8) |
                        self.data[self.data_pointer + 7] as u64;
        self.data_pointer += 8;
    }

    fn get_bits(&mut self, n_bits: u8) -> u32 {
        if n_bits > 32 {
            panic!("can get at most 4 byte at a time");
        }

        let mut mask = 0x8000000000000000u64;
        for i in 1..n_bits {
            mask |= (mask >> i)
        }

        let return_val = ((self.buffer & mask) >> (64 - n_bits)) as u32;
        self.buffer <<= n_bits as u32;

        //println!("trailing: {} {} {}", self.trailing, self.data_pointer, self.data.len());
        self.trailing += n_bits as u16;

        while (self.trailing >= 8) & (self.data_pointer < self.data.len()) {
            self.buffer |= ((self.data[self.data_pointer] as u64) << (self.trailing - 8) as u64);
            self.data_pointer += 1;
            self.trailing -= 8;
        }

        return_val
    }

    pub fn move_pointer(&mut self, direction: u16) {
        let new_pnt = self.pointer.clone().get(1, direction).clone();

        match new_pnt {
            Tree::Leaf(v) => {
                self.size = v;

                match self.table {
                    Table::Ld => {
                        self.table = Table::La;
                        self.pointer = self.la_tree.get(0, 0).clone();
                    },
                    Table::La => {
                        self.pointer = self.la_tree.get(0, 0).clone();
                    },
                    Table::Cd => {
                        self.table = Table::Ca;
                        self.pointer = self.ca_tree.get(0, 0).clone();
                    },
                    Table::Ca => {
                        self.pointer = self.ca_tree.get(0, 0).clone();
                    },
                }
            },
            Tree::None => panic!("invalid code"),
            _ => self.pointer = new_pnt
        }
    }
}

fn prepare_lookup_table(table: &Vec<(u8, u16)>)
    -> Vec<(u8, u16, Rc<Box<Tree<u8>>>)> {
    let mut scv = Vec::with_capacity(table.len());

    for (i, value) in table.iter().enumerate() {
        if value.0 < 17 {
            scv.push((value.0, value.1, Rc::new(Box::new(Tree::Leaf(i as u8)))));
        }
    }

    scv
}

fn decode_value(size: u8, value: u32) -> i32 {
    let positive_mask = 1 << (size - 1);

    if value & positive_mask == 0 {
        - ((!value & (0xFF >> 8 - size)) as i32) // negative value
    } else {
        value as i32 // positive value
    }
}