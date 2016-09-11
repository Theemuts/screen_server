use super::util::DataBox;

#[derive(Debug)]
pub enum ContextMessage {
    Init,
    Close,
    ChangeView(u8),
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
    Init,
    ChangeView(u8),
    Close,
    Exit
}

#[derive(Debug)]
pub enum PendingAckMessage {
    NewSend(u32, u32, Vec<u16>),
    NewReceive(Vec<u32>),
    Clear,
    Close
}

#[derive(Debug)]
pub enum SenderMessage {
    EndOfData(u32),
    Macroblock(u32, Vec<u8>),
    Close
}