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

#[inline(always)]
pub fn u8s_to_u16(first: u8, last: u8) -> u16 {
    ((first as u16) << 8) | (last as u16)
}

#[inline(always)]
pub fn u8s_to_u32(first: u8, second: u8, third: u8, last: u8) -> u32 {
    ((first as u32) << 24) | ((second as u32) << 16) | ((third as u32) << 8) | (last as u32)
}