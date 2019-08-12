pub use bidir_iter::BidirIterator;

use buffered_offset_reader::{BufOffsetReader, OffsetRead, OffsetReadMut, OffsetWrite};
use byteorder::{BigEndian, ReadBytesExt};
use bytes::{BufMut, BytesMut};
use flume_log::*;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{Seek, SeekFrom};
use std::marker::PhantomData;
use std::mem::size_of;
use std::path::Path;

const DATA_FILE_NAME: str = "data"
const JOURNAL_FILE_NAME: str = "jrnl"
const OFFSET_FILE_NAME: str = "ofset"

#[derive(Debug, Fail)]
pub enum FlumeOffsetLogError {
    #[fail(display = "Incorrect framing values detected, log file might be corrupt")]
    CorruptLogFile {},

    #[fail(display = "The decode buffer passed to decode was too small")]
    DecodeBufferSizeTooSmall {},
}

pub struct GoOffsetLog<ByteType> {
    pub file: File,
    end_of_file: u64,
    last_offset: Option<u64>,
    tmp_buffer: BytesMut,
    byte_type: PhantomData<ByteType>,
}

// A Frame is like a LogEntry, but without the data
#[derive(Debug)]
pub struct Frame {
    pub offset: u64,
    pub data_size: usize
}

impl Frame {
    fn data_start(&self) -> u64 {
        self.offset + size_of::<u32>() as u64
    }
}

#[derive(Debug)] // TODO: derive more traits
pub struct LogEntry {
    pub offset: u64,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct ReadResult {
    pub entry: LogEntry,
    pub next: u64,
}


impl<ByteType> OffsetLog<ByteType> {

    pub fn new<P: AsRef<Path>>(path: P) -> Result<OffsetLog<ByteType>, Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;

        OffsetLog::from_file(file)
    }

    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<OffsetLog<ByteType>, Error> {
        let file = OpenOptions::new()
            .read(true)
            .open(&path)?;

        OffsetLog::from_file(file)
    }

    pub fn from_file(mut file: File) -> Result<OffsetLog<ByteType>, Error> {
        let file_length = file.seek(SeekFrom::End(0))?;

        let last_offset = if file_length > 0 {
            let frame = read_prev_frame::<ByteType, _>(file_length, |b, o| file.read_at(b, o))?;
            Some(frame.offset)
        } else {
            None
        };

        Ok(OffsetLog {
            file,
            end_of_file: file_length,
            last_offset,
            tmp_buffer: BytesMut::new(),
            byte_type: PhantomData,
        })
    }

    pub fn end(&self) -> u64 {
        self.end_of_file
    }

    pub fn read(&self, offset: u64) -> Result<ReadResult, Error> {
        read_next::<ByteType, _>(offset, &self.file)
    }

    pub fn append_batch(&mut self, buffs: &[&[u8]]) -> Result<Vec<u64>, Error> {
        unimplemented!()
    }

    pub fn iter(&self) -> OffsetLogIter<ByteType> {
        // TODO: what are the chances that try_clone() will fail?
        //  I'd rather not return a Result<> here.
        OffsetLogIter::new(self.file.try_clone().unwrap())
    }

    pub fn iter_at_offset(&self, offset: u64) -> OffsetLogIter<ByteType> {
        OffsetLogIter::with_starting_offset(self.file.try_clone().unwrap(),
                                            offset)
    }

}

pub struct OffsetLogIter<ByteType> {
    reader: BufOffsetReader<File>,
    current: u64,
    next: u64,
    byte_type: PhantomData<ByteType>,
}

impl<ByteType> OffsetLogIter<ByteType> {
    pub fn new(file: File) -> OffsetLogIter<ByteType> {
        OffsetLogIter::with_starting_offset(file, 0)
    }

    pub fn with_starting_offset(file: File, offset: u64) -> OffsetLogIter<ByteType> {
        OffsetLogIter {
            reader: BufOffsetReader::new(file),
            current: offset,
            next: offset,
            byte_type: PhantomData,
        }
    }
}

impl<ByteType> BidirIterator for OffsetLogIter<ByteType> {
    type Item = LogEntry;

    fn next(&mut self) -> Option<Self::Item> {
        self.current = self.next;
        let r = read_next_mut::<u32, _>(self.current, &mut self.reader).ok()?;
        self.next = r.next;
        Some(r.entry)
    }

    fn prev(&mut self) -> Option<Self::Item> {
        self.next = self.current;
        let r = read_prev_mut::<u32, _>(self.current, &mut self.reader).ok()?;
        self.current = r.entry.offset;
        Some(r.entry)
    }
}

pub fn encode<T>(offset: u64, item: &[u8], dest: &mut BytesMut) -> Result<u64, Error> {
    unimplemented!()
}

pub fn validate_entry<T>(offset: u64, data_size: usize, rest: &[u8]) -> Result<u64, Error> {
    unimplemented!()
}

pub fn read_next<ByteType, R: OffsetRead>(offset: u64, r: &R) -> Result<ReadResult, Error> {
    read_next_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o))
}

pub fn read_next_mut<ByteType, R: OffsetReadMut>(offset: u64, r: &mut R) -> Result<ReadResult, Error> {
    read_next_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o))
}

pub fn read_prev<ByteType, R: OffsetRead>(offset: u64, r: &R) -> Result<ReadResult, Error> {
    read_prev_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o))
}

pub fn read_prev_mut<ByteType, R: OffsetReadMut>(offset: u64, r: &mut R) -> Result<ReadResult, Error> {
    read_prev_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o))
}

fn read_next_impl<ByteType, F>(offset: u64, mut read_at: F) -> Result<ReadResult, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    let frame = read_next_frame::<ByteType, _>(offset, &mut read_at)?;
    read_entry::<ByteType, _>(&frame, &mut read_at)
}

fn read_prev_impl<ByteType, F>(offset: u64, mut read_at: F) -> Result<ReadResult, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    let frame = read_prev_frame::<ByteType, _>(offset, &mut read_at)?;
    read_entry::<ByteType, _>(&frame, &mut read_at)
}

fn read_next_frame<ByteType, F>(offset: u64, read_at: &mut F) -> Result<Frame, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    // Entry is [payload size: u32, payload, payload_size: u32, next_offset: ByteType]

    const HEAD_SIZE: usize = size_of::<u32>();

    let mut head_bytes = [0; HEAD_SIZE];
    let n = read_at(&mut head_bytes, offset)?;
    if n < HEAD_SIZE {
        return Err(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let data_size = (&head_bytes[..]).read_u32::<BigEndian>()? as usize;
    Ok(Frame {
        offset,
        data_size
    })
}

fn read_prev_frame<ByteType, F>(offset: u64, mut read_at: F) -> Result<Frame, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    let tail_size = size_of_frame_tail::<ByteType>(); // TODO: why can't this be const?

    // big enough, assuming ByteType isn't bigger than a u64
    let mut tmp = [0; size_of::<u32>() + size_of::<u64>()];
    if tmp.len() as u64 > offset {
        return Err(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let n = read_at(&mut tmp[..tail_size], offset - tail_size as u64)?;
    if n < tail_size {
        return Err(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let data_size = (&tmp[..]).read_u32::<BigEndian>()? as usize;
    if (data_size as u64) > offset {
        return Err(FlumeOffsetLogError::CorruptLogFile {}.into());
    }

    let data_start = offset - tail_size as u64 - data_size as u64;

    Ok(Frame {
        offset: data_start - size_of::<u32>() as u64,
        data_size
    })
}

fn read_entry<ByteType, F>(frame: &Frame, read_at: &mut F) -> Result<ReadResult, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    // Entry is [payload size: u32, payload, payload_size: u32, next_offset: ByteType]
    let tail_size = size_of_frame_tail::<ByteType>();
    let to_read = frame.data_size + tail_size;

    let mut buf = Vec::with_capacity(to_read);
    unsafe { buf.set_len(to_read) };

    let n = read_at(&mut buf, frame.data_start())?;
    if n < to_read {
        return Err(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let next = validate_entry::<ByteType>(frame.offset, frame.data_size, &buf)?;

    // Chop the tail off of buf, so it only contains the entry data.
    buf.truncate(frame.data_size);

    Ok(ReadResult {
        entry: LogEntry {
            offset: frame.offset,
            data: buf,
        },
        next: next,
    })
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;
    use flume_log::FlumeLog;
    use go_offset_log::*;


    #[test]
    fn simple_open() {

    }
}
