use std::net::
{
    SocketAddr,
    UdpSocket
};

use std::sync::mpsc::
{
    Sender,
    Receiver,
};

use std::thread::
{
    self,
    JoinHandle
};

use std::io::Result;

use super::messages::{SenderMessage, MainMessage, PendingAckMessage};

const MAX_BUFFER_SIZE: usize = 1000;

const OPCODE_HANDSHAKE_ACK: u8 = 0;
const OPCODE_SCREEN_INFO: u8 = 1;
const OPCODE_IMAGE_DATA: u8 = 2;

pub struct Udp {
    socket: UdpSocket,
    client: SocketAddr
}

pub fn init_udp_sockets(pending_ack_sender: Sender<PendingAckMessage>,
                        udp_sender_receiver: Receiver<SenderMessage>,
                        main_sender: Sender<MainMessage>)
    -> (JoinHandle<()>, JoinHandle<()>)
{
    let s_handle = Udp::start_sender_thread(pending_ack_sender.clone(), udp_sender_receiver);
    let r_handle = Udp::start_receiver_thread(pending_ack_sender, main_sender);

    (s_handle, r_handle)
}


impl Udp {
    fn start_sender_thread(to_pending_ack: Sender<PendingAckMessage>,
                           udp_sender_receiver: Receiver<SenderMessage>)
        -> JoinHandle<()>
    {
        // Spawn the sender thread.
        thread::spawn(move || {
            // The packet id is a 32 bit unsigned integer
            // Create a buffer for sending data
            // The vector of ids present in the current packet
            let mut id = 0u32;
            let mut buffer = Vec::with_capacity(MAX_BUFFER_SIZE);
            let mut present_ids = Vec::with_capacity(100);

            // Create a new UDP socket and await handshake
            let mut udp = None;

            //main_sender.send(MainMessage::Init).unwrap();

            // Set the intial packet id
            set_packet_id(&mut buffer, id);

            // Start the event loop
            loop {
                match udp_sender_receiver.recv() {
                    // We received the last block to send. If the buffer is
                    // longer than 4 bytes, there is data present which must
                    // be sent.
                    Ok(SenderMessage::AcceptHandshake(src, protocol_version)) => {
                        println!("UDP Sender: Accept handshake");

                        let reply = vec![OPCODE_HANDSHAKE_ACK, protocol_version];

                        udp = Some(Self::new_sender(src));
                        udp.as_ref().unwrap().send(reply.as_slice()).unwrap();
                    },
                    Ok(SenderMessage::ScreenInfo(info)) => {
                        println!("UDP Receiver: Screen Info");

                        if udp.as_ref().is_some() {
                            let len = info.len() + 1;

                            let mut i = 1;
                            let mut reply = vec![0u8; len];

                            reply[0] = OPCODE_SCREEN_INFO;

                            for v in info {
                                reply[i] = v;
                                i += 1;
                            }

                            udp.as_ref().unwrap().send(reply.as_slice()).unwrap();
                        }
                    },
                    Ok(SenderMessage::EndOfData(timestamp)) => {
                        if (&udp).is_some() {
                            if buffer.len() > 10 {
                                buffer[1] = (timestamp >> 24) as u8;
                                buffer[2] = (timestamp >> 16) as u8;
                                buffer[3] = (timestamp >> 8) as u8;
                                buffer[4] = timestamp as u8;

                                buffer[9] = present_ids.len() as u8;

                                //println!("n blocks: {}", present_ids.len());

                                let _ = udp.as_ref().unwrap().send(buffer.as_slice()).unwrap();
                                to_pending_ack.send(PendingAckMessage::NewSend(timestamp, id, present_ids.clone())).unwrap();
                                present_ids.clear();

                                // Increment packet id.
                                id += 1;
                            }

                            // Clear buffer and set appropriate packet id.
                            buffer.clear();
                            set_packet_id(&mut buffer, id);
                        }
                    },
                    // We received a new encoded macroblock to send. If the
                    // data is too long for the current buffer, its current
                    // contents are sent, the buffer is cleared and
                    // reinitialized, the blocks present are added to the map
                    // of unacknowledged packets and the current list is
                    // cleared. The block id is added to the current id list.
                    Ok(SenderMessage::Macroblock(timestamp, data)) => {
                        if (&udp).is_some() {
                            if (buffer.len() + data.len()) >= MAX_BUFFER_SIZE {
                                buffer[1] = (timestamp >> 24) as u8;
                                buffer[2] = (timestamp >> 16) as u8;
                                buffer[3] = (timestamp >> 8) as u8;
                                buffer[4] = timestamp as u8;

                                buffer[9] = present_ids.len() as u8;

                                //println!("n blocks: {}", present_ids.len());

                                let _ = udp.as_ref().unwrap().send(buffer.as_slice()).unwrap();
                                buffer.clear();

                                to_pending_ack.send(PendingAckMessage::NewSend(timestamp, id, present_ids.clone())).unwrap();
                                present_ids.clear();

                                id += 1;
                                set_packet_id(&mut buffer, id);
                            }

                            // Add to packet id list.
                            present_ids.push(get_block_id(&data));
                            buffer.extend(data.iter().cloned());
                        }
                    },
                    Ok(SenderMessage::Close) => {
                        println!("UDP Sender: Close");
                        let reply = vec![3];

                        udp.as_ref().unwrap().send(reply.as_slice()).unwrap();

                        udp = None;
                        to_pending_ack.send(PendingAckMessage::Close).unwrap();
                        return;
                    }
                    _ => ()
                };
            };
        })
    }

    fn start_receiver_thread(to_pending_ack: Sender<PendingAckMessage>,
                             main_sender: Sender<MainMessage>)
        -> JoinHandle<()>
    {
        let sock = match UdpSocket::bind("0.0.0.0:9998") {
            Ok(s) => s,
            Err(e) => panic!("Could not bind socket: {}", e)
        };

        thread::spawn(move || {
            loop {
                let mut buf = vec![0u8; 800];

                match sock.recv_from(buf.as_mut_slice()) {
                    Err(e) => panic!("Error receiving data: {}", e),
                    Ok((amt, src)) => {
                        match buf[0] {
                            0 if amt == 3 => {
                                println!("UDP Receiver: Handshake");
                                let msg = MainMessage::Handshake(src, buf[1], buf[2]);
                                main_sender.send(msg).unwrap();
                            },
                            1 if amt == 1 => {
                                println!("UDP Receiver: Request screen info");
                                let msg = MainMessage::RequestScreenInfo;
                                main_sender.send(msg).unwrap();
                            },
                            2 if amt == 3 => {
                                println!("UDP Receiver: Request view");
                                let msg = MainMessage::RequestView(buf[1], buf[2]);
                                main_sender.send(msg).unwrap();
                            },
                            3 if amt == 1 => {
                                println!("UDP Receiver: Refresh");
                                let msg = MainMessage::Refresh;
                                main_sender.send(msg).unwrap();
                            },
                            4 if amt == 1 => {
                                println!("UDP Receiver: Close");
                                let msg = MainMessage::Close;
                                main_sender.send(msg).unwrap();
                                return;
                            },
                            5 if amt == 1 => {
                                println!("UDP Receiver: Exit");
                                let msg = MainMessage::Exit;
                                main_sender.send(msg).unwrap();
                                return;
                            },
                            11 => {
                                //println!("UDP Receiver: Receive ack");

                                let ids = get_packet_ids(&buf);
                                buf.clear();

                                let msg = PendingAckMessage::NewReceive(ids);
                                to_pending_ack.send(msg).unwrap();
                            },
                            _ => {
                                ();
                            }
                        };
                    }
                };
            };
        })
    }

    fn new_sender(mut src: SocketAddr)
        -> Self
    {
        let sock = match UdpSocket::bind("0.0.0.0:9999") {
            Ok(s) => s,
            Err(e) => panic!("Could not bind socket: {}", e)
        };

        src.set_port(36492);

        Udp { socket: sock, client: src }

    }

    fn send(&self, buf: &[u8]) -> Result<usize>
    {
        let size = try!(self.socket.send_to(buf, self.client));

        Ok(size)
    }
}

fn set_packet_id(buffer: &mut Vec<u8>, id: u32)
{
    buffer.push(OPCODE_IMAGE_DATA);

    buffer.push(0u8);
    buffer.push(0u8);
    buffer.push(0u8);
    buffer.push(0u8);

    buffer.push((id >> 24) as u8);
    buffer.push((id >> 16) as u8);
    buffer.push((id >> 8) as u8);
    buffer.push(id as u8);

    buffer.push(0u8);
}

fn get_packet_ids(buffer: &Vec<u8>) -> Vec<u32>
{
    let n_ids = buffer[1] as usize;
    let mut result = Vec::with_capacity(n_ids);

    for i in 0..n_ids {
        result.push(((buffer[4 * i + 2] as u32) << 24) | ((buffer[4 * i + 3] as u32) << 16) | ((buffer[4 * i + 4] as u32) << 8) | (buffer[4 * i + 5] as u32))
    }

    result
}

fn get_block_id(data: &Vec<u8>) -> u16
{
    ((data[0] as u16) << 2) | ((data[1] as u16) >> 6)
}