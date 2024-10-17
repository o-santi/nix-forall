use std::collections::HashMap;
use std::ffi::CString;
use anyhow::Result;

use crate::eval::NixEvalState;
use crate::bindings::{libexpr_init, libstore_init, libstore_init_no_load_config, setting_set};
use crate::store::{NixContext, NixStore};

#[derive(Default)]
pub struct NixSettings {
  load_external_config: bool,
  settings: HashMap<String, String>,
  store_params: HashMap<String, String>,
  lookup_path: Vec<String>
}

impl NixSettings {

  pub fn load_config() -> Self {
    NixSettings {
      load_external_config: true,
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

  pub fn with_lookup_path(mut self, path: &str) -> Self {
    self.lookup_path.push(path.to_string());
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
      ctx.check_call()?;
    }
    unsafe {
      if self.load_external_config {
        libstore_init(ctx.ptr());
        ctx.check_call()?;
        libexpr_init(ctx.ptr());
        ctx.check_call()?;
      } else {
        libstore_init_no_load_config(ctx.ptr());
        ctx.check_call()?;
        libexpr_init(ctx.ptr()); 
        ctx.check_call()?;
      }
    }
    ctx.check_call().expect("Couldn't initialize libexpr");
    let store = NixStore::new(ctx, store_path, self.store_params)?;
    NixEvalState::new(store, self.lookup_path)
  }
}
