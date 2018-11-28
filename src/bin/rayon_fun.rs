extern crate flumedb;
extern crate bus;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;
extern crate rayon;
extern crate serde;
extern crate serde_json;

#[macro_use]
extern crate serde_derive;

use flumedb::*;
use flumedb::flume_view::FlumeView;
use rayon::prelude::*;
use std::sync::mpsc::{Sender, Receiver, TryRecvError};
use std::sync::mpsc;
use bus::Bus;
use std::thread;

use serde_json::Value;

use std::collections::HashMap;

#[derive(Clone)]
struct Data {
    buff: Vec<u8>,
    seq: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct DummyViewData {
    key: String,
    value: String,
}

struct HashView {
    state: HashMap<String, usize>,
    latest: usize,
    //deserializer: 
}

impl FlumeView for HashView {
    //T should have the trait bound that it's hashable or something?
    fn append(&mut self, seq: usize, item: &[u8]){
        //TODO: should append ever be able to fail?
        let key = serde_json::from_slice(item)
            .map(|value: Value|{
                value["key"]
                    .as_str()
                    .expect("Expected a value with a key property")
                    .to_string()
            })
            .unwrap();

        self.state.insert(key, seq);
        self.latest = seq;
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

    let vals: Vec<_> = (0..10000000)
        .map(|num|(num.to_string(), num.to_string(), num))
        .map(|(key, value, num)| (DummyViewData{key, value}, num))
        .map(|(kv, num)| (serde_json::to_string(&kv).unwrap(), num))
        .map(|(buff, num)| Data{buff: buff.into(), seq: num})
        .collect();

    let mut view = HashView::new();

    vals
        .iter()
        .for_each(|val| view.append(val.seq, &val.buff)); //We can't do a par_iter here. Makes sense, it needs to be sequential on the insertion order. But I _think_ we should be able to par_iter on the views collection.

    let sum: usize = vals
        .par_iter()
        .map(|val| serde_json::from_slice(&val.buff).unwrap())
        .map(|val: DummyViewData| view.get(val.key.clone()).unwrap())
        .sum();

    println!("{}", sum);

    info!("Done");
} 
