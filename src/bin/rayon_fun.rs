extern crate flumedb;
extern crate bincode;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;
extern crate rayon;
extern crate serde;
extern crate serde_json;
extern crate serde_cbor;

#[macro_use]
extern crate serde_derive;

use flumedb::*;
use flumedb::flume_view::FlumeView;
use rayon::prelude::*;
use bincode::{serialize, deserialize};

use serde_json::Value;

use std::collections::HashMap;

#[derive(Clone)]
struct Data {
    buff: Vec<u8>,
    seq: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct DummyViewData {
    key: usize,
    value: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct DummyViewTuple(usize, usize);

struct HashView {
    state: HashMap<usize, usize>,
    latest: usize,
    //deserializer: 
}

impl FlumeView for HashView {
    //T should have the trait bound that it's hashable or something?
    fn append(&mut self, seq: usize, item: &[u8]){
        //TODO: should append ever be able to fail?
        let key = deserialize(item)
            .map(|value: DummyViewTuple|{
                value.0
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

    fn get(&self, key: usize) -> Option<&usize>{
        self.state.get(&key)
    }
}

fn main() {

    pretty_env_logger::init_timed();
    info!("Starting up...");

    let vals: Vec<_> = (0..10000000)
        .map(|num|(num, num, num))
        .map(|(key, value, num)| (DummyViewTuple(key, value), num))
        .map(|(kv, num)| (serialize(&kv).unwrap(), num))
        .map(|(buff, num)| Data{buff: buff, seq: num})
        .collect();

    let mut view = HashView::new();

    info!("Built fake log");
    vals
        .iter()
        .for_each(|val| view.append(val.seq, &val.buff)); //We can't do a par_iter here. Makes sense, it needs to be sequential on the insertion order. But I _think_ we should be able to par_iter on the views collection.

    info!("Built view");

    let sum: usize = vals
        .par_iter()
        .map(|val| deserialize(&val.buff).unwrap())
        .map(|val: DummyViewTuple| view.get(val.0).unwrap())
        .sum();

    println!("{}", sum);

    info!("Done");
} 
