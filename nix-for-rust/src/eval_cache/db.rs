use std::ffi::OsStr;
use std::str::FromStr;
use std::path::{PathBuf, Path};
use sqlx::{Acquire, Pool, Sqlite};
use sqlx::{sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous}, SqlitePool};
use anyhow::Result;
use std::sync::LazyLock;
use super::FileAttribute;
use tokio::runtime::Runtime;

static TOKIO_RT: LazyLock<Runtime> = LazyLock::new(|| {
  tokio::runtime::Builder::new_current_thread()
    .enable_time()
    .build()
    .expect("Could not initialize tokio runtime")
});

static SQLITE_POOL: LazyLock<Pool<Sqlite>> = LazyLock::new(|| {
  let cache_directory = home::home_dir()
    .unwrap_or(Path::new("/tmp").to_path_buf())
    .join(".cache")
    .join("nix-for-rust")
    .join("eval-cache");
  std::fs::create_dir_all(&cache_directory).expect("Could not create cache directory");
  let sqlite_file = cache_directory.join("sqlite-v1.db");
  let db_url = sqlite_file.as_path().to_string_lossy();
  TOKIO_RT.block_on(async { setup_db(db_url).await.expect("Could not run database setup") })
});

pub fn query_attr_in_cache(file_attr: &FileAttribute) -> Result<Option<String>> {
  let pool = &*SQLITE_POOL;
  TOKIO_RT.block_on(async {
    query_evaluation_output(pool, file_attr).await
  })
}


pub async fn setup_db<P: AsRef<str>>(database_url: P) -> Result<SqlitePool> {
  let conn_options = SqliteConnectOptions::from_str(database_url.as_ref())?
    .foreign_keys(true)
    .journal_mode(SqliteJournalMode::Wal)
    .synchronous(SqliteSynchronous::Normal)
    .pragma("mmap_size", "134217728")
    .create_if_missing(true);
  let pool = SqlitePool::connect_with(conn_options).await?;
  sqlx::migrate!("./src/eval_cache/migrations").run(&pool).await?;
  Ok(pool)
}

pub fn hash_files(files: &[PathBuf]) -> Result<blake3::Hash> {
  let mut hasher = blake3::Hasher::new();
  let mut files = files.to_vec();
  files.sort();
  for file in files {
    if file.is_dir() {
      for entry in std::fs::read_dir(file)? {
        hasher.update(entry?.path().as_os_str().as_encoded_bytes());
      }
    } else {
      hasher.update_mmap(&file)?;
    }
  }
  Ok(hasher.finalize())
}

pub async fn query_evaluation_output<'a, A>(conn: A, file_attr: &FileAttribute) -> Result<Option<String>>
where
    A: Acquire<'a, Database = Sqlite>,
{
  let mut conn = conn.acquire().await?;
  let eval_outputs: Vec<(i64, String, String)> = sqlx::query_as(r#"
     SELECT id, input_hash, output FROM evaluation_output
     WHERE main_file_path = ? AND main_file_hash = ? AND accessor_path = ? "#)
    .bind(file_attr.path.as_os_str().as_encoded_bytes())
    .bind(file_attr.hash.to_string())
    .bind(file_attr.accessor_path.join("."))
    .fetch_all(&mut *conn)
    .await?;
  for (evaluation_id, input_hash, output) in eval_outputs {
    let files: Vec<PathBuf> = sqlx::query_as("SELECT file_path FROM evaluation_input WHERE evaluation_id = ?")
      .bind(evaluation_id)
      .fetch_all(&mut *conn)
      .await?
      .into_iter()
      .map(|(path,): (Vec<u8>, )| unsafe {
        Path::new(OsStr::from_encoded_bytes_unchecked(&path)).to_path_buf()
      })
      .collect();
    let hash = hash_files(&files)?;
    if hash.to_string() == input_hash {
      return Ok(Some(output));
    }
  }
  Ok(None)
}

pub fn insert_evaluation_output(file_attr: &FileAttribute, input_files: Vec<PathBuf>, output: &str) -> Result<()> {
  TOKIO_RT.block_on(async {
    let mut conn = SQLITE_POOL.acquire().await?;
    let input_hash = hash_files(&input_files)?;
    let (evaluation_id, ): (i64, ) = sqlx::query_as(r#"
        INSERT INTO evaluation_output (main_file_path, accessor_path, output, main_file_hash, input_hash) VALUES (?, ?, ?, ?, ?)
        RETURNING id
      "#)
      .bind(file_attr.path.as_os_str().as_encoded_bytes())
      .bind(file_attr.accessor_path.join("."))
      .bind(output)
      .bind(file_attr.hash.to_string())
      .bind(input_hash.to_string())
      .fetch_one(&mut *conn)
      .await?;
    for file in input_files {
      sqlx::query("INSERT INTO evaluation_input (evaluation_id, file_path) VALUES (?, ?)")
        .bind(evaluation_id)
        .bind(file.as_os_str().as_encoded_bytes())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
  })
}
