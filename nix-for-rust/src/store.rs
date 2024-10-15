use crate::error::{handle_nix_error, NixError};
use crate::term::NixEvalError;
use crate::utils::{callback_get_vec_u8, read_into_hashmap};
use crate::bindings::{c_context, c_context_create, err_code, libexpr_init, libstore_init, libutil_init, store_free, store_get_version, store_open, store_parse_path, store_realise, Store, StorePath, err};
use std::collections::HashMap;
use std::ffi::{c_void, CString};
use std::os::raw::c_char;
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
    unsafe {
      libutil_init(_ctx);
      libexpr_init(_ctx);
      libstore_init(_ctx);
    };
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
    if err != err::NIX_OK {
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

pub struct StoreWrapper(NonNull<Store>);

impl NixStore {

  pub fn store_ptr(&self) -> *mut Store {
    self._store.0.as_ptr()
  }
  
  pub fn new(ctx: NixContext, uri: &str) -> Self {
    let uri = CString::new(uri).expect("Invalid C-String");
    let _store = unsafe {
      let params: *mut *mut *const c_char = [null_mut() as *mut *const c_char].as_mut_ptr();
      store_open(ctx._ctx.as_ptr(), uri.into_raw(), params)
    };
    let store = match NonNull::new(_store) {
      Some(s) => s,
      None => panic!("store_open returned null")
    };
    NixStore { ctx, _store: Rc::new(StoreWrapper(store)) }
  }
  
  pub fn version(&self) -> Result<String> {
    unsafe {
      let version_string : Vec<u8> = Vec::new();
      let result = store_get_version(self.ctx._ctx.as_ptr(), self.store_ptr(), Some(callback_get_vec_u8), version_string.as_ptr() as *mut c_void);
      if result == err::NIX_OK {
        Ok(String::from_utf8(version_string).expect("Nix returned invalid string"))
      } else {
        anyhow::bail!("Could not read version string.")
      }
    }
  }

  fn parse_path(&self, path: &str) -> Result<NonNull<StorePath>, NixEvalError> {
    let c_path = CString::new(path).expect("nix path is not a valid c string");
    let path = unsafe {
      store_parse_path(self.ctx._ctx.as_ptr(), self.store_ptr(), c_path.as_ptr())
    };
    self.ctx.check_call()?;
    Ok(NonNull::new(path)
      .expect("store_parse_path returned null"))
  }
  
  pub fn build(&self, path: &str) -> Result<HashMap<String, String>, NixEvalError> {
    let path = self.parse_path(path)?;
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

impl Drop for StoreWrapper {
  fn drop(&mut self) {
    unsafe {
      store_free(self.0.as_ptr());
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
