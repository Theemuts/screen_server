use std::collections::HashMap;

use std::sync::mpsc::{Sender, Receiver};

use std::thread;

use super::messages::{ContextMessage, PendingAckMessage};

pub fn start_pending_ack_thread(to_context: Sender<ContextMessage>, receiver: Receiver<PendingAckMessage>) {
    thread::spawn(move || {
        let mut packet_map = HashMap::new();

        loop {
            match receiver.recv() {
                Ok(PendingAckMessage::NewSend(timestamp, packet_id, present_ids)) => {
                    packet_map.insert(packet_id, (timestamp, present_ids.clone()));
                },
                Ok(PendingAckMessage::NewReceive(packet_ids)) => {
                    for packet_id in &packet_ids {
                        match packet_map.remove(packet_id) {
                            Some((timestamp, ref ids)) => {
                                to_context.send(ContextMessage::AckPackets(timestamp, ids.clone()));
                            },
                            None => ()
                        }
                    }
                },
                Ok(PendingAckMessage::Close) => {
                    break
                }
                _ => panic!()
            }
        }
    });
}