use tokio_io::codec::{Encoder, Decoder};
use bytes::{BytesMut, BufMut};
use std::{io };
use std::fs::{File, OpenOptions};
use std::io::{SeekFrom, Seek, Read, Write};
use std::mem::size_of;
use std::marker::PhantomData;
use byteorder::{BigEndian, ReadBytesExt};
use flume_log::*;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct OffsetCodec<ByteType> {
    last_valid_offset: u64,
    length: u64,
    byte_type: PhantomData<ByteType>
}

impl<ByteType> OffsetCodec<ByteType> {
    pub fn new() -> OffsetCodec<ByteType> {
        OffsetCodec { 
            last_valid_offset: 0, 
            length: 0,
            byte_type: PhantomData }
    }
    pub fn with_length(length: u64) -> OffsetCodec<ByteType> {
        OffsetCodec { 
            last_valid_offset: 0, 
            length,
            byte_type: PhantomData 
        }
    }
}

pub struct OffsetLog<ByteType>{
    file: File,
    offset_codec: OffsetCodec<ByteType>,
}

impl<ByteType> OffsetLog<ByteType> {
    fn new(path: String) -> OffsetLog<ByteType> {

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

        OffsetLog{
            offset_codec,
            file
        }
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.file.flush()
    }
}

impl<ByteType> FlumeLog for OffsetLog<ByteType> {

    fn get(&mut self, seq_num: u64) -> Result<Vec<u8>, ()>{
        let mut buf = vec![0;4096]; //TODO need to allocate according to the size of the thing stored in there.
        //IDEA: the hashview could be queried to find out how big the thing is.
        self.file.seek(SeekFrom::Start(seq_num as u64))
            .and_then(|_|{
                self.file.read(&mut buf)
            })
            .and_then(|n|{
                self.offset_codec.decode(&mut buf.into())
            })
            .map(|val| val.unwrap().data_buffer ) //TODO don't just unwrap here.
            .map_err(|_| ())

    }

    fn latest(&self) -> u64{
        self.offset_codec.last_valid_offset
    }

    fn append(& mut self, buff: &[u8]) -> Result<u64, ()>{
        self.file.seek(SeekFrom::End(0)) // Could store a bool for is_at_end to avoid the sys call. If it is actually a sys call.
            .and_then(|_|{
                let mut vec = Vec::new();
                vec.extend_from_slice(buff);
                let mut encoded = BytesMut::with_capacity(size_of_framing_bytes::<ByteType>() + buff.len());
                self.offset_codec.encode(vec, &mut encoded)
                    .map(|_| encoded)
            })
            .and_then(|data|{
                self.file.write(&data)
            })
            .map(|len| len as u64)
            .or(Err(()))
    }

    fn clear(&mut self, seq_num: u64){
        unimplemented!();
    }
}

#[derive(Debug)]
pub struct Data {
    pub data_buffer: Vec<u8>,
    pub id: u64 
}

fn size_of_framing_bytes<T>() -> usize{
    size_of::<u32>() * 2 + size_of::<T>()
}

fn is_valid_frame<T>(buf: & BytesMut, data_size: usize, last_valid_offset: u64 ) -> bool {
    let second_data_size_index = data_size + size_of::<u32>();
    let filesize_index = data_size + size_of::<u32>() * 2;

    let second_data_size = (&buf[second_data_size_index..]).read_u32::<BigEndian>().unwrap() as usize;
    let file_size = (&buf[filesize_index..]).read_uint::<BigEndian>(size_of::<T>()).unwrap() as u64;

    let next_offset = last_valid_offset + size_of_framing_bytes::<T>() as u64 + data_size as u64;

    next_offset == file_size && second_data_size == data_size 
}

impl<ByteType> Encoder for OffsetCodec<ByteType> {
    type Item = Vec<u8>; //TODO make this a slice
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dest: &mut BytesMut)-> Result<(), Self::Error>{
        let chunk_size = size_of_framing_bytes::<ByteType>() + item.len();
        dest.reserve(chunk_size);
        dest.put_u32_be(item.len() as u32);
        dest.put_slice(&item);
        dest.put_u32_be(item.len() as u32);
        self.length += chunk_size as u64;

        match size_of::<ByteType>() {
            4 => dest.put_u32_be(self.length as u32),
            8 => dest.put_u64_be(self.length as u64),
            _ => panic!("Only supports 32 or 64 bit offset logs.") //TODO: Panicking here doesn't seem great.
        }

        Ok(())
    }
}

impl<ByteType> Decoder for OffsetCodec<ByteType> {
    type Item = Data;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if buf.len() < size_of::<u32>() {
            return Ok(None)
        }
        let data_size = (&buf[..]).read_u32::<BigEndian>().unwrap() as usize;

        if buf.len() < data_size + size_of_framing_bytes::<ByteType>() {
            return Ok(None)
        }

        if !is_valid_frame::<ByteType>(buf, data_size, self.last_valid_offset){
            return Err(io::Error::new(io::ErrorKind::Other, "Frame values were incorrect. The database may be corrupt"))
        }

        buf.advance(size_of::<u32>());//drop off one BytesType
        let data_buffer = buf.split_to(data_size);
        buf.advance(size_of::<u32>() + size_of::<ByteType>());//drop off 2 ByteTypes.
        let data = Data {data_buffer: data_buffer.to_vec(), id: self.last_valid_offset };

        let next_offset = self.last_valid_offset + size_of_framing_bytes::<ByteType>() as u64 + data_size as u64;
        self.last_valid_offset = next_offset;

        Ok(Some(data))
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decode(buf)
    }
}


#[cfg(test)]    
mod test {
    use offset_log::{Decoder, Encoder};
    use offset_log::{OffsetCodec, OffsetLog};
    use flume_log::FlumeLog;
    use bytes::{BytesMut};
    use serde_json::*;
    use std::fs::File;

    #[test]
    fn simple_encode(){
        let mut codec = OffsetCodec::<u32>::new();
        let to_encode = vec![1,2,3,4];
        let mut buf = BytesMut::with_capacity(16);
        codec.encode(to_encode, &mut buf).unwrap();

        assert_eq!(&buf[..], &[0,0,0,4,  1,2,3,4, 0,0,0,4, 0,0,0,16])
    }

    #[test]
    fn simple_encode_u64(){
        let mut codec = OffsetCodec::<u64>::new();
        let to_encode = vec![1,2,3,4];
        let mut buf = BytesMut::with_capacity(20);
        codec.encode(to_encode, &mut buf).unwrap();

        assert_eq!(&buf[..], &[0,0,0,4,  1,2,3,4, 0,0,0,4, 0,0,0,0,0,0,0,20])
    }

    #[test]
    fn encode_multi(){
        let mut codec = OffsetCodec::<u32>::new();
        let to_encode = vec![1,2,3,4];
        let mut buf = BytesMut::with_capacity(32);
        codec.encode(to_encode, &mut buf).unwrap();
        let to_encode = vec![5,6,7,8];
        codec.encode(to_encode, &mut buf).unwrap();

        assert_eq!(&buf[..], &[0,0,0,4,  1,2,3,4, 0,0,0,4, 0,0,0,16, 0,0,0,4, 5,6,7,8, 0,0,0,4, 0,0,0,32])
    }
    #[test]
    fn encode_multi_u64(){
        let mut codec = OffsetCodec::<u64>::new();
        let to_encode = vec![1,2,3,4];
        let mut buf = BytesMut::with_capacity(40);
        codec.encode(to_encode, &mut buf).unwrap();
        let to_encode = vec![5,6,7,8];
        codec.encode(to_encode, &mut buf).unwrap();

        assert_eq!(&buf[0..20], &[0,0,0,4, 1,2,3,4, 0,0,0,4, 0,0,0,0,0,0,0,20]);
        assert_eq!(&buf[20..], &[0,0,0,4, 5,6,7,8, 0,0,0,4, 0,0,0,0,0,0,0,40])
    }
    #[test]
    fn simple(){
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0,0,0,8, 1,2,3,4,5,6,7,8, 0,0,0,8, 0,0,0,20];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1,2,3,4,5,6,7,8]);
            },
            _ => assert!(false)
        }
    }
    #[test]
    fn simple_u64(){
        let mut codec = OffsetCodec::<u64>::new();
        let frame_bytes: &[u8] = &[0,0,0,8, 1,2,3,4,5,6,7,8, 0,0,0,8, 0,0,0,0,0,0,0,24];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1,2,3,4,5,6,7,8]);
            },
            _ => assert!(false)
        }
    }
    #[test]
    fn mulitple(){
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0,0,0,8, 1,2,3,4,5,6,7,8, 0,0,0,8, 0,0,0,20,  0,0,0,8, 9,10,11,12,13,14,15,16, 0,0,0,8, 0,0,0,40];
        let mut bytes = BytesMut::from(frame_bytes);
        let result1 = codec.decode(&mut bytes );

        match result1 {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1,2,3,4,5,6,7,8]);
            },
            _ => assert!(false)
        }
        let result2 = codec.decode(&mut bytes);

        match result2 {
            Ok(Some(data)) => {
                assert_eq!(data.id, 20);
                assert_eq!(&data.data_buffer, &[9,10,11,12,13,14,15,16]);
            },
            _ => assert!(false)
        }
    }
    #[test]
    fn mulitple_u64(){
        let mut codec = OffsetCodec::<u64>::new();
        let frame_bytes: &[u8] = &[0,0,0,8, 1,2,3,4,5,6,7,8, 0,0,0,8, 0,0,0,0,0,0,0,24,  0,0,0,8, 9,10,11,12,13,14,15,16, 0,0,0,8, 0,0,0,0,0,0,0,48];
        let mut bytes = BytesMut::from(frame_bytes);
        let result1 = codec.decode(&mut bytes );

        match result1 {
            Ok(Some(data)) => {
                assert_eq!(data.id, 0);
                assert_eq!(&data.data_buffer, &[1,2,3,4,5,6,7,8]);
            },
            _ => assert!(false)
        }
        let result2 = codec.decode(&mut bytes);

        match result2 {
            Ok(Some(data)) => {
                assert_eq!(data.id, 24);
                assert_eq!(&data.data_buffer, &[9,10,11,12,13,14,15,16]);
            },
            _ => assert!(false)
        }
    }
    #[test]
    fn returns_ok_none_when_buffer_is_incomplete_frame(){
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0,0,0,8, 1,2,3,4,5,6,7,8, 0,0,0,9, 0,0,0];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(None) => assert!(true),
            _ => assert!(false)
        }
    }
    #[test]
    fn returns_ok_none_when_buffer_less_than_4_bytes(){
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0,0,0];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(None) => assert!(true),
            _ => assert!(false)
        }
    }
    #[test]
    fn errors_with_bad_second_size_value(){
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0,0,0,8, 1,2,3,4,5,6,7,8, 0,0,0,9, 0,0,0,20];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(_)) => {
                assert!(false)
            },
            _ => assert!(true)
        }
    }
    #[test]
    fn errors_with_bad_offset_value(){
        let mut codec = OffsetCodec::<u32>::new();
        let frame_bytes: &[u8] = &[0,0,0,8, 1,2,3,4,5,6,7,8, 0,0,0,8, 0,0,0,21];
        let result = codec.decode(&mut BytesMut::from(frame_bytes));

        match result {
            Ok(Some(data)) => {
                assert!(false)
            },
            _ => assert!(true)
        }
    }
    #[test]
    fn read_from_a_file(){
        let mut offset_log = OffsetLog::<u32>::new("./db/test".to_string());
        let result = offset_log.get(0)
            .and_then(|val| from_slice(&val).or(Err(())))
            .map(|val: Value| { 
                match val["value"] {
                    Value::Number(ref num) => num.as_u64().unwrap(),
                    _ => panic!()
                }
            
            })
            .unwrap();
        assert_eq!(result , 0);
    }
    #[test]
    fn write_to_a_file(){
        let filename = "/tmp/test123.offset".to_string(); //careful not to reuse this filename, threads might make things weird
        std::fs::remove_file(filename.clone())
            .or::<Result<()>>(Ok(()))
            .unwrap();

        let test_vec = b"{\"value\": 1}";

        let mut offset_log = OffsetLog::<u32>::new(filename);
        let result = offset_log.append(test_vec)
            .and_then(|_| offset_log.flush().map_err(|_|()))
            .and_then(|_| offset_log.get(0))
            .and_then(|val| from_slice(&val).or(Err(())))
            .map(|val: Value| { 
                match val["value"] {
                    Value::Number(ref num) => {
                        let result = num.as_u64().unwrap();
                        result
                    },
                    _ => panic!()
                }
            
            })
            .unwrap();
        assert_eq!(result , 1);
    }
}
