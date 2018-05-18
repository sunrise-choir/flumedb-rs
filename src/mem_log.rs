use futures::stream::{iter_ok};
use futures::Stream;
use futures::{Sink, Poll, Async};
use futures::sync::mpsc::channel;
use std::sync::mpsc;
use futures::future::{ok};
use obv::Obv;
use flume_log::*;
use push_stream::*;
use std::thread;


pub struct MemLog<T> 
{
   log: Vec<T>, 
   since: Obv<Option<usize>>,
   latest: Obv<Option<T>>,
}

impl<T> MemLog<T> {
    fn new(log: Vec<T>) -> MemLog<T>{
        let seq = match log.len() {
            0 => None,
            x => Some(x-1)
        };
        let since = Obv::<Option<usize>>::new(seq);
        let latest = Obv::<Option<T>>::new(None);
        MemLog{log, since, latest}
    }
}



impl<T : 'static + Clone> FlumeLog for MemLog<T>
{
    type Item = T;
    fn get(&self, seq_num: usize) -> BoxedItemFuture<T>{
        let item = self.log.get(seq_num);
        let res = match item {
            Some(i) => Some((*i).clone()),
            None => None
        };
        Box::new(ok::<Option<T>, i32>(res))
    }

    fn append(& mut self, item: T) -> BoxedSequenceFuture {
        self.log.push(item.clone());
        let latest_seq = self.log.len() -1;
        self.since.set(Some(latest_seq));
        self.latest.set(Some(item));
        Box::new(ok::<usize, i32>(latest_seq))
    }

    fn stream(& mut self, opts: StreamOpts) -> BoxedLogStream<T> {
        if !opts.live{
            Box::new(iter_ok::<_, ()>(self.log.clone()))
        } else{ 
            let live = PushStream::<T>::new();
            let tx = live.tx.clone();
            
            self.latest.subscribe(Box::new(move |option|{
                let mine = option.clone();
                if let Some(item) = mine {
                    tx.send(item);
                }
            }));
            Box::new(iter_ok::<_, ()>(self.log.clone()).chain(live))
        } 
    }

    fn since(& mut self) -> & mut Obv<Option<usize>>{
        & mut self.since
    }
}


#[cfg(test)]
mod tests {
    use mem_log::MemLog;
    use flume_log::*;
    use futures::prelude::*;
    use futures::Future;
    use futures::Stream;
    use futures::future;
    use futures_cpupool::CpuPool;
    use futures::sync::mpsc::channel;
    use serde;
    use serde_json;

    #[test]
    fn get() {
        let vec = vec![1,2,3];
        let log: MemLog<i32> = MemLog::new(vec);
        let res = log.get(0)
            .wait();
        assert_eq!(res.unwrap(), Some(1))
    }

    #[test]
    fn append_returns_a_sequence_value(){
        let vec = Vec::new();
        let mut log: MemLog<i32> = MemLog::new(vec);
        let seq = log.append(1)
            .wait();
        assert_eq!(seq.unwrap(), 0);
    }
    #[test]
    fn append_stores_val_in_log(){
        let vec = Vec::new();
        let mut log: MemLog<i32> = MemLog::new(vec);
        let res = log.append(1000)
            .and_then(|seq| log.get(seq))
            .wait();
        assert_eq!(res.unwrap(), Some(1000));
    }
    #[test]
    fn since(){
        let vec = Vec::new();
        let mut log: MemLog<i32> = MemLog::new(vec);
        {
            let since = log.since();
            assert_eq!(*since.get(), None); //empty log has value seq None

            //TODO:: improve this test by using channels like in obv
            since.subscribe(Box::new(|seq| assert_eq!(seq.unwrap(), 0)));
        }

        let _ = log.append(1000)
            .wait();
    }
    #[test]
    fn stream_not_live_all(){
    
        let vec = vec![1,2,3];
        let mut log: MemLog<i32> = MemLog::new(vec);

        let sum = log.stream(StreamOpts::new())
            .fold(0, |acc, x| future::ok::<i32, ()>(acc + x))
            .wait();
        assert_eq!(sum, Ok(6));
    }
    #[test]
    fn stream_is_live(){
        let expected = 4;
        let vec = vec![];
        let mut log: MemLog<i32> = MemLog::new(vec);
        let mut opts = StreamOpts::new();
        opts.live = true;

        let mut stream = log.stream(opts);

        let _ = log.append(expected);
        match stream.poll() {
            Ok(Async::Ready(Some(val))) => assert_eq!(val, expected),
            _ => assert!(false)
        }
        match stream.poll() {
            Ok(Async::Ready(Some(val))) => assert!(false),
            _ => assert!(true)
        }

        let _ = log.append(expected);
        match stream.poll() {
            Ok(Async::Ready(Some(val))) => assert_eq!(val, expected),
            _ => assert!(false)
        }
        match stream.poll() {
            Ok(Async::Ready(Some(val))) => assert!(false),
            _ => assert!(true)
        }
    }
    //#[test]
    //fn serde(){
    //    #[derive(Serialize, Deserialize)]
    //    struct Address {
    //        street: String,
    //        city: String,
    //    }

    //    let addy = Address{
    //        street: "derp".to_owned(),
    //        city: "wellington".to_owned()
    //    };

    //    let bytes = serde_json::to_bytes(&addy).unwrap();

    //}

}
