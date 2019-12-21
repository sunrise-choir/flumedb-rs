#[derive(Debug)] // TODO: derive more traits
pub struct LogEntry {
    pub offset: u64,
    pub data: Vec<u8>,
}
