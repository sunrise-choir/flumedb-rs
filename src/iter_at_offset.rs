use log_entry::LogEntry;

pub trait IterAtOffset<I: Iterator<Item = LogEntry>> {
    fn iter_at_offset(&self, offset: u64) -> I;
}
