mod db;
mod trace;

use anyhow::Result;
use interprocess::unnamed_pipe::Sender;
use nix::sys::signal;
use nix::unistd::{fork, ForkResult};
use trace::FileTracer;
use std::io::{BufReader, BufRead, Write};
use std::path::{PathBuf, Path};
use nix::sys::ptrace;
use libseccomp::{ScmpAction, ScmpArch, ScmpFilterContext, ScmpSyscall};

use crate::eval::NixEvalState;
use crate::term::{NixTerm, Repr};

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

fn set_seccomp() -> Result<()> {
  ScmpFilterContext::new(ScmpAction::Allow)?
    .add_arch(ScmpArch::native())?
    .add_rule(ScmpAction::Trace(0), ScmpSyscall::from_name("open")?)?
    .add_rule(ScmpAction::Trace(0), ScmpSyscall::from_name("openat")?)?
    .add_rule(ScmpAction::Trace(0), ScmpSyscall::from_name("read")?)?
    .add_rule(ScmpAction::Trace(0), ScmpSyscall::from_name("getdents")?)?
    .add_rule(ScmpAction::Trace(0), ScmpSyscall::from_name("getdents64")?)?
    .load()?;
  Ok(())
}

impl NixEvalState {

  fn evaluate_traced<'s, S: AsRef<str>, I: IntoIterator<Item=S>, P: AsRef<Path>>(&self, file: P, accessor_path: I, mut sender: Sender) {
    set_seccomp().unwrap();
    ptrace::traceme().unwrap();
    signal::raise(signal::Signal::SIGSTOP).unwrap();
    let path = self.eval_file(file)
      .and_then(|attr| attr.get_path(accessor_path).map_err(|e| e.into()))
      .and_then(|string| match string {
        NixTerm::String(s) => Ok(s),
        _ => Err(anyhow::format_err!("term did not evaluate to string"))
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

  pub fn eval_attr_from_file<S: AsRef<str>, I: IntoIterator<Item=S> + Clone, P: AsRef<Path>>(&self, file: P, accessor_path: I) -> Result<String> {
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
          let tracer = FileTracer::new();
          let input_files = tracer.watch(child)?;
          let mut output = String::new();
          BufReader::new(receiver).read_line(&mut output)?;
          let out = output.trim();
          if out.is_empty() {
            return Err(anyhow::format_err!("Error while evaluating expression"));
          };
          db::insert_evaluation_output(&file_attribute, input_files.into_iter().collect(), &out)?;
          Ok(output)
        }
      }
    }
  }
}
