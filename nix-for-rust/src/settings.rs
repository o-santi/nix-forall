use std::collections::HashMap;
use std::ffi::CString;
use anyhow::Result;

use crate::eval::NixEvalState;
use crate::bindings::{libexpr_init, libstore_init_no_load_config, setting_set};
use crate::store::{NixContext, NixStore};

#[derive(Default, Clone)]
pub struct NixSettings {
  pub settings: HashMap<String, String>,
  pub store_params: HashMap<String, String>,
  pub lookup_path: Vec<String>
}

impl NixSettings {

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

  pub fn get_setting(&self, key: &str) -> Option<String> {
    self.settings.get(key).map(String::from)
  }

  pub fn with_store(self, store_path: &str) -> Result<NixEvalState> {
    let ctx = NixContext::default();
    let settings: Vec<(CString, CString)> = self.settings
      .iter()
      .map(|(key, val)| Ok((CString::new(key.as_str())?, CString::new(val.as_str())?)))
      .collect::<Result<_>>()?;
    for (key, val) in settings.iter() {
      unsafe {
        setting_set(ctx.ptr(), key.as_ptr(), val.as_ptr());
      };
      ctx.check_call()?;
    }
    unsafe {
      libstore_init_no_load_config(ctx.ptr());
      ctx.check_call().expect("Couldn't initialize libstore");
      libexpr_init(ctx.ptr());
      ctx.check_call().expect("Couldn't initialize libexpr");
    }
    let store = NixStore::new(ctx.clone(), store_path, self.store_params.clone())?;
    let state = NixEvalState::new(store, self);
    // we need to unset the keys, in order for them to not leak
    // as the `setting_set` affects the globalConfig
    for (key, _) in settings.iter() {
      let val = CString::new("")?;
      unsafe {
        setting_set(ctx.ptr(), key.as_ptr(), val.as_ptr());
      }
    }
    state
  }
}
