use super::libxdo::XDo;

pub fn new_session() -> XDo {
    XDo::new(None).unwrap()
}