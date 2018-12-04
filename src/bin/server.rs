extern crate flumedb;
extern crate jsonrpc_core;
extern crate jsonrpc_http_server;

use jsonrpc_core::*;
use jsonrpc_http_server::*;


fn main() {

    //Pseudo code of server start up
    //
    // - file path.
    // - load log with path, gets the latest seq value. 
    // - load persisted views. 
    // - rebuild views that need it  
    //   - if they've just been created they need the whole log.
    //   - or if view and log out of sync just rebuild as a first pass. 
    // -  
    // - move it all into a thread that polls channels 
    //   - where do the views live?
    //   - how do we satisfy the usecase where views are getting built and a client wants to query
    //   a view? Most of the time one whole new view might be rebuilt, or maybe we're doing initial
    //   sync, but in that case the bottleneck shouldn't be flume, it'll be crypto and calls to
    //   query a view should still get serviced.
    // - 


    let mut io = IoHandler::new();
    io.add_method("get_by_seq", |_: Params| {
        Ok(Value::String("hello".to_string()))
    });

    io.add_method("get_by_key", |_: Params| {
        Ok(Value::String("hello".to_string()))
    });
    let server = ServerBuilder::new(io)
        .start_http(&"127.0.0.1:3030".parse().unwrap())
        .unwrap();

    server.wait()
}
