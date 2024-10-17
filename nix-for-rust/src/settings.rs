use std::collections::HashMap;
use std::ffi::CString;
use std::str::FromStr;
use anyhow::Result;

use crate::eval::NixEvalState;
use crate::bindings::{libexpr_init, setting_set};
use crate::store::{NixContext, NixStore};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct NixSettings {
  settings: HashMap<String, String>,
  store_params: HashMap<String, String>,
  lookup_path: HashMap<String, String>
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
      ..Default::default()
    }
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

  pub fn with_lookup_path(mut self, key: &str, val: &str) -> Self {
    self.lookup_path.insert(key.to_string(), val.to_string());
    self
  }

  pub fn with_store(self, store_path: &str) -> Result<NixEvalState> {
    let ctx = NixContext::default();
    for (key, val) in self.settings {
      let key = CString::new(key)?;
      let val = CString::new(val)?;
      unsafe {
        setting_set(ctx.ptr(), key.as_ptr(), val.as_ptr());
      };
      ctx.check_call()?;
    }
    unsafe {
      libexpr_init(ctx.ptr());
      ctx.check_call().expect("Couldn't initialize libexpr");
    };
    let store = NixStore::new(ctx, store_path, self.store_params)?;
    NixEvalState::new(store, self.lookup_path)
  }
}
