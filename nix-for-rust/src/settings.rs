use std::collections::HashMap;
use std::ffi::CString;
use anyhow::Result;
use nix::sys::resource::{Resource, setrlimit, getrlimit};

use crate::eval::NixEvalState;
use crate::bindings::{libexpr_init, libstore_init_no_load_config, setting_get, setting_set};
use crate::store::{NixContext, NixStore};
use crate::utils::{callback_get_result_string, callback_get_result_string_data};

#[derive(Clone)]
pub struct NixSettings {
  pub settings: HashMap<String, String>,
  pub store_params: HashMap<String, String>,
  pub lookup_path: Vec<String>,
  pub stack_size: u64
}

fn set_stack_size(new_max: u64) -> nix::Result<()> {
  let (_soft_limit, hard_limit) = getrlimit(Resource::RLIMIT_STACK)?;
  setrlimit(Resource::RLIMIT_STACK, std::cmp::min(new_max, hard_limit), hard_limit)
}

impl Default for NixSettings {
  fn default() -> Self {
    NixSettings {
      stack_size: 64 * 1024 * 1024,  // default stack size to 64MB
      settings: HashMap::default(),
      store_params: HashMap::default(),
      lookup_path: Vec::default()
    }
  }
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

  pub fn with_stack_size(mut self, size: u64) -> Self {
    self.stack_size = size;
    self
  }

  pub fn with_store(self, store_path: &str) -> Result<NixEvalState> {
    set_stack_size(self.stack_size)?;
    
    let ctx = NixContext::default();
    unsafe {
      libstore_init_no_load_config(ctx.ptr());
      ctx.check_call().expect("Couldn't initialize libstore");
      libexpr_init(ctx.ptr());
      ctx.check_call().expect("Couldn't initialize libexpr");
    }
    let mut settings: HashMap<CString, CString> = self.settings
      .iter()
      .map(|(key, val)| Ok((CString::new(key.as_str())?, CString::new(val.as_str())?)))
      .collect::<Result<_>>()?;
    for (key, val) in settings.iter_mut() {
      let mut old_val: Result<String> = Err(anyhow::format_err!(""));
      unsafe {
        setting_get(ctx.ptr(), key.as_ptr(), Some(callback_get_result_string), callback_get_result_string_data(&mut old_val));
        setting_set(ctx.ptr(), key.as_ptr(), val.as_ptr());
      };
      ctx.check_call()?;
      *val = CString::new(old_val.unwrap_or("".to_string()))?;
    }
    let store = NixStore::new(ctx.clone(), store_path, self.store_params.clone())?;
    let state = NixEvalState::new(store, self)?;
    // we need to unset the keys, in order for them to not leak
    // as the `setting_set` affects the globalConfig
    for (key, old_val) in settings.iter() {
      unsafe {
        setting_set(ctx.ptr(), key.as_ptr(), old_val.as_ptr());
        // ctx.check_call()?;
      }
    }
    Ok(state)
  }
}
