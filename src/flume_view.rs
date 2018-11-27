pub trait FlumeView<I> {
    fn append(&mut self, seq: usize, item: &I);
    fn latest(&self) -> usize;
}
