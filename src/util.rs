use super::x11::xlib::XImage;

pub fn get_data(image_pointer: Option<*mut XImage>) -> *mut i8 {
    let pointer = image_pointer.unwrap();

    unsafe {
        (*pointer).data
    }
}