use std::ffi::OsString;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use clap::Parser;
use nix_for_rust::{eval::NixEvalState, settings::NixSettings};
use anyhow::Result;
use nix_for_rust::derivation::{Derivation, InputDrv};
use std::process::Command;
use tempdir::TempDir;

#[derive(Parser)]
struct Args {
  file: Option<PathBuf>,
}

static KEEP_VARS: [&'static str; 13] = [
  "HOME",
  "XDG_RUNTIME_DIR",
  "USER",
  "DISPLAY",
  "BASHOPTS",
  "SHELLOPTS",
  "WAYLAND_DISPLAY",
  "WAYLAND_SOCKET",
  "TERM",
  "IN_NIX_SHELL",
  "TZ",
  "UID",
  "LOCALE_ARCHIVE"
];

fn maybe_default_file() -> Option<PathBuf> {
  let cwd = std::env::current_dir().ok()?;
  let default = cwd.join("default.nix");
  if default.exists() {
    return Some(default);
  }
  let shell = cwd.join("shell.nix");
  if shell.exists() {
    return Some(shell);
  }
  None
}

fn make_rcfile() -> Result<(PathBuf, PathBuf)> {
  let dir = TempDir::new("bash")?.into_path();
  let filepath = dir.join("rc");
  let mut file = std::fs::OpenOptions::new()
    .create(true)
    .write(true)
    .open(&filepath)?;
  writeln!(file, "dontAddDisableDepTrack=1;")?;
  writeln!(file, "set -x;")?;
  writeln!(file, "[ -e $stdenv/setup ] && source $stdenv/setup;")?;
  writeln!(file, "set +e;")?;
  writeln!(file, "eval \"${{shellHook:-}}\"")?;
  Ok((dir, filepath))
}

fn get_bash_interactive() -> Result<String> {
  Ok("/nix/store/c481fhrvslr8nmhhlzdab3k7bpnhb46a-bash-interactive-5.2p26/bin/bash".to_string())
}

fn keep_vars() -> impl Iterator<Item=(&'static str, OsString)> {
  KEEP_VARS
    .into_iter()
    .filter_map(|s| {
      std::env::var_os(s).map(|v| (s, v))
    })
}

fn ensure_inputs_built(nix: &NixEvalState, drv: &Derivation) -> Result<()> {
  for (name, input) in drv.input_drvs.iter() {
    if let InputDrv::Paths(_) = input {
      println!("building {name}");
      let p = nix.store.parse_path(name)?;
      nix.store.build(&p)?;
    } else {
      panic!()
    }
  }
  Ok(())
}

fn main() -> Result<()> {
  let args = Args::parse();
  let mut nix = NixSettings::default()
    .with_default_store()?;
  let file = args.file
    .or_else(maybe_default_file)
    .ok_or_else(|| anyhow::format_err!("Couldn't find default file."))?;
  let drv = nix.eval_attr_from_file(&file, ["drvPath"])?;
  let drv = nix.store.parse_derivation(&drv)?;
  ensure_inputs_built(&nix, &drv)?;
  println!("All inputs built successfully");
  let (tmp_dir, rcfile) = make_rcfile()?;
  Command::new(get_bash_interactive()?)
    .arg("--rcfile").arg(rcfile)
    .env("IN_NIX_SHELL", "pure")
    .env("NIX_BUILD_TOP", &tmp_dir)
    .envs(&drv.env)
    .exec();
  unreachable!("Exec bash failed.");
}
