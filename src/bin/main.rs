extern crate bus;
extern crate flumedb;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use bus::Bus;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread;

#[derive(Clone)]
struct Data {
    buff: Vec<u8>,
    seq: usize,
}

enum HashViewCmd {
    GetSeq(String, usize, Sender<usize>),
}

fn main() {
    pretty_env_logger::init_timed();
    info!("Starting up...");

    let mut vals = Vec::<Data>::new();
    vals.push(Data {
        buff: vec![1, 2, 3],
        seq: 0,
    });
    vals.push(Data {
        buff: vec![1, 2, 3],
        seq: 123,
    });

    let mut log_bus = Bus::<Data>::new(10);
    let mut hash_view_rx = log_bus.add_rx();

    let (hash_view_cmd_tx, hash_view_cmd_rx): (Sender<HashViewCmd>, Receiver<HashViewCmd>) =
        mpsc::channel();

    let hash_view_thread = thread::spawn(move || {
        let mut state = Vec::new();
        loop {
            match hash_view_rx.try_recv() {
                Ok(data) => {
                    trace!("got some data");
                    state.push(data)
                }
                Err(TryRecvError::Disconnected) => {
                    trace!("got an Disconnected error");
                    break;
                }
                _ => (),
            }
            match hash_view_cmd_rx.try_recv() {
                Ok(HashViewCmd::GetSeq(hash, request_num, sender)) => {
                    trace!(
                        "get Seq Cmd for hash {}, request num is  {}",
                        hash,
                        request_num
                    );
                    sender.send(1).unwrap();
                }
                Err(TryRecvError::Disconnected) => {
                    trace!("got a Disconnected error");
                    break;
                }
                _ => (),
            }
            thread::yield_now();
        }
    });

    thread::sleep(std::time::Duration::from_secs(2));
    log_bus.broadcast(vals.pop().unwrap());
    log_bus.broadcast(vals.pop().unwrap());

    let (cb_tx, cb_rx): (Sender<usize>, Receiver<usize>) = mpsc::channel();
    hash_view_cmd_tx
        .send(HashViewCmd::GetSeq("somehash".to_string(), 1, cb_tx))
        .unwrap();

    if let Ok(result) = cb_rx.recv() {
        trace!("hash was {}", result)
    }

    drop(log_bus);

    hash_view_thread.join().unwrap();
}
