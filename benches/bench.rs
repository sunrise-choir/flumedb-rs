#![feature(test)]

extern crate test;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

extern crate tokio;
extern crate tokio_io;
extern crate tokio_fs;
extern crate bytes;
extern crate byteorder;
extern crate flumedb;

use flumedb::offset_log::*;
use tokio_io::codec::FramedRead;
use tokio::fs::File;
use tokio::prelude::*;
use serde_json::{Value};


use test::Bencher;

#[bench]
fn reduce_log_to_sum_of_value(b: &mut Bencher) {
    b.iter(|| {
        let stream = File::open("./db/test")
            .then(|result|{
                match result {
                    Ok(f) => {
                        let reads = FramedRead::new(f, OffsetCodec::<u32>::new())
                            .map(|val| -> u64{
                                let jsn : Value = serde_json::from_slice(&val.data_buffer).unwrap();
                                match jsn["value"] {
                                    Value::Number(ref num) => num.as_u64().unwrap(),
                                    _ => 0
                                }
                            })
                        .fold(0, |sum, num| {
                            Ok::<_, std::io::Error>(sum + num)
                        })
                        .wait();
                        println!("sum was {}", reads.unwrap());
                        Ok(())
                    },
                    Err(e) => {
                        println!("error {}", e);
                        Err(e)
                    }
                }
            })
        .then(|_ : Result<(), _>|{
            Ok(()) 
        });

        tokio::run(stream);
    });
}
