#[macro_use]
extern crate criterion;

use criterion::Criterion;

extern crate serde;
extern crate serde_json;

extern crate byteorder;
extern crate bytes;
extern crate flumedb;

use flumedb::flume_log::FlumeLog;
use flumedb::mem_log::MemLog;
use flumedb::offset_log::*;
use serde_json::{from_slice, Value};

const NUM_ENTRIES: u32 = 10000;

fn offset_log_decode(c: &mut Criterion) {
    c.bench_function("offset_log_decode", |b| {
        b.iter(|| {
            let bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 20];
            let r = read_next::<u32, _>(0, &bytes).unwrap();

            assert_eq!(&r.entry.data, &[1, 2, 3, 4, 5, 6, 7, 8]);
        })
    });
}
fn offset_log_append(c: &mut Criterion) {
    c.bench_function("offset log append", move |b| {
        b.iter(|| {
            let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
            std::fs::remove_file(filename.clone()).unwrap_or(());

            let test_vec = b"{\"value\": 1}";

            let mut offset_log = OffsetLog::<u32>::new(filename).unwrap();

            let offsets: Vec<u64> = (0..NUM_ENTRIES)
                .map(|_| offset_log.append(test_vec).unwrap())
                .collect();

            assert_eq!(offsets.len(), NUM_ENTRIES as usize);
        })
    });
}

fn offset_log_append_batch(c: &mut Criterion) {
    let test_vec: &[u8] = b"{\"value\": 1}";

    let mut test_vecs = Vec::new();

    for _ in 0..NUM_ENTRIES {
        test_vecs.push(test_vec);
    }

    c.bench_function("offset log append batch", move |b| {
        b.iter(|| {
            let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
            std::fs::remove_file(filename.clone()).unwrap_or(());

            let mut offset_log = OffsetLog::<u32>::new(filename).unwrap();

            let result = offset_log.append_batch(&test_vecs).unwrap();

            assert_eq!(result[0], 0);
        })
    });
}

fn offset_log_get(c: &mut Criterion) {
    let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
    std::fs::remove_file(filename.clone()).unwrap_or(());

    let test_vec = b"{\"value\": 1}";

    let mut offset_log = OffsetLog::<u32>::new(filename).unwrap();

    let offsets: Vec<u64> = (0..NUM_ENTRIES)
        .map(|_| offset_log.append(test_vec).unwrap())
        .collect();

    c.bench_function("offset log get", move |b| {
        b.iter(|| {
            for offset in offsets.clone() {
                let result = offset_log.get(offset).unwrap();
                assert_eq!(result.len(), test_vec.len());
            }
        })
    });
}

fn offset_log_iter(c: &mut Criterion) {
    let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
    std::fs::remove_file(filename.clone()).unwrap_or(());

    let test_vec = b"{\"value\": 1}";

    let mut offset_log = OffsetLog::<u32>::new(filename.clone()).unwrap();

    let offsets: Vec<u64> = (0..NUM_ENTRIES)
        .map(|_| offset_log.append(test_vec).unwrap())
        .collect();

    assert_eq!(offsets.len(), NUM_ENTRIES as usize);

    c.bench_function("offset log iter", move |b| {
        b.iter(|| {
            let log_iter = offset_log.iter().forward();

            let sum: u64 = log_iter
                .map(|val| val.data)
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
    });
}

fn mem_log_get(c: &mut Criterion) {
    let mut log = MemLog::new();

    let test_vec = b"{\"value\": 1}";

    let offsets: Vec<u64> = (0..NUM_ENTRIES)
        .map(|_| log.append(test_vec).unwrap())
        .collect();

    c.bench_function("mem log get", move |b| {
        b.iter(|| {
            for offset in offsets.clone() {
                let result = log.get(offset).unwrap();
                assert_eq!(result.len(), test_vec.len());
            }
        })
    });
}

fn mem_log_append(c: &mut Criterion) {
    c.bench_function("mem log append", move |b| {
        b.iter(|| {
            let mut log = MemLog::new();

            let test_vec = b"{\"value\": 1}";

            let offsets: Vec<u64> = (0..NUM_ENTRIES)
                .map(|_| log.append(test_vec).unwrap())
                .collect();

            assert_eq!(offsets.len(), NUM_ENTRIES as usize);
        })
    });
}

fn mem_log_iter(c: &mut Criterion) {
    let mut log = MemLog::new();

    let test_vec = b"{\"value\": 1}";

    (0..NUM_ENTRIES).for_each(|_| {
        log.append(test_vec).unwrap();
    });

    c.bench_function("mem log iter", move |b| {
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
    });
}

criterion_group! {
name = offset_log;
config = Criterion::default().sample_size(10);
targets = offset_log_get, offset_log_append, offset_log_append_batch, offset_log_iter, offset_log_decode
}

criterion_group! {
name = mem_log;
config = Criterion::default().sample_size(10);
targets = mem_log_get, mem_log_append, mem_log_iter
}

criterion_main!(offset_log, mem_log);
