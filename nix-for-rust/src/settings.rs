use std::collections::HashMap;
use std::ffi::CString;
use std::str::FromStr;
use anyhow::Result;

use crate::error::handle_nix_error;
use crate::eval::NixEvalState;
use crate::bindings::setting_set;
use crate::store::{NixContext, NixStore};
use std::path::{Path, PathBuf};

pub struct NixSettings {
  settings: HashMap<String, String>,
  store_params: HashMap<String, String>,
}

fn get_config_home() -> Result<PathBuf> {
  if let Ok(p) = std::env::var("NIX_CONFIG_HOME") {
    Ok(PathBuf::from_str(&p)?)
  } else {
    if let Ok(p) = std::env::var("XDG_CONFIG_HOME") {
      Ok(Path::new(&p).join("nix"))
    } else {
      home::home_dir()
        .map(|p| Ok(p.join(".config").join("nix")))
        .unwrap_or(Err(anyhow::format_err!("Cannot find user home directory.")))
    }
  }
}

fn get_config_files() -> Result<Vec<PathBuf>> {
  let config_home = get_config_home()?;
  let config_dirs_env = std::env::var("XDG_CONFIG_DIRS").unwrap_or("/etc/xdg".to_string());
  let dirs = std::iter::once(config_home)
    .chain(config_dirs_env
      .split(":")
      .map(|p| Path::new(p).join("nix")))
    .map(|p| p.join("nix.conf"))
    .filter(|p| p.exists())
    .collect();
  Ok(dirs)
}

impl NixSettings {

  pub fn empty() -> Self {
    NixSettings {
      settings: HashMap::new(),
      store_params: HashMap::new()
    }
  }

  pub fn default_conf() -> Result<Self> {
    let mut settings = NixSettings::empty();
    settings = settings.load_conf_file(Path::new("/etc/nix/nix.conf"))?;
    for conf in get_config_files()? {
      settings = settings.load_conf_file(&conf)?;
    }
    // println!("{:?}", settings.settings);
    Ok(settings)
  }

  pub fn load_conf_file(mut self, file: &Path) -> Result<Self> {
    let contents = std::fs::read_to_string(file)?;
    for line in contents.lines() {
      let line = line.split_once("#")
        .map(|(line, _comment)| line)
        .unwrap_or(line);
      if line.trim().is_empty() {
        continue
      }
      let Some((key, rest)) = line.split_once(" ") else {
        anyhow::bail!("Syntax error in '{file:?}'")
      };
      let (key, rest) = (key.trim(), rest.trim());
      match key {
        "include" | "!include" => {
          let include_file = Path::new(rest);
          if !include_file.exists() && key != "!include" {
            anyhow::bail!("File '{rest}' included in '{file:?}' doesn't exist");
          }
          self = self.load_conf_file(include_file)?;
        }
        _ => {
          let Some((_equals, val)) = line.split_once("=") else {
            anyhow::bail!("Syntax error in '{file:?}'")
          };
          self = self.with_setting(key, val.trim());
        }
      }
    }
    Ok(self)
  }

  pub fn with_setting(mut self, key: &str, val: &str) -> Self {
    self.settings.insert(key.to_string(), val.to_string());
    self
  }

  pub fn with_store_param(mut self, key: &str, val: &str) -> Self {
    self.store_params.insert(key.to_string(), val.to_string());
    self
  }

  pub fn with_default_store(self) -> Result<NixEvalState> {
    self.with_store("auto")
  }

  pub fn with_store(self, store_path: &str) -> Result<NixEvalState> {
    let ctx = NixContext::default();
    for (key, val) in self.settings {
      let key = CString::new(key)?;
      let val = CString::new(val)?;
      unsafe {
        setting_set(ctx.ptr(), key.as_ptr(), val.as_ptr());
      };
      // ctx.check_call()?;
    }
    let store = NixStore::new(ctx, store_path, self.store_params)?;
    Ok(NixEvalState::new(store))
  }
}
