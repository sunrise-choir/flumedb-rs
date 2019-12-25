[![Build Status](https://travis-ci.com/sunrise-choir/flumedb-rs.svg?branch=master)](https://travis-ci.com/sunrise-choir/flumedb-rs)

# FlumeDB

The [Sunrise Choir](https://sunrisechoir.com) Flume is a re-write of [the JavaScript `flumedb`](https://github.com/flumedb/flumedb) into Rust with a new architecture for better performance and flexibility.

## Architecture

Flume is a modular database:

- our main source of truth is an append-only write-only log: which provides durable storage
- our secondary derived truths are views on the log: which focus on answering queries about the data
  - each view receives data through a read-only copy of the append-only log
  - each view can build up their own model to represent the data, either to point back to the main source of truth (normalized) or to materialize it (denormalized)
  - each view creates a structure optimized for the queries they provide, they don't need to be durable because they can always rebuild from the main log

> In flume, each view remembers a version number, and if the version number changes, it just rebuilds the view. This means view code can be easily updated, or new views added. It just rebuilds the view on startup.

## Example

```rust
use flumedb::Error;
use flumedb::OffsetLog;

fn main() -> Result<(), Error> {
    let path = shellexpand::tilde("~/.ssb/flume/log.offset");
    let log = OffsetLog::<u32>::open_read_only(path.as_ref())?;

    // Read the entry at offset 0
    let r = log.read(0)?;
    // `r.data` is a json string in a standard ssb log.
    // `r.next` is the offset of the next entry.
    let r = log.read(r.next);

    log.iter()
    .map(|e| serde_json::from_slice::<serde_json::Value>(&e.data).unwrap())
    .for_each(|v| println!("{}", serde_json::to_string_pretty(&v).unwrap()));

    Ok(())
}
```
