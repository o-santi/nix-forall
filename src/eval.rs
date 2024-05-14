use crate::bindings::{nix_alloc_value, nix_expr_eval_from_string, nix_gc_decref, nix_gc_incref, nix_libexpr_init, nix_state_create, nix_state_free, EvalState, Value, NIX_OK};
use crate::error::handle_nix_error;
use crate::store::NixStore;
use std::ptr::NonNull;
use std::ffi::{c_void, CString};
use anyhow::Result;

pub struct RawValue {
  pub(crate) _state: NixEvalState,
  pub(crate) value: NonNull<Value>
}

impl RawValue {
  pub fn empty(state: NixEvalState) -> Self {
    let value = unsafe {
      nix_alloc_value(state.store.ctx._ctx.as_ptr(), state._eval_state.as_ptr())
    };
    let value: NonNull<Value> = match NonNull::new(value) {
      Some(v) => v,
      None => panic!("nix_alloc_value returned null"),
    };
    RawValue {
      _state: state,
      value
    }
  }
}

pub struct NixEvalState {
  pub(crate) store: NixStore,
  pub(crate) _eval_state: NonNull<EvalState>
}

impl NixEvalState {
  pub fn new(store: NixStore) -> Self {
    let state = unsafe {
      nix_libexpr_init(store.ctx._ctx.as_ptr());
      let lookup_path = [std::ptr::null()].as_mut_ptr();
      nix_state_create(store.ctx._ctx.as_ptr(), lookup_path, store._store.as_ptr())
    };
    let _eval_state = match NonNull::new(state) {
      Some(n) => n,
      None => panic!("nix_state_create returned null"),
    };
    NixEvalState { store, _eval_state }
  }

  pub fn eval_from_string(&mut self, expr: &str) -> Result<RawValue> {
    let cstr = CString::new(expr)?;
    let current_dir = std::env::current_dir()?.as_path()
      .to_str()
      .ok_or_else(|| anyhow::format_err!("Cannot get current directory"))?
      .to_owned();
    let current_dir = CString::new(current_dir)?;
    let val = RawValue::empty(self.clone());
    unsafe {
      let result = nix_expr_eval_from_string(
        self.store.ctx._ctx.as_ptr(),
        self._eval_state.as_ptr(),
        cstr.as_ptr(),
        current_dir.as_ptr(),
        val.value.as_ptr());
      if result as u32 == NIX_OK {
        Ok(val)
      } else {
        Err(anyhow::anyhow!(handle_nix_error(result, &self)))
      }
    }
  }
}

impl Drop for NixEvalState {
  fn drop(&mut self) {
    unsafe {
      nix_gc_decref(self.store.ctx._ctx.as_ptr(), self._eval_state.as_ptr() as *const c_void);
    }
  }
}

impl Drop for RawValue {
  fn drop(&mut self) {
    unsafe {
      nix_gc_decref(self._state.store.ctx._ctx.as_ptr(), self.value.as_ptr());
    }
  }
}



impl Clone for RawValue {
  fn clone(&self) -> Self {
    unsafe {
      nix_gc_incref(self._state.store.ctx._ctx.as_ptr(), self._state._eval_state.as_ptr() as *const c_void);
    }
    RawValue { _state: self._state.clone(), value: self.value.clone()  }
  }
}


impl Clone for NixEvalState {
  fn clone(&self) -> Self {
    unsafe {
      nix_gc_incref(self.store.ctx._ctx.as_ptr(), self._eval_state.as_ptr() as *const c_void);
    }
    NixEvalState { store: self.store.clone(), _eval_state: self._eval_state.clone() }
  }
}
