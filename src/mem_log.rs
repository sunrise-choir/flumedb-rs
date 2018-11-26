use flume_log::*;

use std::fs::File;
use std::io::{SeekFrom, BufReader};
use std::io::prelude::*;
use std::collections::BTreeMap;
use std::iter::IntoIterator;

pub struct FileLog 
{
   log: BufReader<File>, 
   length_at_seq: BTreeMap<usize, usize>,
   latest: usize
}

impl FileLog {
    fn new(path: String)-> FileLog{

        let f = File::open(path)
            .expect("Unable to open file");
        let log = BufReader::new(f);


        let length_at_seq = BTreeMap::new(); 

        FileLog{
            log,
            length_at_seq,
            latest: 0
        }
    }
}

impl FlumeLog for FileLog {
    //TODO: errors.
    fn get(&mut self, seq_num: usize) -> Result<Vec<u8>, ()> {
        let length = self.length_at_seq.get(&seq_num)
            .ok_or(())?;

        let mut vec = Vec::with_capacity(*length);

        //TODO: do we need to do this?
        unsafe { vec.set_len(*length) };

        self.log.seek(SeekFrom::Start(seq_num as u64))
            .or(Err(()))?;
        self.log.read(&mut vec)
            .or(Err(()))?;

        Ok(vec)
    }
    fn latest(&self) -> usize {
        self.latest
    }
    fn clear(&mut self, seq: usize) {
        unimplemented!()
    
    }
    fn append(& mut self, buff: &[u8]) -> usize {
        let seq = self.latest;
        self.length_at_seq.insert(seq, buff.len());
        self.latest += buff.len();
        unimplemented!()
    }
}

impl Iterator for FileLog {
    type Item=Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
    
        unimplemented!()
    }
}

pub struct MemLog 
{
   log: Vec<Vec<u8>>, 
}

impl MemLog {
    fn new()->MemLog{
        let log = Vec::new();
        MemLog{
            log 
        }
    }
}

impl FlumeLog for MemLog {
    //TODO: errors.
    fn get(&mut self, seq_num: usize) -> Result<Vec<u8>, ()> {
        let slice = self.log.get(seq_num).ok_or(())?;
        Ok(slice.clone())
    }
    fn clear(&mut self, seq: usize) {
        self.log[seq] = Vec::new();
    }
    fn latest(&self) -> usize {
        self.log.len()
    }
    fn append(& mut self, buff: &[u8]) -> usize {

        let seq = self.latest();
        let mut vec = Vec::new();
        vec.extend_from_slice(buff);

        self.log.push(vec);

        seq
    }

}

impl<'a> IntoIterator for &'a MemLog {
    type Item = &'a Vec<u8>;
    type IntoIter = std::slice::Iter<'a, Vec<u8>>;

    fn into_iter(self) -> Self::IntoIter{
        self.log.iter()
    }
}

#[cfg(test)]
mod tests {
    use mem_log::MemLog;
    use flume_log::*;
    #[test]
    fn get() {
        let mut log = MemLog::new();
        let seq0 = log.append("Hello".as_bytes());

        match log.get(seq0) {
            Ok(result) => {
                assert_eq!(String::from_utf8_lossy(&result), "Hello")
            },
            _ => assert!(false)
        }
    }
    #[test]
    fn clear() {
        let mut log = MemLog::new();
        let seq0 = log.append("Hello".as_bytes());
        log.clear(seq0);
        match log.get(seq0) {
            Ok(result) => {
                assert_eq!(result.len(), 0);
            },
            _ => assert!(false)
        }
    }
    #[test]
    fn iter() {
        let mut log = MemLog::new();
        let seq0 = log.append("Hello".as_bytes());
        log.append(" ".as_bytes());
        log.append("World".as_bytes());

        let result = log.into_iter()
            .map(|bytes|String::from_utf8_lossy(bytes))
            .fold(String::new(), |mut acc: String, elem |{
                acc.push_str(&elem);
                acc 
            });

        assert_eq!(result, "Hello World", "Expected Hello World, got {}", result);

        match log.get(seq0) {
            Ok(result) => {
                assert_eq!(String::from_utf8_lossy(&result), "Hello")
            },
            _ => assert!(false)
        }
    }
}
