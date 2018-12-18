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

fn find_or_create_author(conn: &Connection, author: &str) -> Result<i64, Error> {
    let mut stmt = conn.prepare_cached("SELECT id FROM author_id WHERE author=?1")?;

    stmt.query_row(&[author], |row| row.get(0))
        .or_else(|_| {
            conn.prepare_cached("INSERT INTO author_id (author) VALUES (?)")
                .map(|mut stmt| stmt.execute(&[author]))
                .map(|_| conn.last_insert_rowid())
        })
        .map_err(|err| err.into())
}

#[derive(Debug, Fail)]
pub enum FlumeViewSqlError {
    #[fail(display = "Db failed integrity check")]
    DbFailedIntegrityCheck {},
}



fn create_author_index(conn: &Connection) -> Result<usize, Error> {
    info!("Creating author index");
    conn.execute(
        "CREATE INDEX author_id_index on message (author_id)",
        NO_PARAMS,
    )
    .map_err(|err| err.into())
}

fn create_links_to_index(conn: &Connection) -> Result<usize, Error> {
    info!("Creating links index");
    conn.execute("CREATE INDEX links_to_index on links (link_to)", NO_PARAMS)
        .map_err(|err| err.into())
}

fn create_content_type_index(conn: &Connection) -> Result<usize, Error> {
    info!("Creating content type index");
    conn.execute(
        "CREATE INDEX content_type_index on message (content_type)",
        NO_PARAMS,
    )
    .map_err(|err| err.into())
}
fn create_tables(conn: &mut Connection) {
    //Size before using author id is 124M.
    //TODO: does seq need to be stored here?
    conn.execute(
        "CREATE TABLE IF NOT EXISTS message (
          id INTEGER PRIMARY KEY,
          key TEXT UNIQUE, 
          seq INTEGER,
          received_time TEXT,
          asserted_time TEXT,
          root TEXT,
          branch TEXT,
          author_id INTEGER,
          content_type TEXT,
          content JSON
        )",
        NO_PARAMS,
    )
    .unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS author_id (
          id INTEGER PRIMARY KEY,
          author TEXT UNIQUE
        )",
        NO_PARAMS,
    )
    .unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS links (
          id INTEGER PRIMARY KEY,
          link_from TEXT,
          link_to TEXT
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

fn create_indices(connection: &Connection) {
    create_author_index(&connection).unwrap();

    create_links_to_index(&connection).unwrap();

    create_content_type_index(&connection).unwrap();
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
        create_indices(&connection);

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
        info!("Start batch append");
        let tx = self.connection.transaction().unwrap();

        for item in items {
            append_item(&tx, item.0, &item.1).unwrap();
        }

        tx.commit().unwrap();
    }

    pub fn check_db_integrity(&mut self) -> Result<(), Error> {
        self.connection.query_row_and_then("PRAGMA integrity_check", NO_PARAMS, |row| {
            row.get_checked(0)
                .map_err(|err| err.into())
                .and_then(|res: String| {
                    if res == "ok" {
                        return Ok(());
                    }
                    return Err(FlumeViewSqlError::DbFailedIntegrityCheck {}.into());
                })
        })
    }
}

fn find_values_in_object_by_key(
    obj: &serde_json::Value,
    key: &str,
    values: &mut Vec<serde_json::Value>,
) {
    match obj.get(key) {
        Some(val) => values.push(val.clone()),
        _ => (),
    };

    match obj {
        Value::Object(kv) => {
            for val in kv.values() {
                match val {
                    Value::Object(_) => find_values_in_object_by_key(val, key, values),
                    _ => (),
                }
            }
        }
        _ => (),
    }
}

fn append_item(connection: &Connection, seq: Sequence, item: &[u8]) -> Result<(), Error> {
    let signed_seq = seq as i64;
    let mut insert_msg_stmt = connection.prepare_cached("INSERT INTO message (id, key, seq, received_time, asserted_time, root, branch, author_id, content_type, content) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)").unwrap();

    let mut insert_link_stmt = connection
        .prepare_cached("INSERT INTO links (link_from, link_to) VALUES (?, ?)")
        .unwrap();

    let message: SsbMessage = serde_json::from_slice(item).unwrap();

    let mut links = Vec::new();
    find_values_in_object_by_key(&message.value.content, "link", &mut links);

    links
        .iter()
        .filter(|link| link.is_string())
        .for_each(|link| {
            insert_link_stmt
                .execute(&[&message.key, link.as_str().unwrap()])
                .unwrap();
        });

    let author_id = find_or_create_author(&connection, &message.value.author).unwrap();
    insert_msg_stmt
        .execute(&[
            &signed_seq as &ToSql,
            &message.key,
            &message.value.sequence,
            &message.timestamp,
            &message.value.timestamp,
            &message.value.content["root"].as_str() as &ToSql,
            &message.value.content["branch"].as_str() as &ToSql,
            &author_id,
            &message.value.content["type"].as_str() as &ToSql,
            &message.value.content as &ToSql,
        ])
        .unwrap();

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

#[derive(Serialize, Deserialize, Debug)]
struct SsbValue {
    author: String,
    sequence: u32,
    timestamp: f64,
    content: Value,
}

#[derive(Serialize, Deserialize, Debug)]
struct SsbMessage {
    key: String,
    value: SsbValue,
    timestamp: f64,
}

#[cfg(test)]
mod test {
    use flume_view::*;
    use flume_view_sql::*;
    use serde_json::*;

    #[test]
    fn find_values_in_object() {
        let obj = json!({ "key": 1, "value": {"link": "hello", "deeper": {"link": "world"}}});

        let mut vec = Vec::new();
        find_values_in_object_by_key(&obj, "link", &mut vec);

        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0].as_str().unwrap(), "hello");
        assert_eq!(vec[1].as_str().unwrap(), "world");
    }

    #[test]
    fn open_connection() {
        let filename = "/tmp/test123456.sqlite3";
        std::fs::remove_file(filename.clone())
            .or::<Result<()>>(Ok(()))
            .unwrap();
        FlumeViewSql::new(filename, 0);
        assert!(true)
    }

    #[test]
    fn append() {
        let expected_seq = 1234;
        let filename = "/tmp/test12345.sqlite3";
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

    #[test]
    fn test_db_integrity_ok() {
        let filename = "/tmp/test_integrity.sqlite3";
        std::fs::remove_file(filename.clone())
            .or::<Result<()>>(Ok(()))
            .unwrap();

        let mut view = FlumeViewSql::new(filename, 0);
        view.check_db_integrity().unwrap();
    }
    #[test]
    fn test_db_integrity_fails() {
        let filename = "/tmp/test_integrity_bad.sqlite3";
        std::fs::remove_file(filename.clone())
            .or::<Result<()>>(Ok(()))
            .unwrap();

        let mut view = FlumeViewSql::new(filename.clone(), 0);

        std::fs::write(filename, b"BANG").unwrap();

        match view.check_db_integrity() {
            Ok(_) => panic!(),
            Err(_) => assert!(true)
        }
    }
}

