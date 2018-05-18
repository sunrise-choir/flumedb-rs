use tokio_io::codec::Decoder;
use bytes::{BytesMut};
use std::{io };
use std::mem::size_of;
use std::marker::PhantomData;
use byteorder::{BigEndian, ReadBytesExt};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct OffsetCodec<ByteType> {
    last_valid_offset: usize,
    byte_type: PhantomData<ByteType>
}

impl<ByteType> OffsetCodec<ByteType> {
    pub fn new() -> OffsetCodec<ByteType> {
        OffsetCodec { last_valid_offset: 0, byte_type: PhantomData }
    }
}

#[derive(Debug)]
pub struct Data {
    pub data_buffer: Vec<u8>,
    pub id: usize 
}

fn size_of_framing_bytes<T>() -> usize{
    size_of::<u32>() * 2 + size_of::<T>()
}

fn is_valid_frame<T>(buf: & BytesMut, data_size: usize, last_valid_offset: usize ) -> bool {
    let second_data_size_index = data_size + size_of::<u32>();
    let filesize_index = data_size + size_of::<u32>() * 2;

    let second_data_size = (&buf[second_data_size_index..]).read_uint::<BigEndian>(size_of::<u32>()).unwrap() as usize;
    let file_size = (&buf[filesize_index..]).read_uint::<BigEndian>(size_of::<T>()).unwrap() as usize;

    let next_offset = last_valid_offset + size_of_framing_bytes::<T>() + data_size as usize;

    next_offset == file_size && second_data_size == data_size 
}

impl<ByteType> Decoder for OffsetCodec<ByteType> {
    type Item = Data;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if buf.len() < size_of::<u32>() {
            return Ok(None)
        }
        let data_size = (&buf[..]).read_uint::<BigEndian>(size_of::<u32>()).unwrap() as usize;

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

        let next_offset = self.last_valid_offset + size_of_framing_bytes::<ByteType>() + data_size;
        self.last_valid_offset = next_offset;

        Ok(Some(data))
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decode(buf)
    }
}


#[cfg(test)]    
mod test {
    use offset_log::Decoder;
    use offset_log::OffsetCodec;
    use bytes::{BytesMut};

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
}
