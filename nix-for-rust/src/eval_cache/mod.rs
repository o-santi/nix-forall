mod db;
mod trace;

use std::path::Path;
use anyhow::Result;

use trace::FileTracer;
use db::FileAttribute;
use crate::eval::NixEvalState;

impl NixEvalState {

  pub fn eval_attr_from_file<S: AsRef<str>, I: IntoIterator<Item=S> + Clone, P: AsRef<Path>>(&self, file_path: P, accessor_path: I) -> Result<String> {
    let file_attribute = FileAttribute::new(file_path.as_ref(), accessor_path.clone())?;
    if let Some(p) = file_attribute.is_cached()? {
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
      file_attribute.insert_evaluation_output(input_files.into_iter().collect(), &out)?;
      Ok(out)
    }
  }
}
