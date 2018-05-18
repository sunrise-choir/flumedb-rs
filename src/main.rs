
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

extern crate tokio;
extern crate tokio_io;
extern crate tokio_fs;
extern crate bytes;
extern crate byteorder;
extern crate flumedb;

use flumedb::offset_log::*;
use tokio_io::codec::FramedRead;
use tokio::fs::File;
use tokio::runtime::Runtime;
use tokio::prelude::*;
use serde_json::{Value};

fn main() {
}
