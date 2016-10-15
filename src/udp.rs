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

use super::protocol::
{
    SenderMessage,
    MainMessage,
    PendingAckMessage,
    HeartbeatMessage,

    OPCODE_RECEIVE_HANDSHAKE,
    OPCODE_RECEIVE_REQUEST_SCREEN_INFO,
    OPCODE_RECEIVE_REQUEST_VIEW,
    OPCODE_RECEIVE_REFRESH,
    OPCODE_RECEIVE_CLOSE,
    OPCODE_RECEIVE_EXIT,
    OPCODE_RECEIVE_LEFT_CLICK,
    OPCODE_RECEIVE_RIGHT_CLICK,
    OPCODE_RECEIVE_DOUBLE_CLICK,
    OPCODE_RECEIVE_DRAG,
    OPCODE_RECEIVE_KEYBOARD,
    OPCODE_RECEIVE_ACK,
    OPCODE_RECEIVE_HEARTBEAT,

    OPCODE_SEND_HANDSHAKE_ACK,
    OPCODE_SEND_SCREEN_INFO,
    OPCODE_SEND_IMAGE_DATA,
};

use super::util::
{
    u8s_to_u16,
    u8s_to_u32,
};

const MAX_BUFFER_SIZE: usize = 1000;


pub struct Udp {
    socket: UdpSocket,
    client: SocketAddr
}

pub fn init_udp_sockets(pending_ack_sender: Sender<PendingAckMessage>,
                        udp_sender_receiver: Receiver<SenderMessage>,
                        main_sender: Sender<MainMessage>,
                        heartbeat_sender: Sender<HeartbeatMessage>)
    -> (JoinHandle<()>, JoinHandle<()>)
{
    let s_handle = Udp::start_sender_thread(
        pending_ack_sender.clone(),
        udp_sender_receiver
    );

    let r_handle = Udp::start_receiver_thread(
        pending_ack_sender,
        main_sender,
        heartbeat_sender
    );

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

            let mut new_len;

            // Create a new UDP socket and await handshake
            let mut udp = None;

            // Set the intial packet id
            set_packet_id(&mut buffer, id);

            // Start the event loop
            loop {
                match udp_sender_receiver.recv() {
                    // We received the last block to send. If the buffer is
                    // longer than 4 bytes, there is data present which must
                    // be sent.
                    Ok(SenderMessage::AcceptHandshake(
                           src,
                           protocol_version))
                    => {
                        println!("UDP Sender: Accept handshake");

                        let reply = vec![
                            OPCODE_SEND_HANDSHAKE_ACK,
                            protocol_version
                        ];

                        udp = Some(Self::new_sender(src));
                        udp.as_ref()
                            .unwrap()
                            .send(reply.as_slice())
                            .unwrap();
                    },
                    Ok(SenderMessage::ScreenInfo(info))
                    => {
                        println!("UDP Receiver: Screen Info");

                        if udp.as_ref().is_some() {
                            let len = info.len() + 1;

                            let mut i = 1;
                            let mut reply = vec![0u8; len];

                            reply[0] = OPCODE_SEND_SCREEN_INFO;

                            for v in info {
                                reply[i] = v;
                                i += 1;
                            }

                            udp.as_ref()
                                .unwrap()
                                .send(reply.as_slice())
                                .unwrap();
                        }
                    },
                    Ok(SenderMessage::EndOfData(timestamp))
                    => {
                        if (&udp).is_some() {
                            if buffer.len() > 10 {
                                buffer[1] = (timestamp >> 24) as u8;
                                buffer[2] = (timestamp >> 16) as u8;
                                buffer[3] = (timestamp >> 8) as u8;
                                buffer[4] = timestamp as u8;

                                udp.as_ref()
                                    .unwrap()
                                    .send(
                                        buffer.as_slice()
                                    ).unwrap();

                                to_pending_ack
                                    .send(
                                        PendingAckMessage::NewSend(
                                            timestamp,
                                            id,
                                            present_ids.clone()
                                        )
                                    ).unwrap();

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
                    Ok(SenderMessage::Macroblock(timestamp, data))
                    => {
                        if (&udp).is_some() {
                            new_len = buffer.len() + data.len();

                            if (new_len) >= MAX_BUFFER_SIZE {
                                buffer[1] = (timestamp >> 24) as u8;
                                buffer[2] = (timestamp >> 16) as u8;
                                buffer[3] = (timestamp >> 8) as u8;
                                buffer[4] = timestamp as u8;

                                udp.as_ref()
                                    .unwrap()
                                    .send(
                                        buffer.as_slice()
                                    ).unwrap();

                                to_pending_ack
                                    .send(
                                        PendingAckMessage::NewSend(
                                            timestamp,
                                            id,
                                            present_ids.clone()
                                        )
                                    ).unwrap();

                                buffer.clear();
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

                        udp.as_ref()
                            .unwrap()
                            .send(reply.as_slice())
                            .unwrap();

                        to_pending_ack
                            .send(PendingAckMessage::Close)
                            .unwrap();
                        return;
                    }
                    _ => ()
                };
            };
        })
    }

    fn start_receiver_thread(to_pending_ack: Sender<PendingAckMessage>,
                             main_sender: Sender<MainMessage>,
                             heartbeat_sender: Sender<HeartbeatMessage>)
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
                            OPCODE_RECEIVE_HANDSHAKE
                                if amt == 3
                            => {
                                println!("UDP Receiver: Handshake");
                                main_sender
                                    .send(MainMessage::Handshake(
                                        src,
                                        buf[1],
                                        buf[2])
                                    ).unwrap();
                            },

                            OPCODE_RECEIVE_REQUEST_SCREEN_INFO
                                if amt == 1
                            => {
                                println!("UDP Receiver: Request screen info");
                                main_sender
                                    .send(MainMessage::RequestScreenInfo)
                                    .unwrap();
                            },

                            OPCODE_RECEIVE_REQUEST_VIEW
                                if amt == 3
                            => {
                                println!("UDP Receiver: Request view");
                                main_sender
                                    .send(MainMessage::RequestView(
                                        buf[1],
                                        buf[2])
                                    ).unwrap();
                            },

                            OPCODE_RECEIVE_REFRESH
                                if amt == 1
                            => {
                                println!("UDP Receiver: Refresh");
                                main_sender
                                    .send(MainMessage::Refresh)
                                    .unwrap();
                            },

                            OPCODE_RECEIVE_CLOSE
                                if amt == 1
                            => {
                                println!("UDP Receiver: Close");
                                main_sender
                                    .send(MainMessage::Close)
                                    .unwrap();

                                heartbeat_sender
                                    .send(HeartbeatMessage::Close)
                                    .unwrap();
                                return;
                            },

                            OPCODE_RECEIVE_EXIT
                                if amt == 1
                            => {
                                println!("UDP Receiver: Exit");
                                main_sender
                                    .send(MainMessage::Exit)
                                    .unwrap();

                                heartbeat_sender
                                    .send(HeartbeatMessage::Close)
                                    .unwrap();
                                return;
                            },

                            OPCODE_RECEIVE_LEFT_CLICK
                                if amt == 5
                            => {
                                main_sender
                                    .send(MainMessage::LeftClick(
                                        u8s_to_u16(buf[1], buf[2]),
                                        u8s_to_u16(buf[3], buf[4]))
                                    ).unwrap();
                            },

                            OPCODE_RECEIVE_RIGHT_CLICK
                                if amt == 5
                            => {
                                main_sender
                                    .send(MainMessage::RightClick(
                                        u8s_to_u16(buf[1], buf[2]),
                                        u8s_to_u16(buf[3], buf[4]))
                                    ).unwrap();
                            },

                            OPCODE_RECEIVE_DOUBLE_CLICK
                                if amt == 5
                            => {
                                main_sender
                                    .send(MainMessage::DoubleClick(
                                        u8s_to_u16(buf[1], buf[2]),
                                        u8s_to_u16(buf[3], buf[4]))
                                    ).unwrap();
                            },

                            OPCODE_RECEIVE_DRAG
                                if amt == 9
                            => {
                                main_sender
                                    .send(MainMessage::Drag(
                                        u8s_to_u16(buf[1], buf[2]),
                                        u8s_to_u16(buf[3], buf[4]),
                                        u8s_to_u16(buf[5], buf[6]),
                                        u8s_to_u16(buf[7], buf[8]))
                                    ).unwrap();
                            },

                            OPCODE_RECEIVE_KEYBOARD
                            => {
                                main_sender
                                    .send(MainMessage::Keyboard(buf))
                                    .unwrap();
                            },

                            OPCODE_RECEIVE_ACK
                            => {
                                to_pending_ack
                                    .send(PendingAckMessage::NewReceive(
                                        get_packet_ids(&buf))
                                    ).unwrap();

                                buf.clear();
                            },

                            OPCODE_RECEIVE_HEARTBEAT
                                if amt == 1
                            => {
                                heartbeat_sender
                                    .send(HeartbeatMessage::Heartbeat)
                                    .unwrap();
                            },

                            _ => ()
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
        Udp {socket: sock, client: src}
    }

    fn send(&self, buf: &[u8]) -> Result<usize>
    {
        Ok(try!(self.socket.send_to(buf, self.client)))
    }
}

fn set_packet_id(buffer: &mut Vec<u8>, id: u32)
{
    buffer.push(OPCODE_SEND_IMAGE_DATA);

    for _ in 0..4 {
        buffer.push(0u8);
    }

    for i in 0..4 {
        buffer.push(
            (id >> (24 - (3 - i)*8)) as u8
        );
    }}

fn get_packet_ids(buffer: &Vec<u8>) -> Vec<u32>
{
    let n_ids = buffer[1] as usize;
    let mut result = Vec::with_capacity(n_ids);

    for i in 0..n_ids {
        result.push(
            u8s_to_u32(
                buffer[4 * i + 2],
                buffer[4 * i + 3],
                buffer[4 * i + 4],
                buffer[4 * i + 5]
            )
        );
    }

    result
}

#[inline(always)]
fn get_block_id(data: &Vec<u8>) -> u16
{
    ((data[0] as u16) << 2) | ((data[1] as u16) >> 6)
}