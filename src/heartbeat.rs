use super::protocol::{
    MainMessage,
    HeartbeatMessage,
    ReceiverMessage
};

use std::sync::mpsc::{
    Sender,
    Receiver,
    RecvTimeoutError,
};

use std::thread::{
    self,
    JoinHandle
};

use std::time::Duration;

pub fn start_heartbeat_thread(to_main: Sender<MainMessage>,
                              to_udp_receiver: Sender<ReceiverMessage>,
                              receiver: Receiver<HeartbeatMessage>,
                              timeout: u64)
    -> JoinHandle<()>
{
    thread::spawn(move || {
        // Start heartbeat once first heartbeat is received. As a result, it is optional.
        match receiver.recv() {
            Ok(HeartbeatMessage::Close) => {
                println!("Heartbeat: Close initial");
                return
            },
            Ok(HeartbeatMessage::Heartbeat) => {
                let timeout_duration = Duration::from_secs(timeout);

                println!("Rec init hb");
                loop {
                    match receiver.recv_timeout(timeout_duration) {
                        Ok(HeartbeatMessage::Heartbeat) => (),
                        Ok(HeartbeatMessage::Close) => {
                            println!("Heartbeat: Close loop");
                            return
                        },
                        Err(RecvTimeoutError::Timeout) => {
                            println!("Heartbeat: Timeout");
                            to_main.send(MainMessage::Close).unwrap();
                            to_udp_receiver.send(ReceiverMessage::HeartbeatTimeout).unwrap();

                            return
                        },
                        Err(RecvTimeoutError::Disconnected) => {
                            println!("Heartbeat: Timeout");
                            to_main.send(MainMessage::Close).unwrap();
                            return
                        }
                    }
                }
            },
            _ => {
                return
            }
        }
    })
}