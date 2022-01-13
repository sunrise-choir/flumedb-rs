pub use bidir_iter::BidirIterator;

use crate::flume_log::*;
use crate::iter_at_offset::IterAtOffset;
use crate::log_entry::LogEntry;
use buffered_offset_reader::{BufOffsetReader, OffsetRead, OffsetReadMut};
use byteorder::{BigEndian, ReadBytesExt};
use serde_cbor::from_slice;
use serde_cbor::Value as CborValue;
use serde_json::{json, Value};
use ssb_multiformats::multihash::Multihash;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{Seek, SeekFrom};
use std::mem::size_of;
use std::path::Path;

const DATA_FILE_NAME: &str = "data";

#[derive(Debug, Fail)]
pub enum GoFlumeOffsetLogError {
    #[fail(display = "Incorrect framing values detected, log file might be corrupt")]
    CorruptLogFile {},
    #[fail(
        display = "Incorrect values in journal file. File might be corrupt, or we might need better file locking."
    )]
    CorruptJournalFile {},
    #[fail(display = "Incorrect values in offset file. File might be corrupt.")]
    CorruptOffsetFile {},
    #[fail(display = "Unsupported message type in offset log")]
    UnsupportedMessageType {},

    #[fail(display = "The decode buffer passed to decode was too small")]
    DecodeBufferSizeTooSmall {},
}

#[derive(Debug, Default, Deserialize)]
struct GoMsgPackKey<'a> {
    #[serde(rename = "Algo")]
    algo: &'a str,
    #[serde(rename = "Hash")]
    #[serde(with = "serde_bytes")]
    hash: &'a [u8],
}

impl<'a> GoMsgPackKey<'a> {
    pub fn to_legacy_string(&self) -> String {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&self.hash[..32]);
        let multi_hash = Multihash::Message(arr);
        multi_hash.to_legacy_string()
    }
}

#[derive(Debug, Default, Deserialize)]
struct GoMsgPackData<'a> {
    #[serde(rename = "Raw_")]
    raw: &'a str,
    #[serde(rename = "Key_")]
    key: GoMsgPackKey<'a>,
    #[serde(rename = "Sequence_")]
    sequence: u64,
}

type GoCborKey<'a> = (&'a [u8], &'a str);
type GoCborTuple<'a> = (GoCborKey<'a>, CborValue, GoCborKey<'a>, i128, f64, &'a [u8]);

pub struct GoOffsetLog {
    pub data_file: File,
    end_of_file: u64,
}

// A Frame is like a LogEntry, but without the data
#[derive(Debug)]
pub struct Frame {
    pub data_size: usize,
    pub offset: u64,
}

impl Frame {
    fn data_start(&self) -> u64 {
        self.offset + size_of::<u64>() as u64
    }
}

#[derive(Debug)]
pub struct ReadResult {
    pub entry: LogEntry,
    pub next: u64,
}

impl GoOffsetLog {
    /// Where path is a path to the directory that contains go log files
    pub fn new<P: AsRef<Path>>(path: P) -> Result<GoOffsetLog, Error> {
        let data_file_path = Path::new(path.as_ref()).join(DATA_FILE_NAME);

        let data_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(data_file_path)?;

        GoOffsetLog::from_files(data_file)
    }

    /// Where path is a path to the directory that contains go log files
    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<GoOffsetLog, Error> {
        let data_file_path = Path::new(path.as_ref()).join(DATA_FILE_NAME);
        let file = OpenOptions::new().read(true).open(&data_file_path)?;

        GoOffsetLog::from_files(file)
    }

    pub fn from_files(mut data_file: File) -> Result<GoOffsetLog, Error> {
        let file_length = data_file.seek(SeekFrom::End(0))?;

        Ok(GoOffsetLog {
            data_file,
            end_of_file: file_length,
        })
    }

    pub fn end(&self) -> u64 {
        self.end_of_file
    }

    pub fn read(&self, offset: u64) -> Result<ReadResult, Error> {
        read_next::<_>(offset, &self.data_file)
    }

    pub fn append_batch(&mut self, _buffs: &[&[u8]]) -> Result<Vec<u64>, Error> {
        unimplemented!()
    }

    pub fn iter(&self) -> GoOffsetLogIter {
        // TODO: what are the chances that try_clone() will fail?
        //  I'd rather not return a Result<> here.
        GoOffsetLogIter::new(self.data_file.try_clone().unwrap())
    }
}

pub struct GoOffsetLogIter {
    reader: BufOffsetReader<File>,
    current: u64,
    next: u64,
}

impl GoOffsetLogIter {
    pub fn new(file: File) -> GoOffsetLogIter {
        GoOffsetLogIter::with_starting_offset(file, 0)
    }

    pub fn with_starting_offset(file: File, offset: u64) -> GoOffsetLogIter {
        GoOffsetLogIter {
            reader: BufOffsetReader::new(file),
            current: offset,
            next: offset,
        }
    }
}

impl Iterator for GoOffsetLogIter {
    type Item = LogEntry;

    fn next(&mut self) -> Option<Self::Item> {
        self.current = self.next;
        let r = read_next_mut::<_>(self.current, &mut self.reader).ok()?;
        self.next = r.next;
        Some(r.entry)
    }
}

impl IterAtOffset<GoOffsetLogIter> for GoOffsetLog {
    fn iter_at_offset(&self, offset: u64) -> GoOffsetLogIter {
        GoOffsetLogIter::with_starting_offset(self.data_file.try_clone().unwrap(), offset)
    }
}
pub fn read_next<R: OffsetRead>(offset: u64, r: &R) -> Result<ReadResult, Error> {
    read_next_impl::<_>(offset, |b, o| r.read_at(b, o))
}

pub fn read_next_mut<R: OffsetReadMut>(offset: u64, r: &mut R) -> Result<ReadResult, Error> {
    read_next_impl::<_>(offset, |b, o| r.read_at(b, o))
}

fn read_next_impl<F>(offset: u64, mut read_at: F) -> Result<ReadResult, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    let frame = read_next_frame::<_>(offset, &mut read_at)?;
    read_entry::<_>(&frame, &mut read_at)
}

fn read_next_frame<F>(offset: u64, read_at: &mut F) -> Result<Frame, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    // Entry is [payload size: u64, payload ]

    const HEAD_SIZE: usize = size_of::<u64>();

    let mut head_bytes = [0; HEAD_SIZE];
    let n = read_at(&mut head_bytes, offset)?;
    if n < HEAD_SIZE {
        return Err(GoFlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    let data_size = (&head_bytes[..]).read_u64::<BigEndian>()? as usize;
    Ok(Frame { offset, data_size })
}

fn read_entry<F>(frame: &Frame, read_at: &mut F) -> Result<ReadResult, Error>
where
    F: FnMut(&mut [u8], u64) -> io::Result<usize>,
{
    // Entry is [payload size: u64, payload ]

    let mut buf = vec![0; frame.data_size];

    let n = read_at(&mut buf, frame.data_start())?;
    if n < frame.data_size {
        return Err(GoFlumeOffsetLogError::DecodeBufferSizeTooSmall {}.into());
    }

    if buf[0] != 1 {
        return Err(GoFlumeOffsetLogError::UnsupportedMessageType {}.into());
    }

    let tuple: GoCborTuple = from_slice(&buf[1..])?;

    let (_, _, (hash, algo), seq, timestamp, raw) = tuple;

    let key = GoMsgPackKey { algo, hash };

    let cbor = GoMsgPackData {
        raw: std::str::from_utf8(raw)?,
        key,
        sequence: seq as u64,
    };
    // The go log stores data in msg pack.
    // There is a "Raw" field that has the json used for
    // signing.
    // But we also need to get the key that is encoded in msg pack and build a traditional json ssb
    // message that has "key" "value" and "timestamp".
    //let msg_packed: GoMsgPackData = decode::from_slice(&mut buf.as_slice())?;

    let ssb_message = json!({
        "key": cbor.key.to_legacy_string(),
        "value": serde_json::from_str::<Value>(&cbor.raw)?,
        "timestamp": timestamp as u64
    });

    let data = ssb_message.to_string().into_bytes();

    Ok(ReadResult {
        entry: LogEntry {
            offset: frame.offset,
            data,
        },
        next: frame.data_size as u64 + size_of::<u64>() as u64 + frame.offset,
    })
}

#[cfg(test)]
mod test {
    extern crate serde_json;

    use crate::go_offset_log::*;
    use serde_json::Value;
    use std::path::PathBuf;

    #[test]
    fn open_ro() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("test_vecs/four_ssb_messages");
        let log = GoOffsetLog::open_read_only(d).unwrap();
        let vec = log
            .iter()
            .map(|log_entry| log_entry.data)
            .map(|data| serde_json::from_slice::<Value>(&data).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0]["value"]["previous"], Value::Null);
        assert_eq!(vec[1]["value"]["content"]["hello"], "piet!!!");
    }
    #[test]
    fn open_empty() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("test_vecs/empty");
        let log = GoOffsetLog::new(d).unwrap();
        let vec = log.iter().collect::<Vec<_>>();

        assert_eq!(vec.len(), 0);
    }

    #[test]
    fn ssb_messages() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("test_vecs/four_ssb_messages");
        let log = GoOffsetLog::new(d).unwrap();
        let vec = log
            .iter()
            .map(|log_entry| log_entry.data)
            .map(|data| serde_json::from_slice::<Value>(&data).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0]["value"]["previous"], Value::Null);
        assert_eq!(vec[1]["value"]["content"]["hello"], "piet!!!");
    }
}
