use std::ffi::{c_void, OsString};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use home::home_dir;
use anyhow::{Context, Result};
use libseccomp::{ScmpAction, ScmpArch, ScmpFilterContext, ScmpSyscall};
use nix::sys::signal::{self, Signal};
use nix::sys::wait::{wait, waitpid, WaitStatus};
use nix::sys::ptrace::{self, AddressType, Options};
use nix::unistd::{fork, ForkResult, Pid};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug)]
enum FileAccess {
  OpenFile { path: PathBuf, out_fd: u64 },
  FileRead { fd : u64 },
  ListDir { fd: u64 }
}

fn wait_till_syscall_exit(pid: Pid) -> Result<()> {
  ptrace::syscall(pid, None).context("when executing a syscall")?;
  waitpid(pid, None).context("while waiting for syscall exit")?;
  Ok(())
}

impl FileAccess {
  fn from_syscall(pid: Pid) -> Result<Self> {
    let regs = ptrace::getregs(pid)?;
    let syscall = regs.orig_rax;
    match syscall {
      0 => {
        let regs = ptrace::getregs(pid)?;
        Ok(FileAccess::FileRead { fd: regs.rdi })
      }
      2 => { // open
        let path = read_path_from_register(pid, regs.rdi as *mut c_void);
        wait_till_syscall_exit(pid)?;
        let regs = ptrace::getregs(pid)?;
        Ok(FileAccess::OpenFile { path, out_fd: regs.rax })
      },
      257 => { // openat
        let path = read_path_from_register(pid, regs.rsi as *mut c_void);
        wait_till_syscall_exit(pid)?;
        let regs = ptrace::getregs(pid)?;
        Ok(FileAccess::OpenFile { path, out_fd: regs.rax })
      }
      78 | 217 => { // getdents, getdents64
        let regs = ptrace::getregs(pid)?;
        wait_till_syscall_exit(pid)?;
        Ok(FileAccess::ListDir { fd: regs.rdi })
      }
      _ => {
        unreachable!("unknown syscall {syscall}")
      }
    }
  }
}

pub struct FileTracer {
  read_files: FxHashSet<PathBuf>,
  file_descriptors: FxHashMap<u64, PathBuf>,
  home_cache_dir: Option<PathBuf>
}

impl FileTracer {
  pub fn new() -> Self {
    FileTracer {
      read_files: FxHashSet::default(),
      file_descriptors: FxHashMap::default(),
      home_cache_dir: home_dir().map(|mut p| {
        p.push(".cache");
        p
      })
    }
  }

  pub fn watch_call<F: FnOnce() -> Result<String>>(self, f: F) -> Result<(String, FxHashSet<PathBuf>)> {
    let (mut sender, receiver) = interprocess::unnamed_pipe::pipe()?;
    match unsafe { fork()? } {
      ForkResult::Child => {
        setup_seccomp().unwrap();
        ptrace::traceme().unwrap();
        signal::raise(signal::Signal::SIGSTOP).unwrap();

        let string = f();
        match string {
          Ok(p) => {
            sender.write_all(p.as_bytes()).unwrap();
          },
          Err(e) => {
            eprintln!("{e}");
          },
        }
        sender.write(b"\n").unwrap();
        std::process::exit(0);
      }
      ForkResult::Parent { child } => {
        let input_files = self.watch(child)?;
        let mut output = String::new();
        BufReader::new(receiver).read_line(&mut output)?;
        let out = output.trim();
        if out.is_empty() {
          return Err(anyhow::format_err!("Error while evaluating expression"));
        };
        Ok((out.to_string(), input_files))
      }
    }
  }

  fn should_track_file(&self, path: &Path) -> bool {
    // /nix/store -> immutable, useless to track
    // /nix/var -> ephemeral, does not change results of evaluation.
    let is_nix = path.starts_with("/nix");
    let is_git_cache = self.home_cache_dir.as_ref().map(|h| path.starts_with(h)).unwrap_or(true);
    let is_in_proc = path.starts_with("/proc"); 
    !is_nix && !is_in_proc && !is_git_cache
  }

  fn handle_access(&mut self, access: FileAccess) -> Result<()> {
    match access {
      FileAccess::OpenFile { path, out_fd: fd } => {
        self.file_descriptors.insert(fd, path);
      }
      FileAccess::ListDir { fd } | FileAccess::FileRead { fd } => {
        let path = self.file_descriptors.get(&fd)
          .ok_or_else(|| anyhow::format_err!("unknown file descriptor '{fd}'"))?;
        if self.should_track_file(path) {
          self.read_files.insert(path.clone());
        }
      }
    };
    Ok(())
  }

  pub(crate) fn watch(mut self, child: Pid) -> Result<FxHashSet<PathBuf>> {
    wait().context("while attaching to child")?;
    ptrace::setoptions(child, Options::PTRACE_O_TRACESECCOMP | Options::PTRACE_O_TRACESYSGOOD).context("setting ptrace options")?;
    loop {
      let status = wait().context("while waiting for syscall entry")?;
      match status {
        WaitStatus::Exited(_pid, _) => {
          break
        },
        WaitStatus::Stopped(pid, sig @ (Signal::SIGCHLD | Signal::SIGWINCH)) => {
          ptrace::cont(pid, Some(sig)).context("while resuming from a signal")?;
        }
        WaitStatus::Signaled(_pid, _signal, _) => {
          anyhow::bail!("Evaluation process was killed.");
        },
        WaitStatus::Stopped(pid, Signal::SIGTRAP) |
        WaitStatus::PtraceEvent(pid, Signal::SIGTRAP, _)=> {
          // pre-syscall execution
          let access = FileAccess::from_syscall(pid)
            .context("while parsing syscall entry")?;
          // post-syscall execution 
          self.handle_access(access)?;
          ptrace::cont(pid, None).context("while restarting from a syscall stop;")?;
        }
        other => {
          unreachable!("Wait status '{other:?}' should never happen.")
        }
      }
    };
    Ok(self.read_files)
  }

}

fn read_path_from_register(pid: Pid, address: AddressType) -> PathBuf {
  let mut bytes = Vec::new();
  // Move 8 bytes up each time for next read.
  let mut count = 0;
  'done: loop {
    let address = address.wrapping_add(count);

    let res: i64 = match ptrace::read(pid, address) {
      Ok(c_long) => c_long,
      Err(_) => break 'done,
    };

    let bits = res.to_le_bytes();

    if let Some(null_pos) = bits.iter().position(|&c| c == b'\0') {
      bytes.extend_from_slice(&bits[..null_pos]);
      break 'done
    } else {
      bytes.extend_from_slice(&bits);
    }
    
    count += size_of::<i64>();
  }
  PathBuf::from(OsString::from_vec(bytes))
}



fn setup_seccomp() -> Result<()> {
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
