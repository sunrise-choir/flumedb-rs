use std::error::Error;

pub struct StreamOpts {
    pub lt: String,
    pub gt: String,
    pub reverse: bool,
    pub live: bool,
    pub limit: usize,
}

pub type Sequence = u64;

pub trait FlumeLog {
    type Error: Error;

    fn get(&self, seq: Sequence) -> Result<Vec<u8>, Self::Error>;
    fn clear(&mut self, seq: Sequence);
    fn latest(&self) -> Option<Sequence>;
    fn append(&mut self, buff: &[u8]) -> Result<Sequence, Self::Error>;
}
