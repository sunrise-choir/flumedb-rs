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

TODO diagram

> In flume, each view remembers a version number, and if the version number changes, it just rebuilds the view. This means view code can be easily updated, or new views added. It just rebuilds the view on startup. (though, this may take a few minutes on larger data)

## Example

TODO

## Interfaces

### Write-only Log

TODO

`WriteOnlyLog({}) -> { append, getLatest }`

### Read-only Log

TODO

`ReadOnlyLog({}) -> { append, toIter, ...? }`

### View

TODO

`View({ readOnlyLog }) -> { process, getLatest, ... }`

## [Documentation](http://sunrise-choir.github.io/flumedb-rs/flumedb/)
