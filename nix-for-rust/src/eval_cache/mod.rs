mod db;
mod trace;

use anyhow::Result;
use interprocess::unnamed_pipe::Sender;
use nix::sys::signal;
use nix::unistd::{fork, ForkResult};
use trace::trace_accessed_files;
use std::io::{BufReader, BufRead, Write};
use std::path::{PathBuf, Path};
use nix::sys::ptrace;

use crate::eval::NixEvalState;
use crate::term::{NixTerm, Repr};

struct FileAttribute {
  path: PathBuf,
  hash: blake3::Hash,
  accessor_path: Vec<String>
}

impl FileAttribute {
  pub fn new<'s, I: IntoIterator<Item=&'s str>, P: AsRef<Path>>(path: P, accessor_path: I) -> Result<Self> {
    Ok(FileAttribute {
      hash: blake3::hash(&std::fs::read(&path)?),
      path: std::fs::canonicalize(path)?,
      accessor_path: accessor_path.into_iter().map(String::from).collect()
    })
  }
}

impl NixEvalState {

  fn evaluate_traced<'s, I: IntoIterator<Item=&'s str>, P: AsRef<Path>>(&self, file: P, accessor_path: I, mut sender: Sender) {
    ptrace::traceme().unwrap();
    signal::raise(signal::Signal::SIGSTOP).unwrap();
    let path = accessor_path
      .into_iter()
      .fold(self.eval_file(file), |attr, accessor| attr.and_then(|term| term
        .get(accessor)
        .map_err(|e| anyhow::format_err!(e))))
      .and_then(|term| match term {
        NixTerm::String(p) => Ok(p),
        other => Err(anyhow::format_err!("Attribute did not evaluate to string: '{}'", other.repr().unwrap()))
      });
    match path {
      Ok(p) => {
        sender.write_all(p.as_bytes()).unwrap();
        sender.write(b"\n").unwrap();
      },
      Err(e) => {
        sender.write_all(b"\n").unwrap();
        eprintln!("{e}");
      },
    }
  }

  pub fn eval_attr_from_file<'s, I: IntoIterator<Item=&'s str> + Clone, P: AsRef<Path>>(&self, file: P, accessor_path: I) -> Result<String> {
    let file_attribute = FileAttribute::new(&file, accessor_path.clone())?;
    if let Some(p) = db::query_attr_in_cache(&file_attribute)? {
      Ok(p)
    } else {
      let (sender, receiver) = interprocess::unnamed_pipe::pipe()?;
      match unsafe { fork()? } {
        ForkResult::Child => {
          self.evaluate_traced(file, accessor_path, sender);
          std::process::exit(0);
        }
        ForkResult::Parent { child } => {
          let input_files = trace_accessed_files(child);
          let mut output = String::new();
          BufReader::new(receiver).read_line(&mut output)?;
          output = String::from(output.trim());
          if output == "" {
            return Err(anyhow::format_err!("Error while evaluating expression"));
          };
          db::insert_evaluation_output(&file_attribute, input_files.into_iter().collect(), &output)?;
          Ok(output)
        }
      }
    }
  }
}
