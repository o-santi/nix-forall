mod db;
mod trace;

use std::path::{Path, PathBuf};
use anyhow::Result;

use trace::FileTracer;
use crate::eval::NixEvalState;

struct FileAttribute {
  path: PathBuf,
  hash: blake3::Hash,
  accessor_path: Vec<String>
}

impl FileAttribute {
  pub fn new<S: AsRef<str>, I: IntoIterator<Item=S>, P: AsRef<Path>>(path: P, accessor_path: I) -> Result<Self> {
    Ok(FileAttribute {
      hash: blake3::hash(&std::fs::read(&path)?),
      path: std::fs::canonicalize(path)?,
      accessor_path: accessor_path.into_iter().map(|s| String::from(s.as_ref())).collect()
    })
  }
}

impl NixEvalState {

  pub fn eval_attr_from_file<S: AsRef<str>, I: IntoIterator<Item=S> + Clone, P: AsRef<Path>>(&self, file_path: P, accessor_path: I) -> Result<String> {
    let file_attribute = FileAttribute::new(&file_path, accessor_path.clone())?;
    if let Some(p) = db::query_attr_in_cache(&file_attribute)? {
      Ok(p)
    } else {
      let tracer = FileTracer::new();
      let (out, input_files) = tracer.watch_call(|| {
        let attrset = self.eval_file(file_path)?;
        let string = attrset.get_path(accessor_path)?
          .as_str()?
          .to_string();
        Ok(string)
      })?;
      db::insert_evaluation_output(&file_attribute, input_files.into_iter().collect(), &out)?;
      Ok(out)
    }
  }
}
