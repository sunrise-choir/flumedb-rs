use flume_log::*;

use std::iter::IntoIterator;

pub struct MemLog {
    log: Vec<Vec<u8>>,
}

impl MemLog {
    pub fn new() -> MemLog {
        let log = Vec::new();
        MemLog { log }
    }
}

impl FlumeLog for MemLog {
    fn get(&self, seq_num: u64) -> Result<Vec<u8>, Error> {
        self.log
            .get(seq_num as usize)
            .map(|slice| slice.clone())
            .ok_or(FlumeLogError::SequenceNotFound { sequence: seq_num }.into())
    }
    fn clear(&mut self, seq: u64) {
        self.log[seq as usize] = Vec::new();
    }
    fn latest(&self) -> Option<u64> {
        if self.log.len() == 0 {
            None
        } else {
            Some(self.log.len() as u64 - 1)
        }
    }
    fn append(&mut self, buff: &[u8]) -> Result<u64, Error> {
        let seq = self.log.len();
        let mut vec = Vec::new();
        vec.extend_from_slice(buff);

        self.log.push(vec);

        Ok(seq as u64)
    }
}

impl<'a> IntoIterator for &'a MemLog {
    type Item = &'a Vec<u8>;
    type IntoIter = std::slice::Iter<'a, Vec<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        self.log.iter()
    }
}

#[cfg(test)]
mod tests {
    use flume_log::*;
    use mem_log::MemLog;
    #[test]
    fn get() {
        let mut log = MemLog::new();
        let seq0 = log.append("Hello".as_bytes()).unwrap();

        match log.get(seq0) {
            Ok(result) => assert_eq!(String::from_utf8_lossy(&result), "Hello"),
            _ => assert!(false),
        }
    }
    #[test]
    fn clear() {
        let mut log = MemLog::new();
        let seq0 = log.append("Hello".as_bytes()).unwrap();
        log.clear(seq0);
        match log.get(seq0) {
            Ok(result) => {
                assert_eq!(result.len(), 0);
            }
            _ => assert!(false),
        }
    }
    #[test]
    fn iter() {
        let mut log = MemLog::new();
        let seq0 = log.append("Hello".as_bytes()).unwrap();
        log.append(" ".as_bytes()).unwrap();
        log.append("World".as_bytes()).unwrap();

        let result = log
            .into_iter()
            .map(|bytes| String::from_utf8_lossy(bytes))
            .fold(String::new(), |mut acc: String, elem| {
                acc.push_str(&elem);
                acc
            });

        assert_eq!(
            result, "Hello World",
            "Expected Hello World, got {}",
            result
        );

        match log.get(seq0) {
            Ok(result) => assert_eq!(String::from_utf8_lossy(&result), "Hello"),
            _ => assert!(false),
        }
    }
}
