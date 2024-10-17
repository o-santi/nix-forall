use crate::bindings::{alloc_value, expr_eval_from_string, gc_decref, gc_incref, state_create, state_free, err, EvalState, Value}; 
use crate::error::handle_nix_error;
use crate::store::{NixContext, NixStore};
use crate::term::{NixEvalError, NixTerm, ToNix};
use std::os::raw::c_void;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::ffi::{c_char, CString};
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
  
  pub fn new<S:std::fmt::Display, P:IntoIterator<Item=(S, S)>>(store: NixStore, lookup_paths: P) -> Result<Self> {
    let ctx = NixContext::default();
    let mut lookup_path: Vec<CString> = lookup_paths
      .into_iter()
      .map(|(key, val)| {
        let string = format!("{key}={val}");
        Ok(CString::new(string)?)
      })
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
    Ok(NixEvalState { store, _eval_state: Rc::new(StateWrapper(state)) })
  }

  pub fn eval_string(&mut self, expr: &str, cwd: PathBuf) -> Result<NixTerm> {
    let cstr = CString::new(expr)?;
    let current_dir = cwd.as_path()
      .to_str()
      .ok_or_else(|| anyhow::format_err!("Cannot get current directory"))?
      .to_owned();
    let current_dir = CString::new(current_dir)?;
    let val = RawValue::empty(self.clone());
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

  pub fn eval_file(&mut self, file: &std::path::Path) -> Result<NixTerm> {
    let contents = std::fs::read_to_string(&file)?;
    let realpath = std::fs::canonicalize(file)?;
    let cwd = if realpath.is_dir() {
      realpath
    } else {
      realpath.parent().map(|p| p.to_path_buf()).unwrap_or(realpath)
    };
    self.eval_string(&contents, cwd)
  }

  pub fn eval_flake(&mut self, flake_path: &str) -> Result<NixTerm> {
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
