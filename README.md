# Flumedb-rs

## Resync

- Mikey asserted that the abstraction is a good one.
  - An append only log.
  - Multiple materialized views of the log.
  - This is a good abstraction for an append only log. But scuttlebot doesn't have to be an append only log in the db. I _think_ the api scuttlebutt expects is really more like a key value store.
    - We use ssb as a key value store. The `seq` part of flume doesn't belong in the higher level api.
    - Flume builds indexes. And it's not really useful without the indexes. As far as I can tell, without the indexes, just dumping the raw log is not used by much (anything) so not sure if the ordering straight off the log is actually important.
    - how are the indexes actually used? I _still_ don't get Dominic's documentation.

## Sql experiments

- Store all content and author keys raw takes 7.7s, 124M
- Store all content, author by key, 8.4s, 115M
- Store no content, author by key, 7.7s, 35M

## Indexing comparison:
`rm backlinks-* feed keys.ht links links2 query -rf` => 311s to rebuild indices
15s for the equivalent. 20 * faster.

## Noticings

- the log works fine just dealing in bytes. It doesn't need to care about deserializing stuff. But the views _do_ care about deserialized data. There isn't much you can do in a view on just raw bytes. But maybe there are use cases for it?
  - a view might just be some state with an `append` method that expects a derserialized type.
    - hashmap-view needs to be ask about the hash / key of the thing.
    - about needs to be able to go deep into the structure of the message.
    - feeds needs to be able to drill down to value.author
    - first pass is that we just use serde's JSValue type and check if there's an eqivalent type in the cbor crate => it does.

- still an open question about where or when serde should happen. A log deals in bytes. But it might deal in types?

- what is the api that scuttlbutt wants?
  - how could I wire up things like level, flume, sql to it?
    - level would be really easy surely? most of the api is straight from level.


- very important thing to come out of conversation yesterday was that Mikey wants bindings for flume so we can get it in use asap.
  - we'd have to make rust views for some core parts.
  - we'd have to allow for js being able to register views too.
    - there would need to be a stream that output buffers that can be serialized by the flume codec. (is fine).
- I'd like help making a board with milestones so I can celebrate work as I hit it. 
  - maybe flume offset log is a good milestone?
    - either way, we need offset log.
      - it needs to be able to seek.
      - it needs to be able to append.
      - 

- ssb-about uses ssb-backlinks which uses flumeview-query/links which uses flumeview-level and flumeview-query. Query uses map-filter-reduce.

- what would matt want?
  - what functionality does it need for you to implement patchwork on top of it
  - if we implement it, would you use it?
  - what functionality to implement.
  - some sort of backlinks 
  - some sort of query things
  - is it good to do flumeview query?
  - work with him to figure out the alternative.

  - alj , mikey, mix, cel, matt.

  - possible view implementations:
    - mfr queries => boxed thing that takes an iter and returns you a Type that is the built view.
      - this is good because there aren't heaps of js objects crossing the js / rust boundary.
      - could be an easy add to flume as a reader, not writer.
      - what things use flumeview-reduce? Which ones are important and could be ported to rust?

    
 

