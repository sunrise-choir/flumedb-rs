#[macro_use]
extern crate bencher;

use bencher::Bencher;

extern crate serde;
extern crate serde_json;

extern crate byteorder;
extern crate bytes;
extern crate flumedb;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_fs;
extern crate tokio_io;

use bytes::BytesMut;
use flumedb::flume_log::FlumeLog;
use flumedb::mem_log::MemLog;
use flumedb::offset_log::OffsetCodec;
use flumedb::offset_log::*;
use serde_json::{from_slice, Value};
use tokio::fs::File;
use tokio::prelude::*;
use tokio_codec::Framed;
use tokio_io::codec::Decoder;

const NUM_ENTRIES: u32 = 10000;

fn offset_log_decode(b: &mut Bencher) {
    b.iter(|| {
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 20];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
            }
            _ => assert!(false),
        }
    })
}
fn offset_log_append(b: &mut Bencher) {
    b.iter(|| {
        let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
        std::fs::remove_file(filename.clone()).unwrap_or(());

        let test_vec = b"{\"value\": 1}";

        let mut offset_log = OffsetLog::<u32>::new(filename);

        let offsets: Vec<u64> = (0..NUM_ENTRIES)
            .map(|_| offset_log.append(test_vec).unwrap())
            .collect();

        assert_eq!(offsets.len(), NUM_ENTRIES as usize);
    })
}

fn offset_log_get(b: &mut Bencher) {
    let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
    std::fs::remove_file(filename.clone()).unwrap_or(());

    let test_vec = b"{\"value\": 1}";

    let mut offset_log = OffsetLog::<u32>::new(filename);

    let offsets: Vec<u64> = (0..NUM_ENTRIES)
        .map(|_| offset_log.append(test_vec).unwrap())
        .collect();

    b.iter(|| {
        for offset in offsets.clone() {
            let result = offset_log.get(offset).unwrap();
            assert_eq!(result.len(), test_vec.len());
        }
    })
}

fn offset_log_iter(b: &mut Bencher) {
    b.iter(|| {
        let filename = "./db/test".to_string();
        let file = std::fs::File::open(filename).unwrap();

        let log_iter = OffsetLogBufIter::<u32, std::fs::File>::new(file);

        let sum: u64 = log_iter
            .map(|val| from_slice(&val).unwrap())
            .map(|val: Value| match val["value"] {
                Value::Number(ref num) => {
                    let result = num.as_u64().unwrap();
                    result
                }
                _ => panic!(),
            })
            .sum();

        assert!(sum > 0);
    })
}

fn mem_log_get(b: &mut Bencher) {
    let mut log = MemLog::new();

    let test_vec = b"{\"value\": 1}";

    let offsets: Vec<u64> = (0..NUM_ENTRIES)
        .map(|_| log.append(test_vec).unwrap())
        .collect();

    b.iter(|| {
        for offset in offsets.clone() {
            let result = log.get(offset).unwrap();
            assert_eq!(result.len(), test_vec.len());
        }
    })
}

fn mem_log_append(b: &mut Bencher) {
    b.iter(|| {
        let mut log = MemLog::new();

        let test_vec = b"{\"value\": 1}";

        let offsets: Vec<u64> = (0..NUM_ENTRIES)
            .map(|_| log.append(test_vec).unwrap())
            .collect();

        assert_eq!(offsets.len(), NUM_ENTRIES as usize);
    })
}

fn mem_log_iter(b: &mut Bencher) {
    let mut log = MemLog::new();

    let test_vec = b"{\"value\": 1}";

    (0..NUM_ENTRIES).for_each(|_| {
        log.append(test_vec).unwrap();
    });

    b.iter(|| {
        let sum: u64 = log
            .into_iter()
            .map(|val| from_slice(&val).unwrap())
            .map(|val: Value| match val["value"] {
                Value::Number(ref num) => {
                    let result = num.as_u64().unwrap();
                    result
                }
                _ => panic!(),
            })
            .sum();

        assert!(sum > 0);
    })
}

fn tokio_reduce_offset_log(b: &mut Bencher) {
    b.iter(|| {
        let stream = File::open("./db/test")
            .then(|result| match result {
                Ok(f) => {
                    let reads = Framed::new(f, OffsetCodec::<u32>::new())
                        .map(|val| {
                            let jsn: Value = serde_json::from_slice(&val.data_buffer).unwrap();
                            match jsn["value"] {
                                Value::Number(ref num) => num.as_u64().unwrap(),
                                _ => 0,
                            }
                        })
                        .fold(0, |sum, num| Ok::<_, std::io::Error>(sum + num));
                    Ok(reads)
                }
                Err(e) => {
                    println!("error {}", e);
                    Err(e)
                }
            })
            .then(|result| {
                tokio::spawn(result.unwrap().then(|res| {
                    assert!(res.unwrap() > 0);
                    //println!("res was {}", res.unwrap());
                    Ok(())
                }))
            });

        tokio::run(stream);
    });
}

benchmark_group!(
    offset_log,
    offset_log_get,
    offset_log_append,
    offset_log_iter,
    offset_log_decode
);
benchmark_group!(mem_log, mem_log_get, mem_log_append, mem_log_iter);
benchmark_group!(tokio, tokio_reduce_offset_log);

benchmark_main!(offset_log, mem_log);
