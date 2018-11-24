pub struct StreamOpts{
    pub lt: String,
    pub gt: String,
    pub reverse: bool,
    pub live: bool,
    pub limit: usize,
}

pub trait FlumeLog 
{
    //TODO: errors.
    fn get(&mut self, seq_num: usize) -> Result<Box<[u8]>, ()>;
    fn latest(&self) -> usize;
    fn append(& mut self, buff: &[u8]) -> usize;
}

