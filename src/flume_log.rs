pub use failure::Error;

pub struct StreamOpts {
    pub lt: String,
    pub gt: String,
    pub reverse: bool,
    pub live: bool,
    pub limit: usize,
}

#[derive(Debug, Fail)]
pub enum FlumeLogError {
    #[fail(display = "Unable to find sequence: {}", sequence)]
    SequenceNotFound { sequence: u64 },
}

pub type Sequence = u64;

pub trait FlumeLog {
    fn get(&self, seq: Sequence) -> Result<Vec<u8>, Error>;
    fn clear(&mut self, seq: Sequence);
    fn latest(&self) -> Sequence;
    fn append(&mut self, buff: &[u8]) -> Result<Sequence, Error>;
}
