# Flumedb-rs

## Resync

- Mikey asserted that the abstraction is a good one.
  - An append only log.
  - Multiple materialized views of the log.
  - I'm not sure this is the correct abstraction.
    - We use ssb as a key value store. The `seq` part doesn't belong in the higher level api.
    - It builds indexes. And it's not really useful without the indexes. As far as I can tell, without the indexes, just dumping the raw log is not used by much (anything) so not sure if the ordering straight off the log is actually important.
    - I think the thing that alj and others are uncomfortable about is flume is that the flume api is becoming part of the scuttlebot api. Or at least flume views are.
    - how is flume exposed in scuttlebot?
    - how are the indexes actually used?




## Braindump

- Check out what my obs type did. There's also that `data_tracker` module.
- The reader trait _looks_ promising.
  - Is it defo blocking?
  - What does it do across streams?
  - If we take a ref that means the thing we have a ref to is locked. So need to think about where that happens.
  - Can we do multiple readers on a single log for a pubsub pattern?
  - I _think_ reader implements some sort of `read_from` thing so that an index can just start from its last updated spot.

- So the interesting thing is that if you query a view, it should be async and not call back until the view is in sync with the log.

- Can this just be one trait than manages this state internally?

- Could be fun to write some possible example chunks to see what's ergonomic.

Ok so it seems like this should all be built in Tokio. We get futures and streams. I can build an observable out of a stream and a callback.

- tokio should give good guidance on how to manage owenership through a stream.
- the `Cell` type could be a good idea for tracking the since variable?

There's also the thing about codecs and when that should happen.
I think tokio has tools for that too.

Possible ways forward:
- write the test vectors.
- write the example code.
- use `unimplemented!` and write the function sigs and see if it compiles.
 

Example

```rs
  let sumView = FlumeView::new({
     
  });

  let mut db = FlumeMem::new(Flume::Codecs::JSON);

  db
    .subscribe() //this needs to get a clone of the stream or something. I think it will add the stream's Sink to a list for pushing values into. And then it returns the Source.
    .for_each(|element|{
      println!("db had an element {:?} added at seq {:?}", element.val, element.seq); 
    });

  let seq = 2323;

  db
    .get(seq) //does this need to be a future really, it'll all be sync / based on internal values.


```
