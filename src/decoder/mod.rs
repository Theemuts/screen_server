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
    index: usize,
    trailing: u16,
    table: Table,
    size: u8,
    block: usize,
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
            decoded: vec![0; 192],
            index: 0,
            table: Table::Ld,
            size: 0,
            block: 0
        }
    }

    pub fn decode(&mut self) {
        self.init_buffer();

        /*
        println!("decode");
        println!("");
        println!("buffer {:b}", self.buffer);
        println!("trailing {}", self.trailing);
        println!("{}/{}", self.data_pointer, self.data.len());
        */

        self.set_luma_dc();
        self.decode_dc();
        self.set_luma_ac();
        self.decode_ac();
        self.block += 1;

        self.set_chroma_dc();
        self.decode_dc();
        self.set_chroma_ac();
        self.decode_ac();
        self.block += 1;

        self.set_chroma_dc();
        self.decode_dc();
        self.set_chroma_ac();
        self.decode_ac();
        self.block += 1;
        /*
        */

        for i in 0..self.decoded.len() {
            if (i%8) == 0 {
                println!("");
                if (i % 64) == 0 {
                    println!("");
                }
            }

            print!("{}, ", self.decoded[i]);
        }

    }

    pub fn decode_dc(&mut self) -> i32 {
        self.index = 1;

        if ((self.block % 3) != 0) & ((self.buffer >> 62) == 0x00)  {
            self.move_buffer(2);
            return 0;
        }

        let size = self.get_size();
        let value = self.get_bits(size);
        let decoded_value = decode_value(size, value);

        self.decoded[64 * self.block] = decoded_value;
        decoded_value
    }

    fn decode_ac(&mut self) {
        while self.index < 64 {
            self.size = 0;

            // Check for end of block.
            if ((self.block % 3) == 0) & ((self.buffer >> 60) == 0b1010) {
                self.move_buffer(4);
                break;
            } else if ((self.block % 3) != 0) & ((self.buffer >> 62) == 0)  {
                self.move_buffer(2);
                break;
            }

            self.parse_zero_runs();
            let symbol = self.get_size();

            let zero_run = (symbol & 0xF0) >> 4;
            let size = symbol & 0x0F;

            self.index += zero_run as usize;

            let value = self.get_bits(size);
            let decoded_value = decode_value(size, value);
            self.decoded[64 * self.block + UNZIGZAG[self.index] as usize] = decoded_value;

            self.index += 1;
        }
    }

    fn parse_zero_runs(&mut self) {
        while (self.buffer >> 56) as u8 == 0xF0 {
            self.move_buffer(8);
            self.index += 16;
        }
    }

    fn move_buffer(&mut self, shift: u64) {
        self.buffer <<= shift;
        self.trailing += shift as u16;

        if (self.trailing >= 8) & (self.data_pointer < self.data.len()) {
            self.buffer |= (self.data[self.data_pointer] as u64) << (self.trailing as u64 - 8);
            self.data_pointer += 1;
            self.trailing -= 8;
        }

    }

    fn get_size(&mut self) -> u8 {
        let mut code;

        while self.size == 0 {
            code = self.get_bits(1);
            self.move_pointer(code as u16);
        }

        self.size
    }

    pub fn init_buffer(&mut self) {
        let n = if self.data.len() < 8 { self.data.len() } else { 8 };

        for i in 0..n {
            self.buffer |= (self.data[i] as u64) << (56 - i*8);
        };

        self.data_pointer += n;
    }

    fn get_bits(&mut self, n_bits: u8) -> u32 {
        if n_bits > 32 {
            panic!("can get at most 4 byte at a time");
        }

        if n_bits == 0 {
            panic!("must get nonzero amount of bits");
        }

        let mask = MASK[n_bits as usize - 1];

        let return_val = ((self.buffer & mask) >> (64 - n_bits)) as u32;
        self.move_buffer(n_bits as u64);

        return_val
    }

    pub fn set_chroma_ac(&mut self) {
        self.table = Table::Ca;
        self.pointer = self.ca_tree.get(0, 0).clone();
    }

    pub fn set_chroma_dc(&mut self) {
        self.table = Table::Cd;
        self.pointer = self.cd_tree.get(0, 0).clone();
    }

    pub fn set_luma_ac(&mut self) {
        self.table = Table::La;
        self.pointer = self.la_tree.get(0, 0).clone();
    }

    pub fn set_luma_dc(&mut self) {
        self.table = Table::Ld;
        self.pointer = self.ld_tree.get(0, 0).clone();
    }

    pub fn move_pointer(&mut self, direction: u16) {
        let new_pnt = self.pointer.clone().get(1, direction).clone();

        match new_pnt {
            Tree::Leaf(v) => {
                self.size = v;

                match self.table {
                    Table::Ld => self.set_luma_ac(),
                    Table::La => self.set_luma_ac(),
                    _ => self.set_chroma_ac(),
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
        let x = if size == 0 { 0x0000 } else {(0xFFFF >> (16 - size))};
        - ((!value & x) as i32) // negative value
    } else {
        value as i32 // positive value
    }
}