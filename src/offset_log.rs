use byteorder::{BigEndian, ReadBytesExt};
use bytes::{BufMut, BytesMut};
use flume_log::*;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::mem::size_of;
use tokio_codec::{Decoder, Encoder};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct OffsetCodec<ByteType> {
    last_valid_offset: u64,
    length: u64,
    byte_type: PhantomData<ByteType>,
}

impl<ByteType> OffsetCodec<ByteType> {
    pub fn new() -> OffsetCodec<ByteType> {
        OffsetCodec {
            last_valid_offset: 0,
            length: 0,
            byte_type: PhantomData,
        }
    }
    pub fn with_length(length: u64) -> OffsetCodec<ByteType> {
        OffsetCodec {
            last_valid_offset: 0,
            length,
            byte_type: PhantomData,
        }
    }
}

pub struct OffsetLog<ByteType> {
    file: File,
    offset_codec: OffsetCodec<ByteType>,
}

pub struct OffsetLogIter<ByteType, R> {
    reader: BufReader<R>,
    offset_codec: OffsetCodec<ByteType>,
}

impl<ByteType, R: Read> OffsetLogIter<ByteType, R> {
    pub fn new(file: R) -> OffsetLogIter<ByteType, R> {
        let reader = BufReader::new(file);
        let offset_codec = OffsetCodec::new();

        OffsetLogIter {
            reader,
            offset_codec,
        }
    }
}

impl<ByteType, R: Read> Iterator for OffsetLogIter<ByteType, R> {
    type Item = Vec<u8>;

    //Yikes this is hacky!
    //  - Using scopes like this to keep the compiler happy looks yuck. How could that be done
    //  nicer?
    //  - the buffer housekeeping could get moved into the OffsetLogIter.

    fn next(&mut self) -> Option<Self::Item> {
        let mut bytes = BytesMut::new();
        let mut buff_len: usize;
        let mut total_consumed: usize = 0;

        {
            let buff = self
                .reader
                .fill_buf()
                .expect("Buffered read failed trying to read from file");
            bytes.extend_from_slice(buff);
            buff_len = buff.len();
        }

        if bytes.len() == 0 {
            return None;
        }

        // We need 4 bytes to be able to check the length of the data expected.
        if bytes.len() < 4 {
            {
                self.reader.consume(bytes.len());
                total_consumed = bytes.len()
            }

            let buff = self
                .reader
                .fill_buf()
                .expect("Buffered read failed trying to read from file");
            bytes.extend_from_slice(buff);
            buff_len = buff.len();
        }

        let data_size = (&bytes[0..4]).read_u32::<BigEndian>().unwrap() as usize;
        let framed_size = data_size + size_of_framing_bytes::<ByteType>();

        while bytes.len() < framed_size {
            self.reader.consume(buff_len);
            total_consumed += buff_len;
            let buff = self
                .reader
                .fill_buf()
                .expect("Buffered read failed trying to read from file");
            buff_len = buff.len();

            bytes.extend_from_slice(buff);
        }

        self.offset_codec
            .decode(&mut bytes)
            .map(|res| {
                res.map(|data| {
                    self.reader.consume(
                        data.data_buffer.len() + size_of_framing_bytes::<ByteType>()
                            - total_consumed,
                    );
                    data.data_buffer
                })
            })
            .unwrap_or(None)
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

        let offset_codec = OffsetCodec::<ByteType>::with_length(file_length);

        OffsetLog { offset_codec, file }
    }
}

impl<ByteType> FlumeLog for OffsetLog<ByteType> {
    fn get(&mut self, seq_num: u64) -> Result<Vec<u8>, Error> {
        let mut frame_bytes = vec![0; 4];

        self.file.seek(SeekFrom::Start(seq_num as u64))?;
        self.file.read(&mut frame_bytes)?;

        let data_size = size_of_framing_bytes::<ByteType>()
            + (&frame_bytes[0..4]).read_u32::<BigEndian>().unwrap() as usize;

        let mut buf = Vec::with_capacity(data_size);
        unsafe { buf.set_len(data_size) };

        self.file.seek(SeekFrom::Start(seq_num as u64))?;
        self.file.read(&mut buf)?;
        self.offset_codec
            .decode(&mut buf.into())?
            .map(|val| val.data_buffer)
            .ok_or(FlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into())
    }

    fn latest(&self) -> u64 {
        self.offset_codec.last_valid_offset
    }

    fn append(&mut self, buff: &[u8]) -> Result<u64, Error> {
        self.file
            .seek(SeekFrom::End(0)) // Could store a bool for is_at_end to avoid the sys call. If it is actually a sys call.
            .and_then(|_| {
                let mut vec = Vec::new();
                vec.extend_from_slice(buff);
                let mut encoded =
                    BytesMut::with_capacity(size_of_framing_bytes::<ByteType>() + buff.len());
                self.offset_codec.encode(vec, &mut encoded).map(|_| encoded)
            })
            .and_then(|data| self.file.write(&data))
            .map(|len| self.offset_codec.length - len as u64)
            .map_err(|err| err.into())
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

fn size_of_framing_bytes<T>() -> usize {
    size_of::<u32>() * 2 + size_of::<T>()
}

fn is_valid_frame<T>(buf: &BytesMut, data_size: usize) -> bool {
    let second_data_size_index = data_size + size_of::<u32>();

    let second_data_size = (&buf[second_data_size_index..])
        .read_u32::<BigEndian>()
        .unwrap() as usize;

    second_data_size == data_size
}

impl<ByteType> Encoder for OffsetCodec<ByteType> {
    type Item = Vec<u8>; //TODO make this a slice
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dest: &mut BytesMut) -> Result<(), Self::Error> {
        let chunk_size = size_of_framing_bytes::<ByteType>() + item.len();
        dest.reserve(chunk_size);
        dest.put_u32_be(item.len() as u32);
        dest.put_slice(&item);
        dest.put_u32_be(item.len() as u32);
        self.length += chunk_size as u64;

        dest.put_uint_be(self.length.into(), size_of::<ByteType>());

        Ok(())
    }
}

#[derive(Debug, Fail)]
pub enum FlumeOffsetLogError {
    #[fail(display = "Incorrect framing values detected, log file might be corrupt")]
    CorruptLogFile {},
    #[fail(display = "The decode buffer passes to decode was too small")]
    DecodeBufferSizeTooSmall {},
}

impl<ByteType> Decoder for OffsetCodec<ByteType> {
    type Item = Data;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if buf.len() < size_of::<u32>() {
            return Ok(None);
        }
        let data_size = (&buf[..]).read_u32::<BigEndian>().unwrap() as usize;

        if buf.len() < data_size + size_of_framing_bytes::<ByteType>() {
            return Ok(None);
        }

        if !is_valid_frame::<ByteType>(buf, data_size) {
            return Err(FlumeOffsetLogError::CorruptLogFile {}.into());
        }

        buf.advance(size_of::<u32>()); //drop off one BytesType
        let data_buffer = buf.split_to(data_size);
        buf.advance(size_of::<u32>() + size_of::<ByteType>()); //drop off 2 ByteTypes.
        let data = Data {
            data_buffer: data_buffer.to_vec(),
            id: self.last_valid_offset,
        };

        let next_offset =
            self.last_valid_offset + size_of_framing_bytes::<ByteType>() as u64 + data_size as u64;
        self.last_valid_offset = next_offset;

        Ok(Some(data))
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decode(buf)
    }
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;
    use flume_log::FlumeLog;
    use offset_log::{Decoder, Encoder};
    use offset_log::{OffsetCodec, OffsetLog, OffsetLogIter};
    use serde_json::*;

    #[test]
    fn simple_encode() {
        let mut codec = OffsetCodec::<u32>::new();
        let to_encode = vec![1, 2, 3, 4];
        let mut buf = BytesMut::with_capacity(16);
        codec.encode(to_encode, &mut buf).unwrap();

        assert_eq!(&buf[..], &[0, 0, 0, 4, 1, 2, 3, 4, 0, 0, 0, 4, 0, 0, 0, 16])
    }

    #[test]
    fn simple_encode_u64() {
        let mut codec = OffsetCodec::<u64>::new();
        let to_encode = vec![1, 2, 3, 4];
        let mut buf = BytesMut::with_capacity(20);
        codec.encode(to_encode, &mut buf).unwrap();

        assert_eq!(
            &buf[..],
            &[0, 0, 0, 4, 1, 2, 3, 4, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 20]
        )
    }

    #[test]
    fn encode_multi() {
        let mut codec = OffsetCodec::<u32>::new();
        let to_encode = vec![1, 2, 3, 4];
        let mut buf = BytesMut::with_capacity(32);
        codec.encode(to_encode, &mut buf).unwrap();
        let to_encode = vec![5, 6, 7, 8];
        codec.encode(to_encode, &mut buf).unwrap();

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
        let mut codec = OffsetCodec::<u64>::new();
        let to_encode = vec![1, 2, 3, 4];
        let mut buf = BytesMut::with_capacity(40);
        codec.encode(to_encode, &mut buf).unwrap();
        let to_encode = vec![5, 6, 7, 8];
        codec.encode(to_encode, &mut buf).unwrap();

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
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 20];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
            }
            _ => assert!(false),
        }
    }
    #[test]
    fn simple_u64() {
        let mut codec = OffsetCodec::<u64>::new();
        let frame_bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 24,
        ];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
            }
            _ => assert!(false),
        }
    }
    #[test]
    fn mulitple() {
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 20, 0, 0, 0, 8, 9, 10, 11, 12,
            13, 14, 15, 16, 0, 0, 0, 8, 0, 0, 0, 40,
        ];
        let mut bytes = BytesMut::from(frame_bytes);
        let result1 = codec.decode(&mut bytes);

        match result1 {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
            }
            _ => assert!(false),
        }
        let result2 = codec.decode(&mut bytes);

        match result2 {
            Ok(Some(data)) => {
                assert_eq!(data.id, 20);
                assert_eq!(&data.data_buffer, &[9, 10, 11, 12, 13, 14, 15, 16]);
            }
            _ => assert!(false),
        }
    }
    #[test]
    fn mulitple_u64() {
        let mut codec = OffsetCodec::<u64>::new();
        let frame_bytes: &[u8] = &[
            0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 24, 0, 0, 0, 8, 9,
            10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 48,
        ];
        let mut bytes = BytesMut::from(frame_bytes);
        let result1 = codec.decode(&mut bytes);

        match result1 {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1, 2, 3, 4, 5, 6, 7, 8]);
            }
            _ => assert!(false),
        }
        let result2 = codec.decode(&mut bytes);

        match result2 {
            Ok(Some(data)) => {
                assert_eq!(data.id, 24);
                assert_eq!(&data.data_buffer, &[9, 10, 11, 12, 13, 14, 15, 16]);
            }
            _ => assert!(false),
        }
    }
    #[test]
    fn returns_ok_none_when_buffer_is_incomplete_frame() {
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 9, 0, 0, 0];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(None) => assert!(true),
            _ => assert!(false),
        }
    }
    #[test]
    fn returns_ok_none_when_buffer_less_than_4_bytes() {
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0, 0, 0];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(None) => assert!(true),
            _ => assert!(false),
        }
    }
    #[test]
    fn errors_with_bad_second_size_value() {
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0, 0, 0, 8, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 9, 0, 0, 0, 20];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(_)) => assert!(false),
            _ => assert!(true),
        }
    }
    #[test]
    fn read_from_a_file() {
        let mut offset_log = OffsetLog::<u32>::new("./db/test.offset".to_string());
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
        let file = std::fs::File::open(filename).unwrap();

        let log_iter = OffsetLogIter::<u32, std::fs::File>::new(file);

        let sum: u64 = log_iter
            .take(5)
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
