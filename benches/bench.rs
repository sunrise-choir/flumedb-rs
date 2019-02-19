#[macro_use]
extern crate criterion;

use criterion::*;

extern crate serde;
extern crate serde_json;

extern crate byteorder;
extern crate bytes;
extern crate flumedb;
extern crate tempfile;

use flumedb::flume_log::FlumeLog;
use flumedb::mem_log::MemLog;
use flumedb::offset_log::*;
use serde_json::{from_slice, Value};
use tempfile::tempfile;

const NUM_ENTRIES: usize = 10000;

static DEFAULT_TEST_BUF: &[u8] = b"{\"value\": 1}";

fn default_test_bufs() -> Vec<&'static [u8]> {
    vec![DEFAULT_TEST_BUF; NUM_ENTRIES]
}

fn temp_offset_log() -> OffsetLog<u32> {
    OffsetLog::<u32>::from_file(tempfile().unwrap()).unwrap()
}

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
    let buf = DEFAULT_TEST_BUF;
    c.bench_function("offset log append", move |b| {
        b.iter_batched(temp_offset_log,
                     |mut log| {
                         for i in 0..NUM_ENTRIES {
                             let offset = log.append(buf).unwrap() as usize;
                             assert_eq!(offset, i * (12 + buf.len()));
                         }
                     },
                     BatchSize::SmallInput);
    });
}

fn offset_log_append_batch(c: &mut Criterion) {

    let test_bufs = default_test_bufs();
    c.bench_function("offset log append batch - all", move |b| {
        b.iter_batched(temp_offset_log,
                       |mut log| {
                           let offsets = log.append_batch(&test_bufs).unwrap();
                           assert_eq!(offsets.len(), NUM_ENTRIES);
                           assert_eq!(offsets[0], 0);
                       },
                       BatchSize::SmallInput);
    });

    let test_bufs = default_test_bufs();
    c.bench_function("offset log append batch - 100", move |b| {
        b.iter_batched(temp_offset_log,
                       |mut log| {
                           for chunk in test_bufs.chunks(100) {
                               let offsets = log.append_batch(&chunk).unwrap();
                               assert_eq!(offsets.len(), chunk.len());
                           }
                       },
                       BatchSize::SmallInput);
    });

}

fn offset_log_get(c: &mut Criterion) {
    let mut log = temp_offset_log();
    let offsets = log.append_batch(&default_test_bufs()).unwrap();

    c.bench_function("offset log get", move |b| {
        b.iter(|| {
            for offset in offsets.iter() {
                let result = log.get(*offset).unwrap();
                assert_eq!(result.len(), DEFAULT_TEST_BUF.len());
            }
        })
    });
}

fn offset_log_iter(c: &mut Criterion) {

    // Forward
    let mut log = temp_offset_log();
    let offsets = log.append_batch(&default_test_bufs()).unwrap();

    c.bench_function("offset log iter forward", move |b| {
        b.iter_batched(|| log.iter(),
                       |mut iter| {
                           let count = iter.forward().count();
                           assert_eq!(count, offsets.len());
                       },
                       BatchSize::SmallInput)});

    // Backward
    let mut log = temp_offset_log();
    let offsets = log.append_batch(&default_test_bufs()).unwrap();

    c.bench_function("offset log iter backward", move |b| {
        b.iter_batched(|| log.iter_at_offset(log.end()),
                       |mut iter| {
                           let count = iter.backward().count();
                           assert_eq!(count, offsets.len());
                       },
                       BatchSize::SmallInput)});

    // Forward with json decoding
    let mut log = temp_offset_log();
    log.append_batch(&default_test_bufs()).unwrap();

    c.bench_function("offset log iter forward and json decode", move |b| {
        b.iter_batched(|| log.iter(),
                       |mut iter| {
                           let sum: u64 = iter.forward()
                               .map(|val| from_slice(&val.data).unwrap())
                               .map(|val: Value| match val["value"] {
                                   Value::Number(ref num) => {
                                       let r: u64 = num.as_u64().unwrap();
                                       r
                                   }
                                   _ => panic!(),
                               })
                               .sum();
                           assert_eq!(sum, NUM_ENTRIES as u64);
                       },
                       BatchSize::SmallInput)});

}

fn mem_log_get(c: &mut Criterion) {
    let mut log = MemLog::new();
    let test_buf = DEFAULT_TEST_BUF;
    let offsets: Vec<u64> = (0..NUM_ENTRIES)
        .map(|_| log.append(test_buf).unwrap())
        .collect();

    c.bench_function("mem log get", move |b| {
        b.iter(|| {
            for offset in offsets.iter() {
                let result = log.get(*offset).unwrap();
                assert_eq!(result.len(), test_buf.len());
            }
        })
    });
}

fn mem_log_append(c: &mut Criterion) {
    c.bench_function("mem log append", move |b| {
        b.iter(|| {
            let mut log = MemLog::new();

            let offsets: Vec<u64> = (0..NUM_ENTRIES)
                .map(|_| log.append(DEFAULT_TEST_BUF).unwrap())
                .collect();

            assert_eq!(offsets.len(), NUM_ENTRIES as usize);
        })
    });
}

fn mem_log_iter(c: &mut Criterion) {
    let mut log = MemLog::new();

    (0..NUM_ENTRIES).for_each(|_| {
        log.append(DEFAULT_TEST_BUF).unwrap();
    });

    c.bench_function("mem log iter and json decode", move |b| {
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
