use crate::bindings::{alloc_value, eval_state_build, eval_state_builder, eval_state_builder_free, eval_state_builder_load, eval_state_builder_new, eval_state_builder_set_lookup_path, expr_eval_from_string, libexpr_init, state_create, state_free, value_decref, value_incref, EvalState, Value};
use crate::settings::NixSettings;
use crate::store::{NixContext, NixStore};
use crate::term::{NixEvalError, NixTerm, ToNix};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::ffi::{c_char, CString};
use anyhow::Result;

pub struct RawValue<'state> {
  pub _state: &'state NixEvalState,
  pub value: NonNull<Value>
}

pub struct ValueWrapper(pub NonNull<Value>);

impl<'state> RawValue<'state> {
  pub fn empty(state: &'state NixEvalState) -> Self {
    let value = NixContext::non_null(|ctx| unsafe {
      alloc_value(ctx.ptr(), state.state_ptr())
    }).expect("alloc_value should never return null");
    RawValue {
      _state: state,
      value
    }
  }
}

impl<'state> Clone for RawValue<'state> {
  fn clone(&self) -> Self {
    NixContext::checking(|ctx| unsafe {
      value_incref(ctx.ptr(), self.value.as_ptr())
    }).expect("error while increasing reference count");
    RawValue { _state: &self._state, value: self.value }
  }
}

pub struct NixEvalState {
  pub store: NixStore,
  pub settings: NixSettings,
  pub _eval_state: NonNull<EvalState>
}

impl NixEvalState {

  pub fn state_ptr(&self) -> *mut EvalState {
    self._eval_state.as_ptr()
  }
  
  pub fn eval_string<'state>(&'state self, expr: &str, cwd: PathBuf) -> Result<NixTerm<'state>> {
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

  pub fn eval_file<'state, P: AsRef<std::path::Path>>(&'state self, file: P) -> Result<NixTerm<'state>> {
    let contents = std::fs::read_to_string(&file)?;
    let realpath = std::fs::canonicalize(file)?;
    let cwd = if realpath.is_dir() {
      realpath
    } else {
      realpath.parent().map(|p| p.to_path_buf()).unwrap_or(realpath)
    };
    self.eval_string(&contents, cwd)
  }

  pub fn builtins<'state>(&'state self) -> Result<NixTerm<'state>> {
    self.eval_string("builtins", std::env::current_dir()?)
  }
  
}

pub struct NixEvalStateBuilder {
  pub(crate) ptr: NonNull<eval_state_builder>
}

impl NixEvalStateBuilder {
  pub fn new(store: &NixStore) -> Result<Self> {
    let ptr = NixContext::non_null(|ctx| unsafe {
        eval_state_builder_new(ctx.ptr(), store.store_ptr())
      })?;
    Ok(NixEvalStateBuilder { ptr })
  }

  pub fn load_settings(&mut self) -> Result<()> {
    NixContext::checking(|ctx| unsafe {
      eval_state_builder_load(ctx.ptr(), self.ptr.as_ptr());
    })
  }

  pub fn set_lookup_path(&mut self, lookup_path: &[String]) -> Result<()> {
    let mut lookup_path: Vec<CString> = lookup_path
      .iter()
      .map(|path| { Ok(CString::new(path.clone())?)})
      .collect::<Result<_>>()?;
    let mut lookup_path: Vec<*const c_char> = lookup_path
      .iter_mut()
      .map(|p| p.as_ptr())
      .chain(std::iter::once(std::ptr::null()))
      .collect();
    NixContext::checking(|ctx| unsafe {
        eval_state_builder_set_lookup_path(ctx.ptr(), self.ptr.as_ptr(), lookup_path.as_mut_ptr());
      })
  }

  pub fn build(self) -> Result<NonNull<EvalState>> {
    NixContext::checking(|ctx| unsafe {
      libexpr_init(ctx.ptr());
    })?;
    let ptr = NixContext::non_null(|ctx| unsafe {
      eval_state_build(ctx.ptr(), self.ptr.as_ptr())
    })?;
    Ok(ptr)
  }
}

impl Drop for NixEvalStateBuilder {
  fn drop(&mut self) {
    unsafe {
      eval_state_builder_free(self.ptr.as_ptr());
    }
  }
}

impl Drop for NixEvalState {
  fn drop(&mut self) {
    unsafe {
      state_free(self.state_ptr());
    }
  }
}

impl<'state> Drop for RawValue<'state> {
  fn drop(&mut self) {
    NixContext::checking(|ctx| unsafe {
      value_decref(ctx.ptr(), self.value.as_ptr());
    }).expect("error while freeing a RawValue")
  }
}
