use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::thread;
use std::io::Result;

use std::collections::HashMap;

use std::time::{SystemTime, Duration};

use super::messages::{SenderMessage, MainMessage, PendingAckMessage};

const MAX_BUFFER_SIZE: usize = 1000;

pub struct Udp {
    socket: UdpSocket,
    client: SocketAddr,
    buffer: Vec<u8>
}

pub fn init_udp_sockets(pending_ack_sender: Sender<PendingAckMessage>,
                        udp_sender_receiver: Receiver<SenderMessage>,
                        main_sender: Sender<MainMessage>)
{
    // TODO: make sure client and server can communicate.

    // Create the sender thread first. This will block until a handshake
    // has been made.
    Udp::start_sender_thread(pending_ack_sender.clone(), udp_sender_receiver, main_sender.clone());
    Udp::start_receiver_thread(pending_ack_sender, main_sender);
}


impl Udp {
    fn start_sender_thread(to_pending_ack: Sender<PendingAckMessage>,
                           udp_sender_receiver: Receiver<SenderMessage>,
                           main_sender: Sender<MainMessage>)
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
            let udp = Self::new_sender();

            // Stop blocking.
            main_sender.send(MainMessage::Init);

            // Set the intial packet id
            set_packet_id(&mut buffer, id);

            // Start the event loop
            loop {
                match udp_sender_receiver.recv() {
                    // We received the last block to send. If the buffer is
                    // longer than 4 bytes, there is data present which must
                    // be sent.
                    Ok(SenderMessage::EndOfData(timestamp)) => {
                        if buffer.len() > 8 {
                            buffer[0] = (timestamp >> 24) as u8;
                            buffer[1] = (timestamp >> 16) as u8;
                            buffer[2] = (timestamp >> 8) as u8;
                            buffer[3] = timestamp as u8;

                            let _ = udp.send(buffer.as_slice()).unwrap();
                            to_pending_ack.send(PendingAckMessage::NewSend(timestamp, id, present_ids.clone()));
                            present_ids.clear();

                            // Increment packet id.
                            id += 1;
                        }

                        // Clear buffer and set appropriate packet id.
                        buffer.clear();
                        set_packet_id(&mut buffer, id);
                    },
                    // We received a new encoded macroblock to send. If the
                    // data is too long for the current buffer, its current
                    // contents are sent, the buffer is cleared and
                    // reinitialized, the blocks present are added to the map
                    // of unacknowledged packets and the current list is
                    // cleared. The block id is added to the current id list.
                    Ok(SenderMessage::Macroblock(timestamp, data)) => {
                        if (buffer.len() + data.len()) >= MAX_BUFFER_SIZE {
                            buffer[0] = (timestamp >> 24) as u8;
                            buffer[1] = (timestamp >> 16) as u8;
                            buffer[2] = (timestamp >> 8) as u8;
                            buffer[3] = timestamp as u8;

                            let _ = udp.send(buffer.as_slice());
                            buffer.clear();

                            to_pending_ack.send(PendingAckMessage::NewSend(timestamp, id, present_ids.clone()));
                            present_ids.clear();

                            id += 1;
                            set_packet_id(&mut buffer, id);
                        }

                        // Add to packet id list.
                        present_ids.push(get_block_id(&data));
                        buffer.extend(data.iter().cloned());
                    },
                    _ => panic!()
                }
            }
        });
    }

    fn start_receiver_thread(to_pending_ack: Sender<PendingAckMessage>,
                             main_sender: Sender<MainMessage>)
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
                    Ok((amt, _src)) => {
                        let msg = PendingAckMessage::NewReceive(get_packet_ids(&buf));
                        to_pending_ack.send(msg);
                        buf.clear();
                    }
                };
            }
        });
    }

    fn new_sender() -> Self {
        let sock = match UdpSocket::bind("0.0.0.0:9999") {
            Ok(s) => s,
            Err(e) => panic!("Could not bind socket: {}", e)
        };

        let mut buf = vec![0; 1200];

        match sock.recv_from(buf.as_mut_slice()) {
            Ok((_amt, src)) => {
                let mut src = src.clone();
                src.set_port(36492);
                return Udp { socket: sock, client: src, buffer: buf }
            },
            Err(e) => {
                panic!("couldn't recieve a datagram: {}", e);
            }
        }
    }

    fn send(&self, buf: &[u8]) -> Result<usize> {
        let size = try!(self.socket.send_to(buf, self.client));

        Ok(size)
    }
}

fn set_packet_id(buffer: &mut Vec<u8>, id: u32) {
    buffer.push(0u8);
    buffer.push(0u8);
    buffer.push(0u8);
    buffer.push(0u8);

    buffer.push((id >> 24) as u8);
    buffer.push((id >> 16) as u8);
    buffer.push((id >> 8) as u8);
    buffer.push(id as u8);
}

fn get_packet_ids(buffer: &Vec<u8>) -> Vec<u32> {
    let n_ids = buffer[0] as usize;
    let mut result = Vec::with_capacity(n_ids);

    for i in 0..n_ids {
        result.push(((buffer[4 * i + 1] as u32) << 24) | ((buffer[4 * i + 2] as u32) << 16) | ((buffer[4 * i + 3] as u32) << 8) | (buffer[4 * i + 4] as u32))
    }

    result
}

fn get_block_id(data: &Vec<u8>) -> u16 {
    ((data[0] as u16) << 2) | ((data[1] as u16) >> 6)
}