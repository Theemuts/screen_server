use std::collections::HashMap;

use std::sync::mpsc::{
    Sender,
    Receiver
};

use std::thread::{
    self,
    JoinHandle
};

use super::messages::{ContextMessage, PendingAckMessage};

pub fn start_pending_ack_thread(to_context: Sender<ContextMessage>,
                                receiver: Receiver<PendingAckMessage>)
    -> JoinHandle<()>
{
    thread::spawn(move || {
        let mut packet_map = HashMap::new();
        let mut reinit = false;

        loop {
            match receiver.recv() {
                Ok(PendingAckMessage::NewSend(timestamp, packet_id, ref present_ids)) if !reinit => {
                    packet_map.insert(packet_id, (timestamp, present_ids.clone()));
                },
                Ok(PendingAckMessage::NewSend(timestamp, packet_id, ref present_ids)) if timestamp == 0 => {
                    packet_map.insert(packet_id, (timestamp, present_ids.clone()));
                    reinit = false;
                },
                Ok(PendingAckMessage::NewSend(_, _, _)) => (),
                Ok(PendingAckMessage::NewReceive(packet_ids)) => {
                    for packet_id in &packet_ids {
                        match packet_map.remove(packet_id) {
                            Some((timestamp, ref ids)) => {
                                to_context.send(ContextMessage::AckPackets(timestamp, ids.clone())).unwrap();
                            },
                            None => ()
                        }
                    }
                },
                Ok(PendingAckMessage::Close) => {
                    break;
                },
                Ok(PendingAckMessage::Clear) => {
                    packet_map.clear();
                    reinit = true;
                }
                _ => panic!()
            };
        };
    })
}