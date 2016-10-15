use std::net::SocketAddr;

use super::util::DataBox;

#[derive(Debug)]
pub enum ContextMessage {
    RequestView(u8, u8),
    Close,
    Refresh,
    NewScreenshot,
    AckPackets(u32, Vec<u16>)
}

#[derive(Debug)]
pub enum EncoderMessage {
    FirstImage(DataBox),
    DataAndErrors(DataBox, Vec<(i64, usize)>),
    Close
}



#[derive(Debug)]
pub enum MainMessage {
    Handshake(SocketAddr, u8, u8), // 0 (protocol version)
    RequestScreenInfo,             // 1
    RequestView(u8, u8),           // 2 (screenID, segmentID)
    Refresh,                       // 3
    Close,                         // 4
    Exit,                          // 5

    LeftClick(u16, u16),           // 6 (x0 y0)
    RightClick(u16, u16),          // 7 (x0, y0)
    DoubleClick(u16, u16),         // 8 (x0, y0)
    Drag(u16, u16, u16, u16),      // 9 (x0, y0, x1, y1)

    Keyboard(Vec<u8>),             // 10 (unicode string)
}

#[derive(Debug)]
pub enum PendingAckMessage {
    NewSend(u32, u32, Vec<u16>),
    NewReceive(Vec<u32>),
    Close
}

#[derive(Debug)]
pub enum SenderMessage {
    AcceptHandshake(SocketAddr, u8), // Address to send to and protocol version
    RejectHandshake(SocketAddr),
    ScreenInfo(Vec<u8>),
    EndOfData(u32),
    Macroblock(u32, Vec<u8>),
    Close
}