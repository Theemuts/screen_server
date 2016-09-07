#[derive(Debug)]

// TODO: use UDP socket as sink rather than this...
pub struct Sink {
    n_blocks: usize,
    subpixels_per_macroblock: usize,
    pub data: Vec<Vec<u8>>,
    buffer: Vec<u8>,
    pub blocks: Vec<usize>
}

impl Sink {
    pub fn new(n_blocks: usize, subpixels_per_macroblock: usize) -> Self {
        Sink {
            n_blocks: n_blocks,
            subpixels_per_macroblock: subpixels_per_macroblock,
            data: Vec::with_capacity(n_blocks),
            buffer: Vec::with_capacity(subpixels_per_macroblock),
            blocks: Vec::with_capacity(n_blocks)
        }
    }

    pub fn get(&self) -> Vec<(usize, Vec<u8>)> {
        let mut result = Vec::with_capacity(self.data.len());

        for i in 0..self.data.len() {
            result.push((self.blocks[i], self.data[i].clone()));
        }

        result
    }

    pub fn write(&mut self, byte: u8) {
        self.buffer.push(byte);
    }

    pub fn new_block(&mut self, block: usize) {
        if self.buffer.len() > 0 {
            self.buffer.shrink_to_fit();
            self.data.push(self.buffer.clone());
        }

        self.buffer = Vec::with_capacity(self.subpixels_per_macroblock);
        self.blocks.push(block);
    }

    pub fn push_final_block(&mut self) {
        if self.buffer.len() > 0 {
            self.buffer.shrink_to_fit();
            self.data.push(self.buffer.clone());
        }
    }

    pub fn clear(&mut self) {
        self.data = Vec::with_capacity(self.n_blocks);
        self.buffer = Vec::with_capacity(self.subpixels_per_macroblock);
        self.blocks = Vec::with_capacity(self.n_blocks);
    }

    pub fn len(&self) -> usize {
        let mut s = 0;

        for v in &self.data {
            s += (*v).len();
        }

        s
    }

    pub fn block_and_len(&self, block: usize) -> Option<(usize, usize)> {
        if block < self.data.len() {
            Some((block, self.data[block].len()))
        } else {
            None
        }
    }
}