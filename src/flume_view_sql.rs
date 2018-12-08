use failure::Error;
use flume_view::*;

use rusqlite::types::ToSql;
use rusqlite::OpenFlags;
use rusqlite::{Connection, NO_PARAMS};
use serde_json::Value;

use log;

pub struct FlumeViewSql {
    connection: Connection,
    latest: Sequence,
}

fn set_pragmas(conn: &mut Connection) {
    conn.execute("PRAGMA synchronous = OFF", NO_PARAMS).unwrap();
    conn.execute("PRAGMA page_size = 8192", NO_PARAMS).unwrap();
}

fn create_tables(conn: &mut Connection) {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS message (
          id INTEGER PRIMARY KEY,
          key TEXT UNIQUE, 
          seq INTEGER,
          received_time TEXT,
          asserted_time TEXT,
          root TEXT,
          branch TEXT,
          author TEXT,
          content_type TEXT,
          content JSON
        )",
        NO_PARAMS,
    )
    .unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS links (
          id INTEGER PRIMARY KEY,
          links_from INTEGER,
          links_to INTEGER
        )",
        NO_PARAMS,
    )
    .unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS heads (
          id INTEGER PRIMARY KEY,
          links_from INTEGER,
          links_to INTEGER
        )",
        NO_PARAMS,
    )
    .unwrap();
}

impl FlumeViewSql {
    pub fn new(path: &str, latest: Sequence) -> FlumeViewSql {
        //let mut connection = Connection::open(path).expect("unable to open sqlite connection");
        let flags: OpenFlags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection =
            Connection::open_with_flags(path, flags).expect("unable to open sqlite connection");

        set_pragmas(&mut connection);
        create_tables(&mut connection);

        FlumeViewSql { connection, latest }
    }

    pub fn get_seq_by_key(&mut self, key: String) -> Result<i64, Error> {
        let mut stmt = self
            .connection
            .prepare("SELECT id FROM message WHERE key=?1")?;

        stmt.query_row(&[key], |row| row.get(0))
            .map_err(|err| err.into())
    }
    pub fn get_seqs_by_type(&mut self, content_type: String) -> Result<Vec<i64>, Error> {
        let mut stmt = self
            .connection
            .prepare("SELECT id FROM message WHERE content_type=?1")?;

        let rows = stmt.query_map(&[content_type], |row| row.get(0))?;

        let seqs = rows.fold(Vec::<i64>::new(), |mut vec, row| {
            vec.push(row.unwrap());
            vec
        });

        Ok(seqs)
    }

    pub fn append_batch(&mut self, items: Vec<(Sequence, Vec<u8>)>) {
        let tx = self.connection.transaction().unwrap();

        for item in items {
            append_item(&tx, item.0, &item.1).unwrap();
        }

        tx.commit().unwrap();
    }
}

fn append_item(connection: &Connection, seq: Sequence, item: &[u8]) -> Result<(), Error> {
    let signed_seq = seq as i64;
    let mut stmt = connection.prepare_cached("INSERT INTO message (id, key, seq, received_time, asserted_time, root, branch, author, content_type, content) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)").unwrap();

    serde_json::from_slice(item)
        .map(|message: SsbMessage| {
            stmt.execute(&[
                &signed_seq as &ToSql,
                &message.key,
                &message.value.sequence,
                &message.timestamp,
                &message.value.timestamp,
                &message.value.content["root"].as_str() as &ToSql,
                &message.value.content["branch"].as_str() as &ToSql,
                &message.value.author,
                &message.value.content["type"].as_str() as &ToSql,
                &message.value.content as &ToSql,
            ])
            .unwrap();
        })
        .unwrap_or({
            //warn!("Unable to parse item as a ssb message, seq: {}", seq);
        });

    Ok(())
}

impl FlumeView for FlumeViewSql {
    fn append(&mut self, seq: Sequence, item: &[u8]) {
        append_item(&self.connection, seq, item).unwrap()
    }
    fn latest(&self) -> Sequence {
        self.latest
    }
}

#[derive(Serialize, Deserialize)]
struct SsbValue {
    author: String,
    sequence: u32,
    timestamp: i64,
    content: Value,
}

#[derive(Serialize, Deserialize)]
struct SsbMessage {
    key: String,
    value: SsbValue,
    timestamp: i64,
}

#[cfg(test)]
mod test {
    use flume_view::*;
    use flume_view_sql::*;
    use serde_json::*;

    #[test]
    fn open_connection() {
        FlumeViewSql::new("/tmp/test.sqlite3", 0);
        assert!(true)
    }

    #[test]
    fn append() {
        let expected_seq = 1234;
        let filename = "/tmp/test.sqlite3";
        std::fs::remove_file(filename.clone())
            .or::<Result<()>>(Ok(()))
            .unwrap();

        let mut view = FlumeViewSql::new(filename, 0);
        let jsn = r#####"{
  "key": "%KKPLj1tWfuVhCvgJz2hG/nIsVzmBRzUJaqHv+sb+n1c=.sha256",
  "value": {
    "previous": "%xsMQA2GrsZew0GSxmDSBaoxDafVaUJ07YVaDGcp65a4=.sha256",
    "author": "@QlCTpvY7p9ty2yOFrv1WU1AE88aoQc4Y7wYal7PFc+w=.ed25519",
    "sequence": 4797,
    "timestamp": 1543958997985,
    "hash": "sha256",
    "content": {
      "type": "post",
      "root": "%9EdpeKC5CgzpQs/x99CcnbD3n6ugUlwm19F7ZTqMh5w=.sha256",
      "branch": "%sQV8QpyUNvh7fBAs2ts00Qo2gj44CQBmwonWJzm+AeM=.sha256",
      "reply": {
        "%9EdpeKC5CgzpQs/x99CcnbD3n6ugUlwm19F7ZTqMh5w=.sha256": "@+UMKhpbzXAII+2/7ZlsgkJwIsxdfeFi36Z5Rk1gCfY0=.ed25519",
        "%sQV8QpyUNvh7fBAs2ts00Qo2gj44CQBmwonWJzm+AeM=.sha256": "@vzoU7/XuBB5B0xueC9NHFr9Q76VvPktD9GUkYgN9lAc=.ed25519"
      },
      "channel": null,
      "recps": null,
      "text": "If I understand correctly, cjdns overlaying over old IP (which is basically all of the cjdns uses so far) still requires old IP addresses to introduce you to the cjdns network, so the chicken and egg problem is still there.",
      "mentions": []
    },
    "signature": "mi5j/buYZdsiH8l6CVWRqdBKe+0UG6tVTOoVVjMhYl38Nkmb8wiIEfe7zu0JWuiHkaAIq+0/ZqYr6aV14j4fAw==.sig.ed25519"
  },
  "timestamp": 1543959001933
}
"#####;
        view.append(expected_seq, jsn.as_bytes());
        let seq = view
            .get_seq_by_key("%KKPLj1tWfuVhCvgJz2hG/nIsVzmBRzUJaqHv+sb+n1c=.sha256".to_string())
            .unwrap();
        assert_eq!(seq, expected_seq as i64);

        let seqs = view.get_seqs_by_type("post".to_string()).unwrap();
        assert_eq!(seqs[0], expected_seq as i64);
    }
}
