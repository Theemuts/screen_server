pub mod tree;

use std::fmt::Debug;
use self::tree::Tree;

#[derive(Debug)]
pub struct Decoder {
    ld_tree: Box<Tree<u8>>,
    la_tree: Box<Tree<u8>>,
    cd_tree: Box<Tree<u8>>,
    ca_tree: Box<Tree<u8>>,


}

pub fn prepare_lookup_table(table: &Vec<(u8, u16)>)
    -> Vec<(u8, u16, Box<Tree<u8>>)> {
    let mut scv = Vec::with_capacity(table.len());


    for (i, value) in table.iter().enumerate() {
        if value.0 < 17 {
            scv.push((value.0, value.1, Box::new(Tree::Leaf(i as u8))));
        }
    }

    scv
}