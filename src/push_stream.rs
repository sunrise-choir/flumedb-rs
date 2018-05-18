use futures::Stream;
use futures::{Sink, Poll, Async};
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::mpsc::channel;
use std::thread;

use obv::Obv;

pub struct PushStream<T>{
   //queue of latest values
   rx: Receiver<T>,
   pub tx: Sender<T>,
}

impl<T : Clone> PushStream<T>{
    pub fn new() -> PushStream<T>{
        let queue = VecDeque::<T>::new(); 
        let (tx, rx) = channel::<T>();
        PushStream{tx, rx}
    }
}

impl<T> Stream for PushStream<T> {
    type Error = ();
    type Item = T; 

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error>{
        let item = match self.rx.try_recv(){
            Ok(i) => Async::Ready(Some(i)),
            Err(_) => Async::NotReady
        };
        Ok(item)
    }
}
#[cfg(test)]
mod tests {
}
