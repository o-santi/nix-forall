use std::collections::HashMap;
use std::rc::Rc;
use anyhow::Result;
use nix::sys::resource::{Resource, setrlimit, getrlimit};

use crate::eval::{NixEvalState, NixEvalStateBuilder};
use crate::bindings::{flake_settings_add_to_eval_state_builder, libexpr_init, libstore_init_no_load_config, setting_set};
use crate::flakes::FlakeSettings;
use crate::store::{NixContext, NixStore};

#[derive(Clone)]
pub struct NixSettings {
  pub settings: HashMap<String, String>,
  pub store_params: HashMap<String, String>,
  pub lookup_path: Vec<String>,
  pub flake_settings: Option<FlakeSettings>,
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
      lookup_path: Vec::default(),
      flake_settings: None,
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
  
  pub fn with_flakes(mut self, settings: FlakeSettings) -> Self {
    self.flake_settings = Some(settings);
    self
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
    
    let store = NixStore::new(ctx.clone(), store_path, self.store_params.clone())?;
    let mut state_builder = NixEvalStateBuilder::new(&store)?;

    if let Some(flake_settings) = &self.flake_settings {
      unsafe {
        flake_settings_add_to_eval_state_builder(ctx.ptr(), flake_settings.settings_ptr.as_ptr(), state_builder.ptr.as_ptr());
      }
      ctx.check_call()?;
    }
    
    for (key, val) in self.settings.iter() {
      unsafe {
        setting_set(ctx.ptr(), key.as_ptr() as *const i8, val.as_ptr() as *const i8);
      };
      ctx.check_call()?;
    }

    state_builder.load_settings()?;

    state_builder.set_lookup_path(&self.lookup_path)?;

    Ok(NixEvalState {
      store,
      settings: self,
      _eval_state: Rc::new(state_builder.build()?)
    })
  }
}
