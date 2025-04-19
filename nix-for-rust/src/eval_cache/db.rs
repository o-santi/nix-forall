use std::ffi::OsStr;
use std::str::FromStr;
use std::path::{PathBuf, Path};
use sqlx::{Pool, Sqlite};
use sqlx::{sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous}, SqlitePool};
use anyhow::{Context, Result};
use tokio::runtime::Runtime;

pub struct FileAttribute {
  path: PathBuf,
  hash: blake3::Hash,
  accessor_path: Vec<String>,
  rt: Runtime,
  sqlite_pool: SqlitePool
}


impl FileAttribute {
  pub fn new<S: AsRef<str>, I: IntoIterator<Item=S>, P: AsRef<Path>>(path: P, accessor_path: I) -> Result<Self> {
    let rt = tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()
      .expect("Could not initialize tokio runtime");
    let sqlite_conn = rt.block_on(setup_db())?;
    Ok(FileAttribute {
      hash: blake3::hash(&std::fs::read(&path)?),
      path: std::fs::canonicalize(path)?,
      accessor_path: accessor_path.into_iter().map(|s| String::from(s.as_ref())).collect(),
      rt,
      sqlite_pool: sqlite_conn
    })
  }

  pub fn is_cached(&self) -> Result<Option<String>> {
    self.rt.block_on(self.query_evaluation_output())
  }

  async fn query_evaluation_output(&self) -> Result<Option<String>> {
    let mut conn = self.sqlite_pool.acquire().await?;
    let eval_outputs: Vec<(i64, String, String)> = sqlx::query_as(r#"
        SELECT id, input_hash, output FROM evaluation_output
        WHERE main_file_path = ? AND main_file_hash = ? AND accessor_path = ?
    "#)
      .bind(self.path.as_os_str().as_encoded_bytes())
      .bind(self.hash.to_string())
      .bind(self.accessor_path.join("."))
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
      let Ok(hash) = hash_files(files) else {
        continue
      };
      if hash.to_string() == input_hash {
        return Ok(Some(output));
      }
    }
    Ok(None)
  }

  
  pub fn insert_evaluation_output(&self, input_files: Vec<PathBuf>, output: &str) -> Result<()> {
    self.rt.block_on(async {
      let mut conn = self.sqlite_pool.acquire().await?;
      let input_hash = hash_files(input_files.clone())?;
      let (evaluation_id, ): (i64, ) = sqlx::query_as(r#"
          INSERT INTO evaluation_output (main_file_path, accessor_path, output, main_file_hash, input_hash) VALUES (?, ?, ?, ?, ?)
          RETURNING id
      "#)
        .bind(self.path.as_os_str().as_encoded_bytes())
        .bind(self.accessor_path.join("."))
        .bind(output)
        .bind(self.hash.to_string())
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
  
}

async fn setup_db() -> Result<Pool<Sqlite>> {
  let cache_directory = home::home_dir()
    .unwrap_or_else(|| Path::new("/tmp").to_path_buf())
    .join(".cache")
    .join("nix-for-rust")
    .join("eval-cache");
  std::fs::create_dir_all(&cache_directory)
    .context("Could not create cache directory")?;
  let sqlite_file = cache_directory.join("sqlite-v1.db");
  let db_url = sqlite_file.as_path().to_string_lossy();
  setup_db_connection(db_url).await
}

pub async fn setup_db_connection<P: AsRef<str>>(database_url: P) -> Result<SqlitePool> {
  let conn_options = SqliteConnectOptions::from_str(database_url.as_ref())?
    .foreign_keys(true)
    .journal_mode(SqliteJournalMode::Wal)
    .synchronous(SqliteSynchronous::Normal)
    .pragma("mmap_size", "134217728")
    .create_if_missing(true);
  let pool = SqlitePool::connect_lazy_with(conn_options);
  sqlx::migrate!("./src/eval_cache/migrations").run(&pool).await?;
  Ok(pool)
}

pub fn hash_files(mut files: Vec<PathBuf>) -> Result<blake3::Hash> {
  let mut hasher = blake3::Hasher::new();
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
