pub struct StreamOpts {
    pub lt: String,
    pub gt: String,
    pub reverse: bool,
    pub live: bool,
    pub limit: usize,
}

pub trait FlumeLog {
    //TODO: errors.
    fn get(&mut self, seq_num: u64) -> Result<Vec<u8>, ()>;
    fn clear(&mut self, seq_num: u64);
    fn latest(&self) -> u64;
    fn append(&mut self, buff: &[u8]) -> Result<u64, ()>;
}
