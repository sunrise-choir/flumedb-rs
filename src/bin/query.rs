extern crate flumedb;

fn main() {

    let mut v = Vec::<i32>::new();
    v.push(1);
    v.push(2);
    v.push(3);
    v.push(4);
    v.push(5);

    let mut query: Box<Iterator<Item=i32>> = Box::new(v.into_iter());

    query = Box::new(query.map(|v: i32| -> i32 {v * 2}));
    query = Box::new(query.filter(|v: &i32| *v > 4));

    println!("map, filter:");
    query.for_each(|val|{
        println!("{}", val);
    
    });

    let mut v = Vec::<i32>::new();
    v.push(1);
    v.push(2);
    v.push(3);
    v.push(4);
    v.push(5);

    let mut query: Box<Iterator<Item=i32>> = Box::new(v.into_iter());
    query = Box::new(query.filter(|v: &i32| *v > 4));
    query = Box::new(query.map(|v: i32| -> i32 {v * 2}));

    println!("filter map:");
    query.for_each(|val|{
        println!("{}", val);
    
    })
}
