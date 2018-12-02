#[macro_use]
extern crate bencher;

use bencher::Bencher;

extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

extern crate tokio;
extern crate tokio_io;
extern crate tokio_codec;
extern crate tokio_fs;
extern crate bytes;
extern crate byteorder;
extern crate flumedb;

use tokio_io::codec::Decoder;
use flumedb::offset_log::OffsetCodec;
use bytes::{BytesMut};
use flumedb::offset_log::*;
use tokio_codec::{Framed};
use tokio::fs::File;
use tokio::prelude::*;
use serde_json::{Value, from_slice};



fn simple(b: &mut Bencher){
    b.iter(||{
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0,0,0,8, 1,2,3,4,5,6,7,8, 0,0,0,8, 0,0,0,20];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1,2,3,4,5,6,7,8]);
            },
            _ => assert!(false)
        }
    })
}

fn reduce_log_to_sum_of_value_slow_iter(b: &mut Bencher) {
    b.iter(||{
        let filename = "./db/test".to_string();

        let mut offset_log = OffsetLog::<u32>::new(filename);

        let log_iter = OffsetLogIter::new(&mut offset_log);

        let sum: u64 = log_iter
            .map(|val| from_slice(&val).unwrap())
            .map(|val: Value| { 
                match val["value"] {
                    Value::Number(ref num) => {
                        let result = num.as_u64().unwrap();
                        result
                    },
                    _ => panic!()
                }

            })
        .sum();
    })
}
fn reduce_log_to_sum_of_value(b: &mut Bencher) {
    b.iter(||{
        let stream = File::open("./db/test")
            .then(|result|{
                match result {
                    Ok(f) => {
                        let reads = Framed::new(f, OffsetCodec::<u32>::new())
                            .map(|val| {
                                let jsn : Value = serde_json::from_slice(&val.data_buffer).unwrap();
                                match jsn["value"] {
                                    Value::Number(ref num) => num.as_u64().unwrap(),
                                    _ => 0
                                }
                            })
                            .fold(0, |sum, num| {
                                Ok::<_, std::io::Error>(sum + num)
                            });
                        Ok(reads)
                    },
                    Err(e) => {
                        println!("error {}", e);
                        Err(e)
                    }
                }
            })
            .then(|result| {
                tokio::spawn(result.unwrap()
                         .then(|res|{
                             //println!("res was {}", res.unwrap());
                             Ok(())
                         }))
            });

        tokio::run(stream);
    });
}
benchmark_group!(benches, reduce_log_to_sum_of_value, simple, reduce_log_to_sum_of_value_slow_iter);
benchmark_main!(benches);
