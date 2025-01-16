use crate::bindings::{alloc_value, copy_value, expr_eval_from_string, gc_decref, state_create, state_free, EvalState, Value};
use crate::settings::NixSettings;
use crate::store::{NixContext, NixStore};
use crate::term::{NixEvalError, NixTerm, ToNix};
use std::os::raw::c_void;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::ffi::{c_char, CString};
use anyhow::Result;

pub struct RawValue<'state> {           
  pub _state: &'state NixEvalState,
  pub value: NonNull<Value>
}

impl<'state> RawValue<'state> {
  pub fn empty(state: &'state NixEvalState) -> Self {
    let value = unsafe {
      alloc_value(state.store.ctx._ctx.as_ptr(), state.state_ptr())
    };
    let value: NonNull<Value> = match NonNull::new(value) {
      Some(v) => v,
      None => panic!("alloc_value returned null"),
    };
    RawValue {
      _state: state,
      value
    }
  }
}

pub struct NixEvalState {
  pub store: NixStore,
  pub settings: NixSettings,
  pub _eval_state: StateWrapper
}

pub struct StateWrapper(pub NonNull<EvalState>);

impl NixEvalState {

  pub fn state_ptr(&self) -> *mut EvalState {
    self._eval_state.0.as_ptr()
  }
  
  pub fn new(store: NixStore, settings: NixSettings) -> Result<Self> {
    let ctx = NixContext::default();
    let mut lookup_path: Vec<CString> = settings.lookup_path
      .iter()
      .map(|path| { Ok(CString::new(path.clone())?)})
      .collect::<Result<_>>()?;
    let mut lookup_path: Vec<*const c_char> = lookup_path
      .iter_mut()
      .map(|p| p.as_ptr())
      .chain(std::iter::once(std::ptr::null()))
      .collect();
    let state = unsafe {
      state_create(ctx.ptr(), lookup_path.as_mut_ptr(), store.store_ptr())
    };
    ctx.check_call()?;
    let state = NonNull::new(state).ok_or(anyhow::format_err!("state_create return null pointer"))?;
    Ok(NixEvalState { store, settings, _eval_state: StateWrapper(state) })
  }

  pub fn eval_string<'slf>(&'slf mut self, expr: &str, cwd: PathBuf) -> Result<NixTerm<'slf>> {
    let cstr = CString::new(expr)?;
    let current_dir = cwd.as_path()
      .to_str()
      .ok_or_else(|| anyhow::format_err!("Cannot get current directory"))?
      .to_owned();
    let current_dir = CString::new(current_dir)?;
    let val = RawValue::empty(self);
    unsafe {
      expr_eval_from_string(
        self.store.ctx.ptr(),
        self.state_ptr(),
        cstr.as_ptr(),
        current_dir.as_ptr(),
        val.value.as_ptr());
    }
    self.store.ctx.check_call()?;
    val.to_nix(self).map_err(|err: NixEvalError| anyhow::anyhow!(err))
  }

  pub fn eval_file<'slf, P: AsRef<std::path::Path>>(&'slf mut self, file: P) -> Result<NixTerm<'slf>> {
    let contents = std::fs::read_to_string(&file)?;
    let realpath = std::fs::canonicalize(file)?;
    let cwd = if realpath.is_dir() {
      realpath
    } else {
      realpath.parent().map(|p| p.to_path_buf()).unwrap_or(realpath)
    };
    self.eval_string(&contents, cwd)
  }

  pub fn eval_flake<'slf>(&'slf mut self, flake_path: &str) -> Result<NixTerm<'slf>> {
    let has_flakes = |attr| self.settings.get_setting(attr).map(|s| s.contains("flakes")).unwrap_or(false);
    let is_flake_enabled = has_flakes("extra-experimental-features") || has_flakes("experimental-features");
    if !is_flake_enabled {
      anyhow::bail!("Nix Evaluator does not have flakes enabled. Please create it using `extra-experimental-features=flakes`.")
    }
    let (flake_path, cwd) = {
      let path= std::path::Path::new(flake_path);
      if path.try_exists()? {
        let canonical_path = std::fs::canonicalize(path)?;
        (canonical_path
          .clone()
          .into_os_string()
          .into_string()
          .map_err(|_| anyhow::format_err!("Cannot convert flake path to unicode string"))?, canonical_path)
      } else {
        (flake_path.to_string(), std::env::current_dir()?)
      }
    };
    let contents = format!("builtins.getFlake \"{flake_path}\"");
    self.eval_string(&contents, cwd)
  }
  
}

impl Drop for StateWrapper {
  fn drop(&mut self) {
    unsafe {
      state_free(self.0.as_ptr());
    }
  }
}

impl<'state> Drop for RawValue<'state> {
  fn drop(&mut self) {
    unsafe {
      gc_decref(self._state.store.ctx._ctx.as_ptr(), self.value.as_ptr() as *mut c_void);
    }
  }
}


impl<'state> Clone for RawValue<'state> {
  fn clone(&self) -> Self {
    let ctx = self._state.store.ctx.ptr();
    let new_val = RawValue::empty(self._state);
    unsafe {
      copy_value(ctx, new_val.value.as_ptr(), self.value.as_ptr());
    }
    new_val
  }
}
