use std::net::SocketAddr;

use super::util::DataBox;

pub const OPCODE_RECEIVE_HANDSHAKE: u8              = 0;
pub const OPCODE_RECEIVE_REQUEST_SCREEN_INFO: u8    = 1;
pub const OPCODE_RECEIVE_REQUEST_VIEW: u8           = 2;
pub const OPCODE_RECEIVE_REFRESH: u8                = 3;
pub const OPCODE_RECEIVE_CLOSE: u8                  = 4;
pub const OPCODE_RECEIVE_EXIT: u8                   = 5;
pub const OPCODE_RECEIVE_LEFT_CLICK: u8             = 6;
pub const OPCODE_RECEIVE_RIGHT_CLICK: u8            = 7;
pub const OPCODE_RECEIVE_DOUBLE_CLICK: u8           = 8;
pub const OPCODE_RECEIVE_DRAG: u8                   = 9;
pub const OPCODE_RECEIVE_KEYBOARD: u8               = 10;
pub const OPCODE_RECEIVE_ACK: u8                    = 11;
pub const OPCODE_RECEIVE_HEARTBEAT: u8              = 12;

pub const OPCODE_SEND_HANDSHAKE_ACK: u8          = 0;
pub const OPCODE_SEND_SCREEN_INFO: u8            = 1;
pub const OPCODE_SEND_IMAGE_DATA: u8             = 2;

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
pub enum HeartbeatMessage {
    Heartbeat,
    Close,
}

#[derive(Debug)]
pub enum MainMessage {
    Handshake(SocketAddr, u8, u8),
    RequestScreenInfo,
    RequestView(u8, u8),
    Refresh,
    Close,
    Exit,

    LeftClick(u16, u16),
    RightClick(u16, u16),
    DoubleClick(u16, u16),
    Drag(u16, u16, u8, u8, u16, u16, u8, u8),

    Keyboard(Vec<u8>),
}

#[derive(Debug)]
pub enum PendingAckMessage {
    NewSend(u32, u32, Vec<u16>),
    NewReceive(Vec<u32>),
    Close
}

#[derive(Debug)]
pub enum ReceiverMessage {
    HeartbeatTimeout
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