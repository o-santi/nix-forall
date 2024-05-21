use crate::error::{handle_nix_error, NixError};
use crate::term::NixEvalError;
use crate::utils::{callback_get_vec_u8, read_into_hashmap};
use crate::bindings::{nix_c_context, nix_c_context_create, nix_err_code, nix_gc_decref, nix_gc_incref, nix_libexpr_init, nix_libstore_init, nix_libutil_init, nix_store_free, nix_store_get_version, nix_store_open, nix_store_parse_path, nix_store_realise, Store, StorePath, NIX_OK};
use std::collections::HashMap;
use std::ffi::{c_void, CString};
use std::ptr::NonNull;
use anyhow::Result;
use std::rc::Rc;

#[derive(Clone)]
pub struct NixContext {
  pub(crate) _ctx: NonNull<nix_c_context>,
}

impl Default for NixContext {
  fn default() -> Self {
    let _ctx = unsafe { nix_c_context_create() };
    unsafe {
      nix_libutil_init(_ctx);
      nix_libexpr_init(_ctx);
      nix_libstore_init(_ctx);
    };
    let _ctx = match NonNull::new(_ctx) {
      Some(c) => c,
      None => panic!("nix_c_context_create returned null")
    };
    NixContext { _ctx  }
  }

}

impl NixContext {

  pub fn ptr(&self) -> *mut nix_c_context {
    self._ctx.as_ptr()
  }

  pub fn check_call(&self) -> std::result::Result<(), NixError> {
    let err = unsafe { nix_err_code(self._ctx.as_ptr())};
    if err as u32 != NIX_OK {
      Err(handle_nix_error(err, self))
    } else {
      Ok(())
    }
  } 
}

#[derive(Clone)]
pub struct NixStore {
  pub(crate) ctx: NixContext,
  pub(crate) _store: Rc<StoreWrapper>
}

struct StoreWrapper(NonNull<Store>);

impl NixStore {

  pub fn store_ptr(&self) -> *mut Store {
    self._store.0.as_ptr()
  }
  
  pub fn new(ctx: NixContext, uri: &str) -> Self {
    let uri = CString::new(uri).expect("Invalid C-String");
    let _store = unsafe {
      let params = [].as_mut_ptr();
      nix_store_open(ctx._ctx.as_ptr(), uri.into_raw(), params)
    };
    let store = match NonNull::new(_store) {
      Some(s) => s,
      None => panic!("nix_store_open returned null")
    };
    NixStore { ctx, _store: Rc::new(StoreWrapper(store)) }
  }
  
  pub fn version(&self) -> Result<String> {
    unsafe {
      let version_string : Vec<u8> = Vec::new();
      let result = nix_store_get_version(self.ctx._ctx.as_ptr(), self.store_ptr(), Some(callback_get_vec_u8), version_string.as_ptr() as *mut c_void);
      if result == NIX_OK as i32 {
        Ok(String::from_utf8(version_string).expect("Nix returned invalid string"))
      } else {
        anyhow::bail!("Could not read version string.")
      }
    }
  }

  fn parse_path(&self, path: &str) -> Result<NonNull<StorePath>, NixEvalError> {
    let c_path = CString::new(path).expect("nix path is not a valid c string");
    let path = unsafe {
      nix_store_parse_path(self.ctx._ctx.as_ptr(), self.store_ptr(), c_path.as_ptr())
    };
    self.ctx.check_call()?;
    Ok(NonNull::new(path)
      .expect("nix_store_parse_path returned null"))
  }
  
  pub fn build(&self, path: &str) -> Result<HashMap<String, String>, NixEvalError> {
    let path = self.parse_path(path)?;
    let mut map = HashMap::new();
    unsafe {
      nix_store_realise(
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
      nix_store_free(self.0.as_ptr());
    }
  }
}

// impl Clone for NixStore {
//   fn clone(&self) -> Self {
//     unsafe {
//       nix_gc_incref(self.ctx._ctx.as_ptr(), self._store.as_ptr() as *const c_void);
//     }
//     NixStore { _store: self._store.clone(), ctx: self.ctx.clone() }
//   }
// }
