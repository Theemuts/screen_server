use super::xinterface;
use super::x11::xlib;

#[derive(Debug)]
pub struct Context {
    image_pointer: Option<*mut xlib::XImage>,
    display: *mut xlib::Display,
    window: u64,
    width: u32,
    height: u32,
    offset_x: i32,
    offset_y: i32,
    client_state: Vec<u8>,
    bbp: u32,
    block_size: u32,
    n_blocks: usize,
    n_blocks_x: u32,
    n_blocks_y: u32,
    errors: Vec<(i64, usize)>,
    block_table: Vec<usize>
}

impl Context {
    pub fn new(width: u32, offset_x: i32, height: u32, offset_y: i32) -> Self {
        if (height % 8 != 0) | (width % 8 != 0) {
            panic!("height and width must be divisible by 8")
        }

        let display = xinterface::open_display();

        let block_size = 8;
        let n_blocks_x = width / block_size;
        let n_blocks_y = height / block_size;
        let n_blocks = (n_blocks_x * n_blocks_y) as usize;

        let n = (width * height * 3) as usize;

        let mut c = Context {
            image_pointer: None,
            display: display,
            window: xinterface::get_root_window(display),
            width: width,
            height: height,
            offset_x: offset_x,
            offset_y: offset_y,
            client_state: vec![0u8; (height*width*3) as usize],
            bbp: 3,
            block_size: 8,
            n_blocks: n_blocks,
            n_blocks_x: n_blocks_x,
            n_blocks_y: n_blocks_y,
            errors: vec![(0i64, 0usize); n_blocks],
            block_table: vec![0usize; n]
        };

        c.generate_block_lookup_table();
        c.get_new_screenshot();
        c.set_initial_state();
        c
    }

    pub fn get_new_screenshot(&mut self) {
        // Delete old (is None if uninitialized)
        if let Some(im_pointer) = self.image_pointer {
            xinterface::destroy_image(im_pointer);
            self.image_pointer = None;
        }

        let image = xinterface::get_image(self.display, self.window,
                                          self.width, self.offset_x,
                                          self.height, self.offset_y);

        self.image_pointer = Some(image);
    }

    pub fn set_initial_state(&mut self) {
        xinterface::copy_image(self.image_pointer.unwrap(), &mut self.client_state, self.width, self.height);
    }

    pub fn close(&self) {
        xinterface::close_display(self.display);
    }

    pub fn set_block_errors(&mut self) {
        let im_pointer = self.image_pointer.unwrap();

        // Define here for speed
        let data;
        let mut r;
        let mut g;
        let mut b;
        let mut d_b;
        let mut d_g;
        let mut d_r;
        let mut bl;

        // Set data to data in XImage. Much faster than XGetPixel
        unsafe {
            data = (*im_pointer).data;
        }

        // Reset errors.
        for n in 0..self.errors.len() {
            self.errors[n] = (0, n);
        }

        let mut ind = 0;
        let mut i = 0;
        let lim = (4*self.height * self.width) as isize;

        // Get all pixels and errors
        while ind < lim {
            unsafe {
                b = *data.offset(ind) as u8;
                g = *data.offset(ind + 1) as u8;
                r = *data.offset(ind + 2) as u8;
            }

            d_r = r as i64 - self.client_state[i] as i64;
            d_g = g as i64 - self.client_state[i + 1] as i64;
            d_b = b as i64 - self.client_state[i + 2] as i64;

            bl = self.block_table[i];
            self.errors[bl].0 += d_r * d_r + d_g * d_g + d_b * d_b;

            ind += 4;
            i += 3;
        }

        // Sort errors for consumption
        self.errors.sort_by(|a, b| b.cmp(a));
    }

    pub fn print_errors(&self) {
        let mut zero = 0;
        let mut max = 0;
        let mut total = 0;
        let mut var = 0;

        let ref errors = self.errors;

        for i in 0..errors.len() {
            match errors[i].0 {
                0 => zero += 1,
                e if e > max => max = e,
                _ => ()
            }

            total += errors[i].0
        }

        let av = total / errors.len() as i64;

        for i in 0..errors.len() {
            let mut add = errors[i].0 - av;
            add *= add;
            var += add;
        }

        for i in 0..10 {
            let j = (((i + 1) * errors.len() / 10) - 1) as usize;
            println!("{1}: {0}", errors[j].0, errors[j].1);
        }

        let std = f64::sqrt((var / errors.len() as i64) as f64);

        println!("Zero: {}/{}", zero, errors.len());
        println!("Max:  {}", max);
        println!("Av:   {}", av);
        println!("StD:  {}", std);
        println!("nE:  {}", errors.len());
    }

    fn generate_block_lookup_table(&mut self) {
        for n in 0..self.width * self.height * self.bbp {
            let mut scaled_n = n / self.bbp;
            scaled_n = (scaled_n - scaled_n % self.block_size) / self.block_size;
            let block_x = scaled_n % self.n_blocks_x;
            scaled_n = (scaled_n - block_x) / self.n_blocks_x;
            let block_y = (scaled_n - scaled_n % self.block_size) / self.block_size;
            let block = (block_y * self.n_blocks_x + block_x) as usize;
            self.block_table[n as usize] = block;
        }
    }

        /*pub fn send_initial_state(&self) {
            // Encode all blocks
            //Send all blocks and await ack.
        }

        pub fn send_changed_blocks(&self) {
            // Consume changed blocks.
                // Check time
                // Encode changed block
                // Add to send queue
                // Optimistic state update
        }

        }*/
}

