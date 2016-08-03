extern crate libc;
use super::x11::xlib;

use std::ptr::{
null,
null_mut,
};

pub fn open_display() -> *mut xlib::Display {
    unsafe {
        let display = xlib::XOpenDisplay(null());

        if display == null_mut() {
            panic!("can't open display");
        }

        display
    }
}

pub fn get_root_window(display: *mut xlib::Display) -> u64 {
    unsafe {
        let screen_num = xlib::XDefaultScreen(display);
        xlib::XRootWindow(display, screen_num)
    }
}

pub fn get_image(display: *mut xlib::Display, root: u64,
             width: u32, offset_x: i32,
             height: u32, offset_y: i32)
             -> *mut xlib::XImage {
    unsafe {
        let imag: *mut xlib::XImage = xlib::XGetImage(display, root,
                                                      offset_x, offset_y, width, height,
                                                      xlib::XAllPlanes(),
                                                      xlib::ZPixmap);

        if imag == null_mut() {
            panic!("can't get image")
        }

        imag
    }
}

pub fn copy_image(image: *mut xlib::XImage, dest: &mut Vec<u8>, width: u32, height: u32) {
    let mut dest_ind = 0;

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            unsafe {
                let pix: u64 = xlib::XGetPixel(image, x, y);

                let rpix = ((pix & 16711680) >> 16) as u8;
                let gpix = ((pix & 65280) >> 8) as u8;
                let bpix = (pix & 255) as u8;

                dest[dest_ind] = rpix;
                dest[dest_ind + 1] = gpix;
                dest[dest_ind + 2] = bpix;
                dest_ind += 3;
            }
        }
    }
}

pub fn close_display(display: *mut xlib::Display) {
    unsafe {
        xlib::XCloseDisplay(display);
    }
}

pub fn destroy_image(image: *mut xlib::XImage) {
    unsafe {
        xlib::XDestroyImage(image);
    }
}