use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{PathBuf, Path};
use std::cell::RefCell;
use rusqlite::{Connection, params};
use anyhow::Result;
use super::FileAttribute;

const MIGRATION: &str = include_str!("migrations/20241209_init.sql");

struct DbState {
  pid: u32,
  conn: Connection,
}

thread_local! {
  static DB_STATE: RefCell<Option<DbState>> = RefCell::new(None);
}

fn with_conn<F, T>(f: F) -> Result<T>
where
  F: FnOnce(&Connection) -> Result<T>,
{
  DB_STATE.with(|cell| {
    let mut state = cell.borrow_mut();
    let current_pid = std::process::id();
    if state.as_ref().map_or(true, |s| s.pid != current_pid) {
      *state = Some(DbState { pid: current_pid, conn: setup_db()? });
    }
    f(&state.as_ref().unwrap().conn)
  })
}

pub fn setup_db() -> Result<Connection> {
  let cache_directory = home::home_dir()
    .unwrap_or_else(|| Path::new("/tmp").to_path_buf())
    .join(".cache")
    .join("nix-for-rust")
    .join("eval-cache");
  std::fs::create_dir_all(&cache_directory).expect("Could not create cache directory");
  let conn = Connection::open(cache_directory.join("sqlite-v1.db"))?;
  conn.execute_batch("
    PRAGMA journal_mode=WAL;
    PRAGMA synchronous=NORMAL;
    PRAGMA mmap_size=134217728;
    PRAGMA foreign_keys=ON;
  ")?;
  conn.execute_batch(MIGRATION)?;
  Ok(conn)
}

pub fn query_evaluation_output(conn: &Connection, file_attr: &FileAttribute) -> Result<Option<String>> {
  let mut stmt = conn.prepare_cached(
    "SELECT id, input_hash, output FROM evaluation_output
     WHERE main_file_path = ? AND main_file_hash = ? AND accessor_path = ?"
  )?;
  let rows: Vec<(i64, String, String)> = stmt.query_map(
    params![
      file_attr.path.as_os_str().as_encoded_bytes(),
      file_attr.hash.to_string(),
      file_attr.accessor_path.join(".")
    ],
    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
  )?.collect::<rusqlite::Result<_>>()?;

  for (evaluation_id, input_hash, output) in rows {
    let mut stmt = conn.prepare_cached(
      "SELECT file_path FROM evaluation_input WHERE evaluation_id = ?"
    )?;
    let files: Vec<PathBuf> = stmt.query_map(params![evaluation_id], |row| {
      let bytes: Vec<u8> = row.get(0)?;
      Ok(PathBuf::from(OsString::from_vec(bytes)))
    })?.collect::<rusqlite::Result<_>>()?;

    let Ok(hash) = hash_files(&files) else { continue };
    if hash.to_string() == input_hash {
      return Ok(Some(output));
    }
  }
  Ok(None)
}

pub fn query_attr_in_cache(file_attr: &FileAttribute) -> Result<Option<String>> {
  with_conn(|conn| query_evaluation_output(conn, file_attr))
}

pub fn insert_evaluation_output(file_attr: &FileAttribute, input_files: Vec<PathBuf>, output: &str) -> Result<()> {
  with_conn(|conn| {
    let input_hash = hash_files(&input_files)?;
    let evaluation_id: i64 = conn.query_row(
      "INSERT INTO evaluation_output
         (main_file_path, accessor_path, output, main_file_hash, input_hash)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id",
      params![
        file_attr.path.as_os_str().as_encoded_bytes(),
        file_attr.accessor_path.join("."),
        output,
        file_attr.hash.to_string(),
        input_hash.to_string(),
      ],
      |row| row.get(0),
    )?;
    let mut stmt = conn.prepare_cached(
      "INSERT INTO evaluation_input (evaluation_id, file_path) VALUES (?, ?)"
    )?;
    for file in &input_files {
      stmt.execute(params![evaluation_id, file.as_os_str().as_encoded_bytes()])?;
    }
    Ok(())
  })
}

pub fn hash_files(files: &[PathBuf]) -> Result<blake3::Hash> {
  let mut hasher = blake3::Hasher::new();
  let mut files = files.to_vec();
  files.sort();
  for file in files {
    // metadata() follows symlinks, matching the previous file.is_dir() check.
    // A vanished/unreadable path has nothing stable to hash; skip it.
    let md = match std::fs::metadata(&file) {
      Ok(md) => md,
      Err(_) => continue,
    };
    if md.is_dir() {
      for entry in std::fs::read_dir(&file)? {
        hasher.update(entry?.path().as_os_str().as_encoded_bytes());
      }
    } else if md.is_file() {
      hasher.update_mmap(&file)?;
    } else {
      // Non-regular file (character/block device, fifo, socket). Never stream
      // its contents: e.g. /dev/urandom is infinite and would spin forever.
      // Fold only the path so the cache key still varies with the input set.
      hasher.update(file.as_os_str().as_encoded_bytes());
    }
  }
  Ok(hasher.finalize())
}
