pub use bidir_iter::{BidirIterator, Forward};

use crate::flume_log::*;
use crate::iter_at_offset::IterAtOffset;
use crate::log_entry::LogEntry;
use buffered_offset_reader::{BufOffsetReader, OffsetRead, OffsetReadMut, OffsetWrite};
use byteorder::{BigEndian, ReadBytesExt};
use bytes::{BufMut, BytesMut};
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{Seek, SeekFrom};
use std::marker::PhantomData;
use std::mem::size_of;
use std::path::Path;

#[derive(Debug, Fail)]
pub enum FlumeOffsetLogError {
    #[fail(display = "Incorrect framing values detected, log file might be corrupt")]
    CorruptLogFile {},

    #[fail(display = "The decode buffer passed to decode was too small")]
    DecodeBufferSizeTooSmall {},
}

pub struct OffsetLog<ByteType> {
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
    pub data_size: usize,
}

impl Frame {
    fn data_start(&self) -> u64 {
        self.offset + size_of::<u32>() as u64
    }
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
        let file = OpenOptions::new().read(true).open(&path)?;

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

    pub fn append_batch<T: AsRef<[u8]>>(&mut self, buffs: &[T]) -> Result<Vec<u64>, Error> {
        let mut bytes = BytesMut::new();
        let mut offsets = Vec::<u64>::new();

        let new_end = buffs.iter().try_fold(self.end_of_file, |offset, buff| {
            //Maybe there's a more functional way of doing this. Kinda mixing functional and
            //imperative.
            offsets.push(offset);
            encode::<ByteType>(offset, &buff.as_ref(), &mut bytes)
        })?;

        offsets.last().map(|o| self.last_offset = Some(*o));

        self.file.write_at(&bytes, self.end_of_file)?;
        self.end_of_file = new_end;

        Ok(offsets)
    }

    pub fn iter(&self) -> Forward<OffsetLogIter<ByteType>> {
        OffsetLogIter::new(self.file.try_clone().unwrap()).forward_owned()
    }

    pub fn bidir_iter(&self) -> OffsetLogIter<ByteType> {
        // TODO: what are the chances that try_clone() will fail?
        //  I'd rather not return a Result<> here.
        OffsetLogIter::new(self.file.try_clone().unwrap())
    }

    pub fn bidir_iter_at_offset(&self, offset: u64) -> OffsetLogIter<ByteType> {
        OffsetLogIter::with_starting_offset(self.file.try_clone().unwrap(), offset)
    }
}

impl<ByteType> FlumeLog for OffsetLog<ByteType> {
    fn get(&self, seq_num: u64) -> Result<Vec<u8>, Error> {
        self.read(seq_num).map(|r| r.entry.data)
    }

    fn latest(&self) -> Option<u64> {
        self.last_offset
    }

    fn append(&mut self, buff: &[u8]) -> Result<u64, Error> {
        self.tmp_buffer.clear();
        self.tmp_buffer
            .reserve(buff.len() + size_of_framing_bytes::<ByteType>());

        let offset = self.end_of_file;
        let new_end = encode::<ByteType>(offset, buff, &mut self.tmp_buffer)?;
        self.file.write_at(&self.tmp_buffer, offset)?;

        self.end_of_file = new_end;
        self.last_offset = Some(offset);
        Ok(offset)
    }

    fn clear(&mut self, _seq_num: u64) {
        unimplemented!();
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

impl<ByteType> IterAtOffset<Forward<OffsetLogIter<ByteType>>> for OffsetLog<ByteType> {
    fn iter_at_offset(&self, offset: u64) -> Forward<OffsetLogIter<ByteType>> {
        OffsetLogIter::with_starting_offset(self.file.try_clone().unwrap(), offset).forward_owned()
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

fn size_of_frame_tail<T>() -> usize {
    size_of::<u32>() + size_of::<T>()
}
fn size_of_framing_bytes<T>() -> usize {
    size_of::<u32>() * 2 + size_of::<T>()
}

pub fn encode<T>(offset: u64, item: &[u8], dest: &mut BytesMut) -> Result<u64, Error> {
    let chunk_size = size_of_framing_bytes::<T>() + item.len();
    dest.reserve(chunk_size);
    dest.put_u32(item.len() as u32);
    dest.put_slice(&item);
    dest.put_u32(item.len() as u32);
    let next_offset = offset + chunk_size as u64;

    dest.put_uint(next_offset, size_of::<T>());
    Ok(next_offset)
}

pub fn validate_entry<T>(offset: u64, data_size: usize, rest: &[u8]) -> Result<u64, Error> {
    if rest.len() != data_size + size_of_frame_tail::<T>() {
        return Err(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let sz = (&rest[data_size..]).read_u32::<BigEndian>()? as usize;
    if sz != data_size {
        return Err(FlumeOffsetLogError::CorruptLogFile {}.into());
    }

    let next =
        (&rest[(data_size + size_of::<u32>())..]).read_uint::<BigEndian>(size_of::<T>())? as u64;

    // `next` should be equal to the offset of the next entry
    // which may or may not be immediately following this one (I suppose)
    if next < offset + size_of::<u32>() as u64 + rest.len() as u64 {
        return Err(FlumeOffsetLogError::CorruptLogFile {}.into());
    }
    Ok(next)
}

pub fn read_next<ByteType, R: OffsetRead>(offset: u64, r: &R) -> Result<ReadResult, Error> {
    read_next_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o))
}

pub fn read_next_mut<ByteType, R: OffsetReadMut>(
    offset: u64,
    r: &mut R,
) -> Result<ReadResult, Error> {
    read_next_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o))
}

pub fn read_prev<ByteType, R: OffsetRead>(offset: u64, r: &R) -> Result<ReadResult, Error> {
    read_prev_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o))
}

pub fn read_prev_mut<ByteType, R: OffsetReadMut>(
    offset: u64,
    r: &mut R,
) -> Result<ReadResult, Error> {
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
    Ok(Frame { offset, data_size })
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
        data_size,
    })
}

fn read_entry<ByteType, F>(frame: &Frame, read_at: &mut F) -> Result<ReadResult, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    // Entry is [payload size: u32, payload, payload_size: u32, next_offset: ByteType]
    let tail_size = size_of_frame_tail::<ByteType>();
    let to_read = frame.data_size + tail_size;

    let mut buf = vec![0; to_read];

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
        next,
    })
}

// extern crate tempfile;
#[cfg(test)]
mod test {
    use crate::flume_log::FlumeLog;
    use crate::offset_log::*;
    use bytes::BytesMut;

    use serde_json::{from_slice, Value};

    extern crate tempfile;
    use self::tempfile::tempfile;

    fn temp_offset_log() -> OffsetLog<u32> {
        OffsetLog::<u32>::from_file(tempfile().unwrap()).unwrap()
    }

    #[test]
    fn simple_encode() {
        let to_encode = vec![1, 2, 3, 4];
        let mut buf = BytesMut::with_capacity(16);
        encode::<u32>(0, &to_encode, &mut buf).unwrap();

        assert_eq!(&buf[..], &[0, 0, 0, 4, 1, 2, 3, 4, 0, 0, 0, 4, 0, 0, 0, 16])
    }

    #[test]
    fn simple_encode_u64() {
        let to_encode = vec![1, 2, 3, 4];
        let mut buf = BytesMut::with_capacity(20);
        encode::<u64>(0, &to_encode, &mut buf).unwrap();

        assert_eq!(
            &buf[..],
            &[0, 0, 0, 4, 1, 2, 3, 4, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 20]
        )
    }

    #[test]
    fn encode_multi() {
        let mut buf = BytesMut::with_capacity(32);

        encode::<u32>(0, &[1, 2, 3, 4], &mut buf)
            .and_then(|offset| encode::<u32>(offset, &[5, 6, 7, 8], &mut buf))
            .unwrap();

        assert_eq!(
            &buf[..],
            &[
                0, 0, 0, 4, 1, 2, 3, 4, 0, 0, 0, 4, 0, 0, 0, 16, 0, 0, 0, 4, 5, 6, 7, 8, 0, 0, 0,
                4, 0, 0, 0, 32
            ]
        )
    }

    #[test]
    fn encode_multi_u64() {
        let mut buf = BytesMut::with_capacity(40);

        encode::<u64>(0, &[1, 2, 3, 4], &mut buf)
            .and_then(|offset| encode::<u64>(offset, &[5, 6, 7, 8], &mut buf))
            .unwrap();

        assert_eq!(
            &buf[0..20],
            &[0, 0, 0, 4, 1, 2, 3, 4, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 20]
        );
        assert_eq!(
            &buf[20..],
            &[0, 0, 0, 4, 5, 6, 7, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 40]
        )
    }

    #[test]
    fn simple() {
        let bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 20];

        let r = read_next::<u32, _>(0, &bytes).unwrap();
        assert_eq!(r.entry.offset, 0);
        assert_eq!(&r.entry.data, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(r.next, bytes.len() as u64);
    }

    #[test]
    fn simple_u64() {
        let bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 24,
        ];

        let r = read_next::<u64, _>(0, &bytes).unwrap();
        assert_eq!(r.entry.offset, 0);
        assert_eq!(&r.entry.data, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(r.next, bytes.len() as u64);
    }

    #[test]
    fn multiple() {
        let bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 20, 0, 0, 0, 8, 9, 10, 11, 12,
            13, 14, 15, 16, 0, 0, 0, 8, 0, 0, 0, 40,
        ];

        let r1 = read_next::<u32, _>(0, &bytes).unwrap();
        assert_eq!(r1.entry.offset, 0);
        assert_eq!(&r1.entry.data, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(r1.next, 20);

        let r2 = read_next::<u32, _>(r1.next, &bytes).unwrap();
        assert_eq!(r2.entry.offset, r1.next);
        assert_eq!(&r2.entry.data, &[9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(r2.next, 40);

        let r3 = read_prev::<u32, _>(bytes.len() as u64, &bytes).unwrap();
        assert_eq!(r3.entry.offset, r1.next);
        assert_eq!(&r3.entry.data, &[9, 10, 11, 12, 13, 14, 15, 16]);

        let r4 = read_prev::<u32, _>(r3.entry.offset, &bytes).unwrap();
        assert_eq!(r4.entry.offset, 0);
        assert_eq!(&r4.entry.data, &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn multiple_u64() {
        let bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 24, 0, 0, 0, 8, 9,
            10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 48,
        ];

        let r1 = read_next::<u64, _>(0, &bytes).unwrap();
        assert_eq!(r1.entry.offset, 0);
        assert_eq!(&r1.entry.data, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(r1.next, 24);

        let r2 = read_next::<u64, _>(r1.next, &bytes).unwrap();
        assert_eq!(r2.entry.offset, r1.next);
        assert_eq!(&r2.entry.data, &[9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(r2.next, 48);

        let r3 = read_prev::<u64, _>(bytes.len() as u64, &bytes).unwrap();
        assert_eq!(r3.entry.offset, r1.next);
        assert_eq!(&r3.entry.data, &[9, 10, 11, 12, 13, 14, 15, 16]);

        let r4 = read_prev::<u64, _>(r3.entry.offset, &bytes).unwrap();
        assert_eq!(r4.entry.offset, 0);
        assert_eq!(&r4.entry.data, &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn read_incomplete_entry() {
        let bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 9, 0, 0, 0];
        let r = read_next::<u32, _>(0, &bytes);

        assert!(r.is_err());
    }

    #[test]
    fn read_very_incomplete_entry() {
        let bytes: &[u8] = &[0, 0, 0];
        let r = read_next::<u32, _>(0, &bytes);
        assert!(r.is_err());
    }

    #[test]
    fn errors_with_bad_second_size_valuen() {
        let bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 9, 0, 0, 0, 20];
        let r = read_next::<u32, _>(0, &bytes);

        assert!(r.is_err());
    }

    #[test]
    fn errors_with_bad_next_offset_value() {
        let bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 16];
        let r = read_next::<u32, _>(0, &bytes);
        assert!(r.is_err());
    }

    #[test]
    fn read_from_a_file() {
        let log = OffsetLog::<u32>::new("./db/test.offset").unwrap();
        assert_eq!(log.latest(), Some(207));

        let result = log
            .get(0)
            .and_then(|val| from_slice(&val).map_err(|err| err.into()))
            .map(|val: Value| match val["value"] {
                Value::Number(ref num) => num.as_u64().unwrap(),
                _ => panic!(),
            })
            .unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn open_read_only() {
        let mut log = OffsetLog::<u32>::open_read_only("./db/test.offset").unwrap();
        assert_eq!(log.latest(), Some(207));

        let result = log
            .get(0)
            .and_then(|val| from_slice(&val).map_err(|err| err.into()))
            .map(|val: Value| match val["value"] {
                Value::Number(ref num) => num.as_u64().unwrap(),
                _ => panic!(),
            })
            .unwrap();
        assert_eq!(result, 0);

        assert!(log.append(&[1, 2, 3, 4]).is_err());
    }

    #[test]
    fn write_to_a_file() -> Result<(), Error> {
        let test_vec = b"{\"value\": 1}";

        let mut log = temp_offset_log();
        assert_eq!(log.latest(), None);
        let offset = log.append(test_vec)?;
        assert_eq!(offset, 0);
        assert_eq!(log.latest(), Some(0));

        let offset = log.append(test_vec)?;
        assert_eq!(log.latest(), Some(offset));

        let v: Value = from_slice(&log.get(0)?)?;
        let result = match v["value"] {
            Value::Number(ref num) => num.as_u64().unwrap(),
            _ => panic!(),
        };
        assert_eq!(result, 1);
        Ok(())
    }

    #[test]
    fn batch_write_to_a_file() -> Result<(), Error> {
        let test_vec: &[u8] = b"{\"value\": 1}";

        let mut test_vecs = Vec::new();

        for _ in 0..100 {
            test_vecs.push(test_vec);
        }

        let mut offset_log = temp_offset_log();
        let result = offset_log
            .append_batch(test_vecs.as_slice())
            .and_then(|sequences| {
                assert_eq!(sequences.len(), test_vecs.len());
                assert_eq!(sequences[0], 0);
                assert_eq!(
                    sequences[1],
                    test_vec.len() as u64 + size_of_framing_bytes::<u32>() as u64
                );
                offset_log.get(0)
            })
            .and_then(|val| from_slice(&val).map_err(|err| err.into()))
            .map(|val: Value| match val["value"] {
                Value::Number(ref num) => {
                    let result = num.as_u64().unwrap();
                    result
                }
                _ => panic!(),
            })
            .unwrap();
        assert_eq!(result, 1);
        Ok(())
    }

    #[test]
    fn arbitrary_read_and_write_to_a_file() -> Result<(), Error> {
        let mut offset_log = temp_offset_log();

        let data_to_write = vec![b"{\"value\": 1}", b"{\"value\": 2}", b"{\"value\": 3}"];

        let seqs: Vec<u64> = data_to_write
            .iter()
            .map(|data| offset_log.append(*data).unwrap())
            .collect();

        let sum: u64 = seqs
            .iter()
            .rev()
            .map(|seq| offset_log.get(*seq).unwrap())
            .map(|val| from_slice(&val).unwrap())
            .map(|val: Value| match val["value"] {
                Value::Number(ref num) => {
                    let result = num.as_u64().unwrap();
                    result
                }
                _ => panic!(),
            })
            .sum();

        assert_eq!(sum, 6);
        Ok(())
    }

    #[test]
    fn offset_log_as_iter() {
        let log = OffsetLog::<u32>::new("./db/test.offset").unwrap();

        let sum: u64 = log
            .iter()
            .take(5)
            .map(|val| val.data)
            .map(|val| from_slice(&val).unwrap())
            .map(|val: Value| match val["value"] {
                Value::Number(ref num) => {
                    let result = num.as_u64().unwrap();
                    result
                }
                _ => panic!(),
            })
            .sum();

        assert_eq!(sum, 10);
    }

    #[test]
    fn bidir_iter() -> Result<(), Error> {
        let mut log = temp_offset_log();
        log.append(b"abc")?;
        log.append(b"def")?;
        log.append(b"123")?;
        log.append(b"456")?;

        let mut iter = log.bidir_iter();
        assert_eq!(iter.next().unwrap().data, b"abc");
        assert_eq!(iter.next().unwrap().data, b"def");
        assert_eq!(iter.next().unwrap().data, b"123");
        assert_eq!(iter.next().unwrap().data, b"456");
        assert!(iter.next().is_none());
        assert_eq!(iter.prev().unwrap().data, b"456");
        assert_eq!(iter.prev().unwrap().data, b"123");
        assert_eq!(iter.prev().unwrap().data, b"def");
        assert_eq!(iter.prev().unwrap().data, b"abc");
        assert!(iter.prev().is_none());
        assert_eq!(iter.next().unwrap().data, b"abc");

        let iter = log.bidir_iter();
        let mut iter = iter.filter(|e| e.offset % 10 == 0);

        assert_eq!(iter.next().unwrap().data, b"abc");
        assert_eq!(iter.next().unwrap().data, b"123");
        assert!(iter.next().is_none());
        assert_eq!(iter.prev().unwrap().data, b"123");

        let iter = log.bidir_iter();
        let mut iter = iter.map(|e| e.offset);

        // Same iter forward and back
        let forward_offsets: Vec<u64> = iter.forward().collect();
        assert_eq!(forward_offsets, &[0, 15, 30, 45]);

        let backward_offsets: Vec<u64> = iter.backward().collect();
        assert_eq!(backward_offsets, &[45, 30, 15, 0]);

        // Same iter, take two
        let forward_offsets: Vec<u64> = iter.forward().take(2).collect();
        assert_eq!(forward_offsets, &[0, 15]);

        // Same iter, two more
        let forward_offsets: Vec<u64> = iter.forward().take(2).collect();
        assert_eq!(forward_offsets, &[30, 45]);

        // New backward iter, starting at eof
        let backward_offsets: Vec<u64> = log
            .bidir_iter_at_offset(log.end())
            .backward()
            .map(|e| e.offset)
            .collect();
        assert_eq!(backward_offsets, &[45, 30, 15, 0]);

        Ok(())
    }
}
