
use flume_log::*;

struct FlumeDb<L: FlumeLog>{
    log: L
}

impl<L: FlumeLog> FlumeDb<L> {
    fn new() -> FlumeDb<L> {
        unimplemented!()
    
    }
}

