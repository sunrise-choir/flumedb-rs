use flume_log::*;

use std::fs::File;
use std::io::SeekFrom;
use std::io::prelude::*;
use std::collections::BTreeMap;
use std::iter::IntoIterator;

pub struct FileLog 
{
   log: File, 
   length_at_seq: BTreeMap<usize, usize> 
}

impl FlumeLog for FileLog {
    //TODO: errors.
    fn get(&mut self, seq_num: usize) -> Result<Box<[u8]>, ()> {
        let length = self.length_at_seq.get(&seq_num)
            .ok_or(())?;

        let mut vec = Vec::with_capacity(*length);

        //TODO: do we need to do this?
        unsafe { vec.set_len(*length) };

        self.log.seek(SeekFrom::Start(seq_num as u64))
            .or(Err(()))?;
        self.log.read(&mut vec)
            .or(Err(()))?;

        Ok(vec.into_boxed_slice())
    }
    fn latest(&self) -> usize {

        unimplemented!()
    }
    fn append(& mut self, buff: &[u8]) -> usize {
        unimplemented!()
    }
}

impl Iterator for FileLog {
    type Item=Box<[u8]>;

    fn next(&mut self) -> Option<Self::Item> {
    
        unimplemented!()
    }
}




pub struct MemLog 
{
   log: Vec<Vec<u8>>, 
}

impl FlumeLog for MemLog {
    //TODO: errors.
    fn get(&mut self, seq_num: usize) -> Result<Box<[u8]>, ()> {
        let slice = self.log.get(seq_num).ok_or(())?;
        Ok(slice.clone().into_boxed_slice())
    }
    fn latest(&self) -> usize {
        self.log.len()
    }
    fn append(& mut self, buff: &[u8]) -> usize {

        let mut vec = Vec::new();
        vec.extend_from_slice(buff);

        self.log.push(vec);
        self.latest()
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
    #[test]
    fn get() {
        let vec = vec![1,2,3];
    }
}
