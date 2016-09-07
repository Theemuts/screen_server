use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::thread;
use std::io::Result;

use std::collections::HashMap;

const MAX_BUFFER_SIZE: usize = 1000;

pub struct Udp {
    socket: UdpSocket,
    client: SocketAddr,
    buffer: Vec<u8>
}

#[derive(Debug)]
pub enum SenderMessage {
    EndOfData,
    IncrementTimestamp,
    Macroblock(Vec<u8>),
    AcknowledgePackets(Vec<u32>),
}

impl Udp {
    pub fn init_udp_sockets() -> (Sender<SenderMessage>, Receiver<Vec<u16>>) {
        // We need to send messages between the sender and retriever thread to
        // handle the acknowledgment of packets. The sender maintains a map of
        // unacknowledged packets, with packet ids as the key and a vec of the
        // blocks which are present in that packet.
        let (to_receiver, from_sender): (Sender<Vec<u16>>, Receiver<Vec<u16>>) = channel();

        // Create the sender thread first. This will block until a handshake
        // has been made. Needs the to_receiver-end of the channel to send
        // the list of blocks that can be updated to the receiver thread.
        let to_sender = Self::start_a_sender_thread(to_receiver);

        // With the to_sender-end of the channel, we create a receiver thread.
        // This thread must send acknowledgements to the sender thread, and
        // receive an id list in return.
        let from_receiver = Self::start_a_receiver_thread(to_sender.clone(), from_sender);

        // TODO: make sure client and server can communicate.

        (to_sender, from_receiver)
    }

    fn start_a_sender_thread(to_receiver: Sender<Vec<u16>>)
        -> Sender<SenderMessage>
    {
        // Create a channel for communication with this thread
        let (tx_data, rx_data): (Sender<SenderMessage>, Receiver<SenderMessage>) = channel();
        // Create a channel to indicate the handshake has been successful.
        let (tx_udp, rx_udp): (Sender<bool>, Receiver<bool>) = channel();

        // Spawn the sender thread.
        thread::spawn(move || {
            // The packet id is a 32 bit unsigned integer
            let mut timestamp = 0u32;
            let mut id = 0u32;

            // Create a buffer for sending data
            let mut buffer = Vec::with_capacity(MAX_BUFFER_SIZE);

            // The vector of ids present in the current packet
            let mut present_ids = Vec::with_capacity(100);

            // The map of unacknowledged packets.
            let mut packet_map = HashMap::new();

            // Create a new UDP socket and await handshake
            let udp = Self::new_sender();

            // Stop blocking.
            tx_udp.send(true).unwrap();

            // Set the intial packet id
            set_packet_id(&mut buffer, timestamp, id);

            // Start the event loop
            loop {
                match rx_data.recv() {
                    // We received the last block to send. If the buffer is
                    // longer than 4 bytes, there is data present which must
                    // be sent.
                    Ok(SenderMessage::EndOfData) => {
                        if buffer.len() > 8 {
                            let _ = udp.send(buffer.as_slice());

                            packet_map.insert(id, present_ids.clone());
                            present_ids.clear();

                            // Increment packet id.
                            id += 1;
                        }

                        // Clear buffer and set appropriate packet id.
                        buffer.clear();
                        set_packet_id(&mut buffer, timestamp, id);
                    },
                    // We received a new encoded macroblock to send. If the
                    // data is too long for the current buffer, its current
                    // contents are sent, the buffer is cleared and
                    // reinitialized, the blocks present are added to the map
                    // of unacknowledged packets and the current list is
                    // cleared. The block id is added to the current id list.
                    Ok(SenderMessage::Macroblock(data)) => {
                        if (buffer.len() + data.len()) >= MAX_BUFFER_SIZE {
                            let _ = udp.send(buffer.as_slice());
                            buffer.clear();

                            packet_map.insert(id, present_ids.clone());
                            present_ids.clear();

                            id += 1;
                            set_packet_id(&mut buffer, timestamp, id);
                        }

                        // Add to packet id list.
                        present_ids.push(get_block_id(&data));
                        buffer.extend(data.iter().cloned());
                    },
                    // TODO: send to context, not retriever?
                    // A packet has been acknowledged, so we can send the list
                    // of ids to the receiver thread and
                    Ok(SenderMessage::AcknowledgePackets(ids)) => {
                        for id in &ids {
                            to_receiver.send(packet_map.get(&id).unwrap().clone()); // THIS GOES WRONG
                            packet_map.remove(&id);
                        }
                    },
                    Ok(SenderMessage::IncrementTimestamp) => timestamp += 1,
                    Err(e) => ()
                }
            }
        });

        // Block until handshake
        rx_udp.recv().unwrap();

        // Return to_sender channel
        tx_data
    }

    fn start_a_receiver_thread(to_sender: Sender<SenderMessage>,
                               from_sender: Receiver<Vec<u16>>)
        -> Receiver<Vec<u16>>
    {
        let (to_context, from_receiver): (Sender<Vec<u16>>, Receiver<Vec<u16>>) = channel();

        let sock = match UdpSocket::bind("0.0.0.0:9998") {
        Ok(s) => s,
        Err(e) => panic!("Could not bind socket: {}", e)
    };

        thread::spawn(move || {
            let mut msg;

            loop {
                let mut buf = vec![0u8; 800];

                match sock.recv_from(buf.as_mut_slice()) {
                    Err(e) => panic!("Error receiving data: {}", e),
                    Ok((amt, _src)) => {
                        // send ack to sender
                        msg = SenderMessage::AcknowledgePackets(get_packet_ids(&buf));
                        to_sender.send(msg);

                        // receive block ids from sender, send them to context
                        to_context.send(from_sender.recv().unwrap().clone());

                        // clear the buffer
                        buf.clear();
                    }
                };
            }
        });

        from_receiver
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

fn set_packet_id(buffer: &mut Vec<u8>, timestamp: u32, id: u32) {
    buffer.push((timestamp >> 24) as u8);
    buffer.push((timestamp >> 16) as u8);
    buffer.push((timestamp >> 8) as u8);
    buffer.push(timestamp as u8);

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