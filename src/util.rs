use super::x11::xlib::XImage;

#[derive(Debug)]
pub struct DataBox(pub *mut i8);

unsafe impl Send for DataBox {}
unsafe impl Sync for DataBox {}

pub fn get_data(image_pointer: Option<*mut XImage>) -> DataBox
{
    let pointer = image_pointer.unwrap();

    unsafe {
        DataBox((*pointer).data)
    }
}

pub fn value_at(s: *mut i8, index: isize) -> u8
{
    unsafe {
        *s.offset(index) as u8
    }
}