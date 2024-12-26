use crate::error::{handle_nix_error, NixError};
use crate::term::NixEvalError;
use crate::utils::{callback_get_result_string, callback_get_result_string_data, read_into_hashmap};
use crate::bindings::{c_context, c_context_create, err, err_code, store_free, store_get_version, store_open, store_parse_path, store_path_free, store_path_name, store_realise, Store, StorePath};
use std::collections::HashMap;
use std::ffi::{c_void, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::ptr::{null_mut, NonNull};
use anyhow::Result;
use std::rc::Rc;

#[derive(Clone)]
pub struct NixContext {
  pub(crate) _ctx: NonNull<c_context>,
}

impl Default for NixContext {
  fn default() -> Self {
    let _ctx = unsafe { c_context_create() };
    let _ctx = match NonNull::new(_ctx) {
      Some(c) => c,
      None => panic!("c_context_create returned null")
    };
    NixContext { _ctx  }
  }

}

impl NixContext {

  pub fn ptr(&self) -> *mut c_context {
    self._ctx.as_ptr()
  }

  pub fn check_call(&self) -> std::result::Result<(), NixError> {
    let err = unsafe { err_code(self._ctx.as_ptr())};
    if err != err::NIX_OK as i32 {
      Err(handle_nix_error(err, self))
    } else {
      Ok(())
    }
  }
}

#[derive(Clone)]
pub struct NixStore {
  pub ctx: NixContext,
  pub _store: Rc<StoreWrapper>
}

#[derive(Debug)]
pub struct NixStorePath {
  pub path: PathBuf,
  _ptr: NonNull<StorePath>
}

pub struct StoreWrapper(NonNull<Store>);

impl NixStore {

  pub(crate) fn store_ptr(&self) -> *mut Store {
    self._store.0.as_ptr()
  }
  
  pub fn new<I: IntoIterator<Item=(S, S)>, S: Into<Vec<u8>>>(ctx: NixContext, uri: &str, extra_params: I) -> Result<Self> {
    let uri = CString::new(uri)?;
    let _store = {
      let params: Vec<(CString, CString)> = extra_params
        .into_iter()
        .map(|(k, v)| Ok((CString::new(k)?, CString::new(v)?)))
        .collect::<Result<_>>()?;
      let mut params: Vec<[*const c_char; 2]> = params
        .iter()
        .map(|(k, v)| [k.as_ptr(), v.as_ptr()])
        .collect();
      let mut params: Vec<*mut *const c_char> = params
        .iter_mut()
        .map(|p| p.as_mut_ptr())
        .chain(std::iter::once(null_mut()))
        .collect();
      unsafe {
        store_open(ctx._ctx.as_ptr(), uri.into_raw(), params.as_mut_ptr())
      }
    };
    let store = NonNull::new(_store).ok_or(anyhow::anyhow!("nix_store_open returned NonNull"))?;
    Ok(NixStore { ctx, _store: Rc::new(StoreWrapper(store)) })
  }
  
  pub fn version(&self) -> Result<String> {
    let mut version_string : Result<String> = Err(anyhow::anyhow!("Nix C API didn't return a string."));
    unsafe { store_get_version(self.ctx._ctx.as_ptr(), self.store_ptr(), Some(callback_get_result_string), callback_get_result_string_data(&mut version_string)) };
    self.ctx.check_call()?;
    version_string
  }

  pub fn parse_path(&self, path: &str) -> Result<NixStorePath> {
    let c_path = CString::new(path)?;
    let path_ptr = unsafe {
      store_parse_path(self.ctx._ctx.as_ptr(), self.store_ptr(), c_path.as_ptr())
    };
    self.ctx.check_call()?;
    Ok(NixStorePath {
      path: Path::new(path).to_path_buf(),
      _ptr: NonNull::new(path_ptr)
        .ok_or_else(|| anyhow::format_err!("store_parse_path returned null"))?
    })
  }
  
  pub fn build(&self, path: &NixStorePath) -> Result<HashMap<String, String>, NixEvalError> {
    let mut map = HashMap::new();
    unsafe {
      store_realise(
        self.ctx._ctx.as_ptr(),
        self.store_ptr(),
        path.as_ptr(),
        &mut map as *mut HashMap<String, String> as *mut c_void,
        Some(read_into_hashmap)
      );
    }
    self.ctx.check_call()?;
    Ok(map)
  }
}

impl NixStorePath {
  fn as_ptr(&self) -> *mut StorePath {
    self._ptr.as_ptr()
  }

  pub fn name(&self) -> Result<String> {
    let mut name : Result<String> = Err(anyhow::anyhow!("Nix C API didn't return a string."));
    unsafe {
      store_path_name(
        self.as_ptr(),
        Some(callback_get_result_string),
        callback_get_result_string_data(&mut name))
    };
    name
  }
}

impl Drop for StoreWrapper {
  fn drop(&mut self) {
    unsafe {
      store_free(self.0.as_ptr());
    }
  }
}

impl Drop for NixStorePath {
  fn drop(&mut self) {
    unsafe {
      store_path_free(self.as_ptr());
    }
  }
}

// impl Clone for NixStore {
//   fn clone(&self) -> Self {
//     unsafe {
//       gc_incref(self.ctx._ctx.as_ptr(), self._store.as_ptr() as *const c_void);
//     }
//     NixStore { _store: self._store.clone(), ctx: self.ctx.clone() }
//   }
// }
