pub trait FlumeView {
    fn append(&mut self, seq: usize, item: &[u8]); 
    fn latest(&self) -> usize;
}
