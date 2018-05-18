use futures::prelude::*;

use obv::Obv;

pub enum StreamSelector {
    Lt,
    Lte,
    Gt,
    Gte,
    All,
}

pub struct StreamOpts{
    pub selector: StreamSelector,
    pub reverse: bool,
    pub live: bool,
    pub limit: usize,
}

impl StreamOpts {
    pub fn new() -> StreamOpts {
        StreamOpts{
            selector: StreamSelector::All,
            reverse: false,
            live: false,
            limit: 0
        }
    }
}

pub type Id = usize;
pub type IdItemTuple<T> = (Id, T);

pub type SequenceFuture = Future<Item=usize,Error=i32>;
pub type BoxedSequenceFuture = Box<SequenceFuture>;
pub type ItemFuture<T> = Future<Item=Option<IdItemTuple<T>>,Error=i32>;
pub type BoxedItemFuture<T> = Box<ItemFuture< T>>;
pub type LogStream<T> = Stream<Item=IdItemTuple<T>, Error=()>;
pub type BoxedLogStream<T> = Box<LogStream<T>>;

pub trait FlumeLog 
{
    type Item;
    fn get(&self, seq_num: usize) -> BoxedItemFuture<Self::Item>;
    fn stream(& mut self, opts: StreamOpts) -> BoxedLogStream<Self::Item>;
    fn since(& mut self) -> & mut Obv<Option<usize>>;
    fn append(& mut self, item: Self::Item) -> BoxedSequenceFuture;
}

