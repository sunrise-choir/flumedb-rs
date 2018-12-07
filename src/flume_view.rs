pub use flume_log::Sequence;

pub trait FlumeView {
    fn append(&mut self, seq: Sequence, item: &[u8]);
    fn latest(&self) -> Sequence;
}
