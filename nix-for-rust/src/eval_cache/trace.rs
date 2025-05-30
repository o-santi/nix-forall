use std::collections::{HashSet, HashMap};
use std::ffi::{c_void, OsString};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use home::home_dir;
use anyhow::{Context, Result};
use nix::sys::signal::Signal;
use nix::sys::wait::{wait, waitpid, WaitStatus};
use nix::sys::ptrace::{self, AddressType, Options};
use nix::unistd::Pid;

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
  fn from_syscall(pid: Pid) -> Result<Option<Self>> {
    let regs = ptrace::sysgetregs(pid)?;
    let syscall = regs.orig_rax;
    match syscall {
      0 => {
        let regs = ptrace::sysgetregs(pid)?;
        Ok(Some(FileAccess::FileRead { fd: regs.rdi }))
      }
      2 => { // open
        let path = read_path_from_register(pid, regs.rdi as *mut c_void);
        wait_till_syscall_exit(pid)?;
        let regs = ptrace::sysgetregs(pid)?;
        Ok(Some(FileAccess::OpenFile { path, out_fd: regs.rax }))
      },
      257 => { // openat
        let path = read_path_from_register(pid, regs.rsi as *mut c_void);
        wait_till_syscall_exit(pid)?;
        let regs = ptrace::sysgetregs(pid)?;
        Ok(Some(FileAccess::OpenFile { path, out_fd: regs.rax }))
      }
      78 | 217 => { // getdents, getdents64
        let regs = ptrace::sysgetregs(pid)?;
        wait_till_syscall_exit(pid)?;
        Ok(Some(FileAccess::ListDir { fd: regs.rdi }))
      }
      _ => {
        wait_till_syscall_exit(pid)?;
        Ok(None)
      }
    }
  }
}

pub struct FileTracer {
  read_files: HashSet<PathBuf>,
  file_descriptors: HashMap<u64, PathBuf>,
  home_cache_dir: Option<PathBuf>
}

impl FileTracer {
  pub fn new() -> Self {
    FileTracer {
      read_files: HashSet::new(),
      file_descriptors: HashMap::new(),
      home_cache_dir: home_dir().map(|mut p| {
        p.push(".cache");
        p
      })
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

  fn handle_access(&mut self, access: FileAccess) {
    match access {
      FileAccess::OpenFile { path, out_fd: fd } => {
        self.file_descriptors.insert(fd, path);
      }
      FileAccess::ListDir { fd } | FileAccess::FileRead { fd } => {
        let path = self.file_descriptors.get(&fd).expect(&format!("unknown file descriptor '{fd}'"));
        if self.should_track_file(path) {
          self.read_files.insert(path.clone());
        }
      }
    }
  }

  pub(crate) fn watch(mut self, child: Pid) -> Result<HashSet<PathBuf>> {
    wait().context("while attaching to child")?;
    ptrace::setoptions(child, Options::PTRACE_O_TRACESYSGOOD).context("setting ptrace options")?;
    ptrace::syscall(child, None).context("Exception thrown when executing syscall")?;
    loop {
      let status = wait().context("while waiting for syscall entry")?;
      match status {
        WaitStatus::Exited(_pid, _) => {
          break
        },
        WaitStatus::Stopped(pid, sig @ (Signal::SIGCHLD | Signal::SIGWINCH)) => {
          ptrace::syscall(pid, Some(sig)).context("while resuming from a signal")?;
        }
        WaitStatus::Signaled(_pid, _signal, _) => {
          anyhow::bail!("Evaluation process was killed.");
        },
        WaitStatus::PtraceSyscall(pid) => {
          // pre-syscall execution
          let access = FileAccess::from_syscall(pid)
            .context("while parsing syscall entry")?;
          // post-syscall execution 
          if let Some(access) = access {
            self.handle_access(access);
          }
          // arrange for next syscall
          if let Err(_) = ptrace::syscall(pid, None) {
            break;
          };
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
