use crate::bindings::{alloc_value, expr_eval_from_string, gc_decref, gc_incref, state_create, state_free, err, EvalState, Value}; 
use crate::error::handle_nix_error;
use crate::store::{NixContext, NixStore};
use crate::term::{NixEvalError, NixTerm, ToNix};
use std::os::raw::c_void;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::ffi::CString;
use anyhow::Result;
use std::rc::Rc;

pub struct RawValue {           
  pub _state: NixEvalState,
  pub value: NonNull<Value>
}

impl RawValue {
  pub fn empty(state: NixEvalState) -> Self {
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

#[derive(Clone)]
pub struct NixEvalState {
  pub store: NixStore,
  pub _eval_state: Rc<StateWrapper>
}

pub struct StateWrapper(pub NonNull<EvalState>);

impl NixEvalState {

  pub fn state_ptr(&self) -> *mut EvalState {
    self._eval_state.0.as_ptr()
  }
  
  pub fn new(store: NixStore) -> Self {
    let ctx = NixContext::default();
    let state = unsafe {
      let lookup_path = std::ptr::null_mut();
      state_create(ctx.ptr(), lookup_path, store.store_ptr())
    };
    let state = match NonNull::new(state) {
      Some(n) => n,
      None => panic!("state_create returned null"),
    };
    NixEvalState { store, _eval_state: Rc::new(StateWrapper(state)) }
  }

  pub fn eval_from_string(&mut self, expr: &str, cwd: PathBuf) -> Result<NixTerm> {
    let cstr = CString::new(expr)?;
    let current_dir = cwd.as_path()
      .to_str()
      .ok_or_else(|| anyhow::format_err!("Cannot get current directory"))?
      .to_owned();
    let current_dir = CString::new(current_dir)?;
    let val = RawValue::empty(self.clone());
    unsafe {
      let result = expr_eval_from_string(
        self.store.ctx.ptr(),
        self.state_ptr(),
        cstr.as_ptr(),
        current_dir.as_ptr(),
        val.value.as_ptr());
      if result == err::NIX_OK {
        val.to_nix(self).map_err(|err: NixEvalError| anyhow::anyhow!(err))
      } else {
        anyhow::bail!(handle_nix_error(result, &self.store.ctx))
      }
    }
  }
}

impl Drop for StateWrapper {
  fn drop(&mut self) {
    unsafe {
      state_free(self.0.as_ptr());
    }
  }
}

impl Drop for RawValue {
  fn drop(&mut self) {
    unsafe {
      gc_decref(self._state.store.ctx._ctx.as_ptr(), self.value.as_ptr() as *mut c_void);
    }
  }
}


impl Clone for RawValue {
  fn clone(&self) -> Self {
    unsafe {
      gc_incref(self._state.store.ctx._ctx.as_ptr(), self.value.as_ptr() as *const c_void);
    }
    RawValue { _state: self._state.clone(), value: self.value.clone()  }
  }
}
