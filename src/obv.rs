use std::collections::HashMap;

pub struct Obv<T>{
    id_count: i32,
    value: T,
    subscribers: HashMap<i32, Box<Fn(&T)>>,
}

impl<T> Obv<T>{
    pub fn new(value: T) -> Obv<T>{
        Obv{id_count: 0, value, subscribers: HashMap::new()}
    }
    pub fn set(&mut self, new_value: T){
        self.value = new_value; 
        let subs = &self.subscribers;
        for sub in subs.values(){
            sub(&self.value);
        };
    }
    pub fn get(&self) -> &T{
        &self.value 
    }
    pub fn subscribe(&mut self, subscriber: Box<Fn(&T)>) -> i32{
        self.id_count += 1;
        &self.subscribers.insert(self.id_count, subscriber);
        self.id_count.clone()
    }
    pub fn unsubscribe(&mut self, id: &i32){
        &self.subscribers.remove(&id);
    }
}

#[cfg(test)]    
mod test {
    use obv::Obv;
    use std::sync::mpsc::{Sender, Receiver};
    use std::sync::mpsc;

    #[test]
    fn obv_get_returns_value(){
        let obv: Obv<i32> = Obv::new(3);
        assert!(*obv.get() == 3);
    }

    #[test]
    fn obv_value_value_is_updateable(){
        let mut obv: Obv<i32> = Obv::new(3);
        assert!(obv.value == 3);
        obv.value = 2;
        assert!(obv.value == 2);
    }

    #[test]
    fn set_updates_value(){
        let mut obv: Obv<i32> = Obv::new(3);
        assert!(obv.value == 3);
        obv.set(2);
        assert!(obv.value == 2);
    }

    #[test]
    fn set_updates_value_and_calls_subscriber(){
        let (tx, rx): (Sender<i32>, Receiver<i32>) = mpsc::channel();
        let mut obv: Obv<i32> = Obv::new(3);
        assert!(obv.value == 3);
        obv.subscribe(Box::new(move |val| {
            assert!(*val == 2);
            tx.send(1).unwrap();
        }));
        obv.set(2);
        assert!(rx.recv().unwrap() == 1);
    }

    #[test]
    fn set_updates_value_and_calls_subscribers(){
        let (tx, rx): (Sender<i32>, Receiver<i32>) = mpsc::channel();
        let other_tx = tx.clone();

        let mut obv: Obv<i32> = Obv::new(3);
        assert!(obv.value == 3);
        obv.subscribe(Box::new(move |val| {
            assert!(*val == 2);
            tx.send(1).unwrap();
        }));
        obv.subscribe(Box::new(move |val| {
            assert!(*val == 2);
            other_tx.send(1).unwrap();
        }));
        obv.set(2);
        assert!(rx.recv().unwrap() == 1);
        assert!(rx.recv().unwrap() == 1);
    }

    #[test]
    fn subscribe_returns_an_id_for_unsubscribe(){
        let mut obv: Obv<i32> = Obv::new(3);
        assert!(obv.value == 3);
        let id = obv.subscribe(Box::new(move |_| {
            assert!(false);
        }));
        obv.unsubscribe(&id);
        obv.set(2);
    }

    #[test]
    fn calling_unsubscribe_twice_is_ok(){
        let mut obv: Obv<i32> = Obv::new(3);
        assert!(obv.value == 3);
        let id = obv.subscribe(Box::new(move |_| {
            assert!(false);
        }));
        obv.unsubscribe(&id);
        obv.unsubscribe(&id);
        obv.set(2);
    }
}
