extern crate flumedb;
extern crate bus;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use flumedb::*;
use flumedb::flume_view::FlumeView;
use std::sync::mpsc::{Sender, Receiver, TryRecvError};
use std::sync::mpsc;
use bus::Bus;
use std::thread;

use std::collections::HashMap;

#[derive(Clone)]
struct Data {
    buff: DummyViewData,
    seq: usize,
}

#[derive(Clone)]
struct DummyViewData {
    key: String,
    value: String,
}

struct HashView {
    state: HashMap<String, usize>,
    latest: usize,
}

impl FlumeView<DummyViewData> for HashView {
    fn append(&mut self, seq: usize, item: &DummyViewData){
        self.state.insert(item.key.clone(), seq);
    }
    fn latest(&self) -> usize {
        self.latest
    }
}

impl HashView {
    fn new() -> HashView {
        HashView{
            state: HashMap::new(),
            latest: 0
        }
    }

    fn get(&self, key: String) -> Option<&usize>{
        self.state.get(&key)
    }
}

fn main() {

    pretty_env_logger::init_timed();
    info!("Starting up...");

    let mut vals = Vec::<Data>::new();
    vals.push(Data{buff: DummyViewData{key: "key1".to_string(), value: "value1".to_string()}, seq: 0});
    vals.push(Data{buff: DummyViewData{key: "key2".to_string(), value: "value2".to_string()}, seq: 22});
    vals.push(Data{buff: DummyViewData{key: "key3".to_string(), value: "value3".to_string()}, seq: 333});

    let mut view = HashView::new();

    vals
        .iter()
        .for_each(|val| view.append(val.seq, &val.buff));


    vals
        .iter()
        .for_each(|val| {
            let result = view.get(val.buff.key.clone()).unwrap();
            println!("got value: {}", result);    
        });
} 
