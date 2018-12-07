#[macro_use]
extern crate bencher;

use bencher::Bencher;

extern crate serde;
extern crate serde_json;

extern crate byteorder;
extern crate bytes;
extern crate flumedb;
extern crate tokio_codec;

use bytes::BytesMut;
use flumedb::flume_log::FlumeLog;
use flumedb::mem_log::MemLog;
use flumedb::offset_log::OffsetCodec;
use flumedb::offset_log::*;
use serde_json::{from_slice, Value};
use tokio_codec::Decoder;

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

fn offset_log_append_batch(b: &mut Bencher) {
    let test_vec: &[u8] = b"{\"value\": 1}";

    let mut test_vecs = Vec::new();

    for _ in 0..NUM_ENTRIES {
        test_vecs.push(test_vec);
    }

    b.iter(|| {
        let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
        std::fs::remove_file(filename.clone()).unwrap_or(());

        let mut offset_log = OffsetLog::<u32>::new(filename);

        let result = offset_log.append_batch(&test_vecs).unwrap();

        assert_eq!(result[0], 0);
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
    let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
    std::fs::remove_file(filename.clone()).unwrap_or(());

    let test_vec = b"{\"value\": 1}";

    let mut offset_log = OffsetLog::<u32>::new(filename.clone());

    let offsets: Vec<u64> = (0..NUM_ENTRIES)
        .map(|_| offset_log.append(test_vec).unwrap())
        .collect();

    assert_eq!(offsets.len(), NUM_ENTRIES as usize);

    b.iter(|| {
        let file = std::fs::File::open(filename.clone()).unwrap();
        let log_iter = OffsetLogIter::<u32, std::fs::File>::new(file);

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
benchmark_group!(
    offset_log,
    offset_log_get,
    offset_log_append,
    offset_log_append_batch,
    offset_log_iter,
    offset_log_decode
);
benchmark_group!(mem_log, mem_log_get, mem_log_append, mem_log_iter);

benchmark_main!(offset_log, mem_log);
