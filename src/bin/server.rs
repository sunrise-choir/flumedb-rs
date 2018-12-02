extern crate flumedb;
extern crate jsonrpc_core;
extern crate jsonrpc_http_server;

use jsonrpc_core::*;
use jsonrpc_http_server::*;
use flumedb::*; 

fn main() {


    let mut io = IoHandler::new();
    io.add_method("get_by_seq", |params: Params| {
        Ok(Value::String("hello".to_string()))
    });

    io.add_method("get_by_key", |params: Params| {
        Ok(Value::String("hello".to_string()))
    });
    let server = ServerBuilder::new(io)
        .start_http(&"127.0.0.1:3030".parse().unwrap())
        .unwrap();

    server.wait()
}
