extern crate x11;

use std::time::SystemTime;

mod xinterface;
mod context;

fn main () {
    let mut context = context::Context::new(640, 0, 360, 0);
    // context.send_initial_state();

    context.get_new_screenshot();
    let t2 = SystemTime::now();
    for _ in 0..10000 {
        context.set_block_errors();
    }

    println!("{:?}", t2.elapsed());
    //println!("{:?} {:?}", t1, t2.elapsed());
    //context.send_changed_blocks();

    // Based on error, encode block.
    context.print_errors();

    context.close();
}