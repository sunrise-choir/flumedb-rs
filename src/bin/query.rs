extern crate flumedb;
use flumedb::*; 
use std::iter::Filter;

fn combine_iters<Item>(i1: &Iterator<Item=Item>, i2: &Iterator<Item=Item> ) {

}

fn main() {

    let mut v = Vec::<i32>::new();
    v.push(1);
    v.push(2);
    v.push(3);
    v.push(4);
    v.push(5);

    let mut query: Box<Iterator<Item=i32>> = Box::new(v.into_iter());

    query = Box::new(query.map(|v: i32| v * 2));
    query = Box::new(query.filter(|v| v > &4));

    query.for_each(|val|{
        println!("{}", val);
    
    })

}
