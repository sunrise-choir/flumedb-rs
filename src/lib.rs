//!
//!# flumedb
//!
//!## flumedb.get (seq) -> Future<(Item, seq)> 
//!
//!This exposes `get` from the underlying `flumelog` module.
//!This takes a `seq` as is the keys in the log,
//!and returns the value at that sequence or an error.
//!
//!## flumedb.stream (opts)
//!
//!This exposes `stream` from the underlying `flumelog` module.
//!It supports options familiar from leveldb ranges, i.e. `lt`, `gt`, `lte`, `gte`, `reverse`, `live`, `limit`.
//!
//!## flumedb.append (value, cb(err, seq))
//!
//!Appends a value to the database. The encoding of this value will be handled by the `flumelog`,
//!will callback when value is successfully written to the log (or it errors).
//!On success, the callback will return the new maximum sequence in the log.
//!
//!If `value` is an array of values, it will be treated as a batched write.
//!By the time of the callback, flumedb.since will have been updated.
//!
//!## flumedb.use(name, createFlumeview) => self
//!
//!Installs a `flumeview` module.
//!This will add all methods that the flumeview describes in it's `modules` property to `flumedb[name]`
//!If `name` is already a property of `flumedb` then an error will be thrown.
//!You probably want to add plugins at startup, but given the streaming async nature of flumedb,
//!it will work completely fine if you add them later!
//!
//!## flumedb.rebuild (cb)
//!
//!Destroys all views and rebuilds them from scratch. If you have a large database and lots of views
//!this might take a while, because it must read completely through the whole database again.
//!`cb` will be called once the views are back up to date with the log. You can continue using
//!the various read APIs and appending to the database while it is being rebuilt,
//!but view reads will be delayed until those views are up to date.
//!
//!## flumedb.close(cb)
//!
//!closes all flumeviews.
//!
//!## flumedb.closed => boolean
//!
//!set to true if `flumedb.close()` has been called
//!
//!# flumelog
//!
//!The log is the heart of `flumedb`, it's the cannonical store of data, and all state is stored there.
//!The views are generated completely from the data in the log, so they can be blown away and regenerated easily.
//!
//!## flumelog.get (seq, cb)
//!
//!Retrives the value at position `seq` in the log.
//!
//!## flumelog.stream(opts)
//!
//!Returns a source stream over the log.
//!It supports options familiar from leveldb ranges, i.e. `lt`, `gt`, `lte`, `gte`, `reverse`, `live`, `limit`.
//!
//!## flumelog.since => observable
//!
//!An observable which represents the state of the log. If the log is uninitialized
//!it should be set to `undefined`, and if the log is empty it should be `-1` if the log has data,
//!it should be a number larger or equal to zero.
//!
//!
//!## flumelog.append (value, cb(err, seq))
//!
//!Appends a value (or array of values) to the log, and returns the new latest sequence.
//!`flumelog.since` is updated before calling `cb`.
//!
//!## flumelog.dir => directory name
//!
//!The filename of the directory this flumelog is persisted in
//!(this is used by the views, if they are also persistent).
//!
//!# flumeview
//!
//!A `flumeview` provides a read API, and a streaming sink that accepts data from the log
//!(this will be managed by `flumedb`).
//!
//!## flumeview.since => observable
//!
//!The current state of the view (this must be the sequence of the last value processed by the view)
//!a flumeview _must_ process items from the main log in order, otherwise inconsistencies will occur.
//!
//!## flumeview.createSink (cb)
//!
//!Returns a pull-stream sink that accepts data from the log. The input will be `{seq, value}` pairs.
//!`cb` will be called when the stream ends (or errors).
//!
//!## flumeview.destroy (cb)
//!
//!Wipe out the flumeview's internal (and persisted) state, and reset `flumeview.since` to `-1`.
//!
//!## flumeview.close (cb)
//!
//!flush any pending writes and release resources, etc.
//!
//!## flumeview.methods = {\<key\>:'sync'|'async'|'source'}
//!
//!An object describing the methods exposed by this view.
//!A view needs to expose at least one method
//!(otherwise, why is it useful?).
//!
//!These the corresponding methods will be added at `flumedb[name][key]`.
//!If they type is `async` or `source` the actual call to the `flumeview[key]` method will
//!be delayed until `flumeview.since` is in up to date with the log.
//!`sync` type methods will be called immediately.
//!
//!## flumeview[name].ready (cb)
//!
//!A `ready` method is also added to each mounted `flumeview` which takes a callback
//!which will be called exactly once, when that view is up to date with the log
//!(from the point where it is called).

extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_io;
extern crate bytes;
extern crate byteorder;

#[macro_use]
extern crate serde_derive;

pub mod offset_log;
pub use offset_log::*;

pub mod flume_log;
pub mod file_log;
pub mod flume_db;
pub mod mem_log;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
