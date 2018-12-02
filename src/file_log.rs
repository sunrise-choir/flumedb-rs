use flume_log::*;

use std::fs::File;
use std::io::{SeekFrom};
use std::io::prelude::*;
use std::collections::BTreeMap;
use std::iter::IntoIterator;

pub struct FileLog 
{
   log: File, 
   offsets_file: File,
   offsets: BTreeMap<usize, usize>,
   latest: usize
}

impl FileLog {
    fn new(path: String)-> FileLog{

        //TODO fix the paths here.
        let log = File::open(path.clone())
            .expect("Unable to open file");

        let offsets_file = File::open(path)
            .expect("Unable to open file");
        
        let offsets = BTreeMap::new(); 

        FileLog {
            log,
            offsets,
            offsets_file,
            latest: 0
        }
    }
    fn load_from_file(path: String) -> FileLog{
        let log = File::open(path)
            .expect("Unable to open file");

        unimplemented!()
    }
    fn store(&self) -> Result<(),()> {
        unimplemented!()
    
    }
}

impl FlumeLog for FileLog {
    //TODO: errors.
    fn get(&mut self, seq_num: usize) -> Result<Vec<u8>, ()> {
        let length = self.offsets.get(&seq_num)
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
    fn append(& mut self, buff: &[u8]) -> Result<usize, ()> {
        let seq = self.latest;
        self.offsets.insert(seq, buff.len());
        self.latest += buff.len();
        self.log
            .seek(SeekFrom::End(0))
            .and_then(|_| self.log.write(buff))
            .or(Err(()))
    }
}

impl Iterator for FileLog {
    type Item=Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
    
        unimplemented!()
    }
}
