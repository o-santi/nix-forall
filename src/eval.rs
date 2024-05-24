use crate::bindings::{nix_alloc_value, nix_expr_eval_from_string, nix_gc_decref, nix_gc_incref, nix_state_create, nix_state_free, EvalState, Value, NIX_OK};
use crate::error::handle_nix_error;
use crate::store::{NixContext, NixStore};
use crate::term::{NixEvalError, NixTerm};
use std::ptr::NonNull;
use std::ffi::CString;
use anyhow::Result;
use std::rc::Rc;

pub struct RawValue {           
  pub(crate) _state: NixEvalState,
  pub(crate) value: Rc<ValueWrapper>
}

pub(crate) struct ValueWrapper(NonNull<Value>);

impl RawValue {

  pub fn ptr(&self) -> *mut Value {
    self.value.0.as_ptr()
  }

  pub(crate) fn from_raw(rawvalue: NonNull<Value>, state: NixEvalState) -> Self {
    RawValue {
      _state: state,
      value: Rc::new(ValueWrapper(rawvalue))
    }
  }
  
  pub fn empty(state: NixEvalState) -> Self {
    let value = unsafe {
      nix_alloc_value(state.store.ctx.ptr(), state.state_ptr())
    };
    state.store.ctx.check_call().expect("nix_alloc_value failed");
    let value: NonNull<Value> = match NonNull::new(value) {
      Some(v) => v,
      None => panic!("nix_alloc_value returned null"),
    };
    RawValue {
      _state: state,
      value: Rc::new(ValueWrapper(value))
    }
  }
}

#[derive(Clone)]
pub struct NixEvalState {
  pub(crate) store: NixStore,
  pub(crate) _eval_state: Rc<StateWrapper>
}

pub(crate) struct StateWrapper(pub(crate) NonNull<EvalState>);

impl NixEvalState {

  pub fn state_ptr(&self) -> *mut EvalState {
    self._eval_state.0.as_ptr()
  }
  
  pub fn new(store: NixStore) -> Self {
    let ctx = NixContext::default();
    let state = unsafe {
      let lookup_path = std::ptr::null_mut();
      nix_state_create(ctx.ptr(), lookup_path, store.store_ptr())
    };
    let state = match NonNull::new(state) {
      Some(n) => n,
      None => panic!("nix_state_create returned null"),
    };
    NixEvalState { store, _eval_state: Rc::new(StateWrapper(state)) }
  }

  pub fn eval_from_string(&mut self, expr: &str) -> Result<NixTerm> {
    let cstr = CString::new(expr)?;
    let current_dir = std::env::current_dir()?.as_path()
      .to_str()
      .ok_or_else(|| anyhow::format_err!("Cannot get current directory"))?
      .to_owned();
    let current_dir = CString::new(current_dir)?;
    let val = RawValue::empty(self.clone());
    unsafe {
      let result = nix_expr_eval_from_string(
        self.store.ctx.ptr(),
        self.state_ptr(),
        cstr.as_ptr(),
        current_dir.as_ptr(),
        val.ptr());
      if result as u32 == NIX_OK {
        val.try_into().map_err(|err: NixEvalError| anyhow::anyhow!(err))
      } else {
        anyhow::bail!(handle_nix_error(result, &self.store.ctx))
      }
    }
  }
}

impl Drop for StateWrapper {
  fn drop(&mut self) {
    unsafe {
      nix_state_free(self.0.as_ptr());
    }
  }
}

impl Clone for RawValue {
  fn clone(&self) -> Self {
    unsafe {
      nix_gc_incref(self._state.store.ctx.ptr(), self.ptr());
    }
    RawValue { _state: self._state.clone(), value: self.value.clone()  }
  }
}
