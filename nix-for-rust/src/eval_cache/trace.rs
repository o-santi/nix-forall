use std::collections::{HashSet, HashMap};
use std::ffi::{c_long, c_void};
use std::path::{Path, PathBuf};

use nix::sys::signal::Signal;
use nix::sys::wait::{wait, waitpid, WaitStatus};
use nix::sys::ptrace::{self, AddressType, Options};
use byteorder::{LittleEndian, WriteBytesExt};
use nix::unistd::Pid;

#[derive(Debug)]
enum FileAccess {
  FileOpen(PathBuf),
  DirOpen(PathBuf),
  ListDir(u64)
}

pub(crate) fn trace_accessed_files(child: Pid) -> HashSet<PathBuf> {
  let mut files = HashSet::new();
  let mut dir_fds = HashMap::new();
  wait().expect("Parent failed while waiting for child");
  ptrace::setoptions(child, Options::PTRACE_O_TRACESYSGOOD).unwrap();
  ptrace::syscall(child, None).expect("Exception thrown when executing syscall");
  loop {
    let status = wait().unwrap();
    match status {
      WaitStatus::Exited(_pid, _) => {
        break
      },
      WaitStatus::Stopped(pid, Signal::SIGCHLD) => {
        ptrace::syscall(pid, Some(Signal::SIGCHLD)).unwrap();
      }
      WaitStatus::Stopped(pid, Signal::SIGWINCH) => {
        ptrace::syscall(pid, Some(Signal::SIGWINCH)).unwrap();
      }
      WaitStatus::Signaled(_pid, _signal, _) => {
        panic!("Evaluation process was killed.");
      },
      WaitStatus::PtraceSyscall(pid) => {
        // pre-syscall execution
        let access = pre_syscall(pid);
        ptrace::syscall(pid, None).expect("Exception thrown when executing syscall");
        waitpid(pid, None).expect("Error while waiting for post-syscall");
        // post-syscall execution 
        if let Some(access) = access {
          match access {
            FileAccess::FileOpen(fa) => {
              files.insert(fa);
            }
            FileAccess::DirOpen(fa) => {
              let regs = ptrace::getregs(pid).expect("should never throw error");
              dir_fds.insert(regs.rax, fa);
            }
            FileAccess::ListDir(dir_fd) => {
              if let Some(path) = dir_fds.get(&dir_fd) {
                files.insert(path.clone());
              }
            }
          }
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
  files
}

fn pre_syscall(child: Pid) -> Option<FileAccess> {
  let regs = ptrace::getregs(child).expect("Call shouldn't fail");
  let syscall = regs.orig_rax;
  let path = match syscall {
    2 => { // open
      read_string_from_register(child, regs.rdi as *mut c_void)
    },
    78 | 217 => { // getdents, getdents64
      return Some(FileAccess::ListDir(regs.rdi));
    }
    257 => { // openat
      read_string_from_register(child, regs.rsi as *mut c_void)
    }
    _ => { return None; }
  };
  let path = Path::new(&path).to_path_buf();
  if should_track_file(&path) {
    if path.is_dir() {
      Some(FileAccess::DirOpen(path))
    } else {
      Some(FileAccess::FileOpen(path))
    }
  } else {
    None
  }
}


fn should_track_file(path: &Path) -> bool {
  let is_immutable = path.starts_with("/nix/store/");
  let is_git_cache = {
    let home = home::home_dir();
    if let Some(mut h) = home {
      h.push(".cache");
      path.starts_with(h)
    } else {
      true
    }
  };
  let is_in_proc = path.starts_with("/proc"); 
  !is_immutable && !is_git_cache && !is_in_proc
}

fn read_string_from_register(pid: Pid, address: AddressType) -> String {
  let mut string = String::new();
  // Move 8 bytes up each time for next read.
  let mut count = 0;
  let word_size = 8;

  'done: loop {
    let mut bytes: Vec<u8> = vec![];
    let address = unsafe { address.offset(count) };

    let res: c_long;

    match ptrace::read(pid, address) {
      Ok(c_long) => res = c_long,
      Err(_) => break 'done,
    }

    bytes.write_i64::<LittleEndian>(res).unwrap_or_else(|err| {
      panic!("Failed to write {} as i64 LittleEndian: {}", res, err);
    });

    for b in bytes {
      if b != 0 {
        string.push(b as char);
      } else {
        break 'done;
      }
    }
    count += word_size;
  }
  string
}
