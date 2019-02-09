use buffered_offset_reader::{BufOffsetReader, OffsetRead, OffsetReadMut, OffsetWrite};
use byteorder::{BigEndian, ReadBytesExt};
use bytes::{BufMut, BytesMut};
use flume_log::*;
use std::fs::{File, OpenOptions};
use std::io;
use std::marker::PhantomData;
use std::mem::size_of;

#[derive(Debug, Fail)]
pub enum FlumeOffsetLogError {
    #[fail(display = "Incorrect framing values detected, log file might be corrupt")]
    CorruptLogFile {},
    #[fail(display = "The decode buffer passed to decode was too small")]
    DecodeBufferSizeTooSmall {},
}

pub struct OffsetLog<ByteType> {
    file: File,
    end_of_file: u64,
    tmp_buffer: BytesMut,
    byte_type: PhantomData<ByteType>,
}


#[derive(Debug)] // TODO: derive more traits
pub struct LogEntry {
    pub id: u64,
    pub data_buffer: Vec<u8>,
}

#[derive(Debug)]
pub struct ReadResult {
    pub entry: LogEntry,
    pub next: u64,
}

pub struct OffsetLogIter<'a, ByteType> {
    reader: BufOffsetReader<'a, File>,
    position: u64,
    byte_type: PhantomData<ByteType>,
}

impl<'a, ByteType> OffsetLogIter<'a, ByteType> {
    pub fn new(file: &'a File) -> OffsetLogIter<'a, ByteType> {
        OffsetLogIter::with_starting_offset(&file, 0)
    }

    pub fn with_starting_offset(file: &File, position: Sequence) -> OffsetLogIter<ByteType> {
        OffsetLogIter {
            reader: BufOffsetReader::new(&file),
            position,
            byte_type: PhantomData,
        }
    }
}

impl<'a, ByteType> Iterator for OffsetLogIter<'a, ByteType> {
    type Item = LogEntry;

    fn next(&mut self) -> Option<Self::Item> {
        match read_entry_mut::<ByteType, _>(self.position, &mut self.reader) {
            Ok(r) => {
                self.position = r.next;
                Some(r.entry)
            }
            Err(_) => None,
        }
    }
}

impl<ByteType> OffsetLog<ByteType> {
    pub fn new(path: String) -> OffsetLog<ByteType> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path.clone())
            .expect("Unable to open file");

        let file_length = std::fs::metadata(path)
            .expect("Unable to get metadata of file")
            .len();

        OffsetLog {
            file,
            end_of_file: file_length,
            tmp_buffer: BytesMut::new(),
            byte_type: PhantomData,
        }
    }

    pub fn read(&self, offset: u64) -> Result<ReadResult, Error> {
        read_entry::<ByteType, _>(offset, &self.file)
    }

    pub fn append_batch(&mut self, buffs: &[&[u8]]) -> Result<Vec<u64>, Error> {
        let mut bytes = BytesMut::new();
        let mut offsets = Vec::<u64>::new();

        let new_end = buffs.iter().try_fold(self.end_of_file, |offset, buff| {
            //Maybe there's a more functional way of doing this. Kinda mixing functional and
            //imperative.
            offsets.push(offset);
            encode::<ByteType>(offset, &buff, &mut bytes)
        })?;

        self.file.write_at(&bytes, self.end_of_file as usize)?;
        self.end_of_file = new_end;

        Ok(offsets)
    }

    pub fn iter(&self) -> OffsetLogIter<ByteType> {
        OffsetLogIter::new(&self.file)
    }
}

impl<ByteType> FlumeLog for OffsetLog<ByteType> {
    fn get(&self, seq_num: u64) -> Result<Vec<u8>, Error> {
        self.read(seq_num).map(|r| r.entry.data_buffer)
    }

    fn latest(&self) -> u64 {
        unimplemented!()
    }

    fn append(&mut self, buff: &[u8]) -> Result<u64, Error> {
        self.tmp_buffer.clear();
        self.tmp_buffer
            .reserve(buff.len() + size_of_framing_bytes::<ByteType>());

        let offset = self.end_of_file;
        let new_end = encode::<ByteType>(offset, buff, &mut self.tmp_buffer)?;
        self.file.write_at(&self.tmp_buffer, offset as usize)?;

        self.end_of_file = new_end;
        Ok(offset)
    }

    fn clear(&mut self, _seq_num: u64) {
        unimplemented!();
    }
}

#[derive(Debug)]
pub struct Data {
    pub data_buffer: Vec<u8>,
    pub id: u64,
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
    dest.put_u32_be(item.len() as u32);
    dest.put_slice(&item);
    dest.put_u32_be(item.len() as u32);
    let new_offset = offset + chunk_size as u64;
    // self.length += chunk_size as u64;

    dest.put_uint_be(new_offset, size_of::<T>());
    Ok(new_offset)
}

pub fn validate_entry<T>(offset: usize, data_size: usize, rest: &[u8]) -> Result<u64, Error> {
    if rest.len() != data_size + size_of_frame_tail::<T>() {
        return Err(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let sz = (&rest[data_size..]).read_u32::<BigEndian>()?;
    if sz as usize != data_size {
        return Err(FlumeOffsetLogError::CorruptLogFile {}.into());
    }

    let next = (&rest[(data_size + size_of::<u32>())..]).read_uint::<BigEndian>(size_of::<T>())?;

    // `next` should be equal to the offset of the next entry
    // which may or may not be immediately following this one (I suppose)
    if next < (offset + size_of::<u32>() + rest.len()) as u64 {
        return Err(FlumeOffsetLogError::CorruptLogFile {}.into());
    }
    Ok(next as u64)
}

pub fn read_entry<ByteType, R: OffsetRead>(offset: u64, r: &R) -> Result<ReadResult, Error> {
    read_entry_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o as usize))
}

pub fn read_entry_mut<ByteType, R: OffsetReadMut>(offset: u64, r: &mut R) -> Result<ReadResult, Error> {
    read_entry_impl::<ByteType, _>(offset, |b, o| r.read_at(b, o as usize))
}


fn read_entry_impl<ByteType, F>(offset: u64, mut read_at: F) -> Result<ReadResult, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    // Entry is [payload size: u32, payload, payload_size: u32, next_offset: ByteType]

    const HEAD_SIZE: usize = size_of::<u32>();
    let tail_size = size_of_frame_tail::<ByteType>();

    let mut head_bytes = [0; HEAD_SIZE];
    let n = read_at(&mut head_bytes, offset)?;
    if n < HEAD_SIZE {
        return Err(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let data_size = (&head_bytes[..]).read_u32::<BigEndian>()? as usize;
    let to_read = data_size + tail_size;

    let mut buf = Vec::with_capacity(to_read);
    unsafe { buf.set_len(to_read) };

    let n = read_at(&mut buf, offset + HEAD_SIZE as u64)?;
    if n < to_read {
        return Err(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let next = validate_entry::<ByteType>(offset as usize, data_size, &buf)?;

    // Chop the tail off of buf, so it only contains the entry data.
    buf.truncate(data_size);

    Ok(ReadResult {
        entry: LogEntry {
            id: offset,
            data_buffer: buf,
        },
        next: next,
    })
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;
    use flume_log::FlumeLog;
    use offset_log::*;

    use serde_json::*;

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

        let r = read_entry::<u32, _>(0, &bytes).unwrap();
        assert_eq!(r.entry.id, 0);
        assert_eq!(&r.entry.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(r.next, bytes.len() as u64);
    }

    #[test]
    fn simple_u64() {
        let bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 24,
        ];

        let r = read_entry::<u64, _>(0, &bytes).unwrap();
        assert_eq!(r.entry.id, 0);
        assert_eq!(&r.entry.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(r.next, bytes.len() as u64);
    }

    #[test]
    fn multiple() {
        let bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 20, 0, 0, 0, 8, 9, 10, 11, 12,
            13, 14, 15, 16, 0, 0, 0, 8, 0, 0, 0, 40,
        ];

        let r1 = read_entry::<u32, _>(0, &bytes).unwrap();
        assert_eq!(r1.entry.id, 0);
        assert_eq!(&r1.entry.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(r1.next, 20);

        let r2 = read_entry::<u32, _>(r1.next, &bytes).unwrap();
        assert_eq!(r2.entry.id, r1.next);
        assert_eq!(&r2.entry.data_buffer, &[9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(r2.next, 40);
    }

    #[test]
    fn multiple_u64() {
        let bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 24, 0, 0, 0, 8, 9,
            10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 48,
        ];

        let r1 = read_entry::<u64, _>(0, &bytes).unwrap();
        assert_eq!(r1.entry.id, 0);
        assert_eq!(&r1.entry.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(r1.next, 24);

        let r2 = read_entry::<u64, _>(r1.next, &bytes).unwrap();
        assert_eq!(r2.entry.id, r1.next);
        assert_eq!(&r2.entry.data_buffer, &[9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(r2.next, 48);
    }

    #[test]
    fn read_incomplete_entry() {
        let bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 9, 0, 0, 0];
        let r = read_entry::<u32, _>(0, &bytes);

        assert!(r.is_err());
    }

    #[test]
    fn read_very_incomplete_entry() {
        let bytes: &[u8] = &[0, 0, 0];
        let r = read_entry::<u32, _>(0, &bytes);
        assert!(r.is_err());
    }

    #[test]
    fn errors_with_bad_second_size_valuen() {
        let bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 9, 0, 0, 0, 20];
        let r = read_entry::<u32, _>(0, &bytes);

        assert!(r.is_err());
    }

    #[test]
    fn errors_with_bad_next_offset_value() {
        let bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 16];
        let r = read_entry::<u32, _>(0, &bytes);
        assert!(r.is_err());
    }

    #[test]
    fn read_from_a_file() {
        let offset_log = OffsetLog::<u32>::new("./db/test.offset".to_string());
        let result = offset_log
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
    fn write_to_a_file() {
        let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
        std::fs::remove_file(filename.clone())
            .or::<Result<()>>(Ok(()))
            .unwrap();

        let test_vec = b"{\"value\": 1}";

        let mut offset_log = OffsetLog::<u32>::new(filename);
        let result = offset_log
            .append(test_vec)
            .and_then(|_| offset_log.get(0))
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
    }
    #[test]
    fn batch_write_to_a_file() {
        let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
        std::fs::remove_file(filename.clone())
            .or::<Result<()>>(Ok(()))
            .unwrap();

        let test_vec: &[u8] = b"{\"value\": 1}";

        let mut test_vecs = Vec::new();

        for _ in 0..100 {
            test_vecs.push(test_vec);
        }

        let mut offset_log = OffsetLog::<u32>::new(filename);
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
    }
    #[test]
    fn arbitrary_read_and_write_to_a_file() {
        let filename = "/tmp/test124.offset".to_string(); //careful not to reuse this filename, threads might make things weird
        std::fs::remove_file(filename.clone())
            .or::<Result<()>>(Ok(()))
            .unwrap();

        let mut offset_log = OffsetLog::<u32>::new(filename);

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
    }

    #[test]
    fn offset_log_as_iter() {
        let filename = "./db/test.offset".to_string();
        let log = OffsetLog::<u32>::new(filename);

        let log_iter = log.iter();

        let sum: u64 = log_iter
            .take(5)
            .map(|val| val.data_buffer)
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

}
