use crate::{callback_get_vec_u8, get_string_callback};
use crate::bindings::{nix_c_context, nix_c_context_create, nix_c_context_free, nix_gc_decref, nix_gc_incref, nix_libstore_init, nix_libutil_init, nix_store_free, nix_store_get_version, nix_store_open, Store, NIX_OK};
use std::ffi::{CString, c_void};
use std::ptr::NonNull;
use anyhow::Result;

#[derive(Clone)]
pub struct NixContext {
  pub(crate) _ctx: NonNull<nix_c_context>,
}

impl Default for NixContext {
  fn default() -> Self {
    let _ctx = unsafe { nix_c_context_create() };
    unsafe { nix_libutil_init(_ctx) };
    let _ctx = match NonNull::new(_ctx) {
      Some(c) => c,
      None => panic!("nix_c_context_create returned null")
    };
    NixContext { _ctx  }
  }
}

pub struct NixStore {
  pub(crate) ctx: NixContext,
  pub(crate) _store: NonNull<Store>
}

impl NixStore {
  pub fn new(ctx: NixContext, uri: &str) -> Self {
    let uri = CString::new(uri).expect("Invalid C-String");
    let _store = unsafe {
      nix_libstore_init(ctx._ctx.as_ptr());
      let params = [].as_mut_ptr();
      nix_store_open(ctx._ctx.as_ptr(), uri.into_raw(), params)
    };
    let _store = match NonNull::new(_store) {
      Some(s) => s,
      None => panic!("nix_store_open returned null")
    };
    NixStore { ctx, _store }
  }
  
  pub fn version(&self) -> Result<String> {
    unsafe {
      let version_string : Vec<u8> = Vec::new();
      let result = nix_store_get_version(self.ctx._ctx.as_ptr(), self._store.as_ptr(), Some(callback_get_vec_u8), version_string.as_ptr() as *mut c_void);
      if result == NIX_OK as i32 {
        Ok(String::from_utf8(version_string).expect("Nix returned invalid string"))
      } else {
        anyhow::bail!("Could not read version string.")
      }
    }
  }
}

impl Drop for NixStore {
  fn drop(&mut self) {
    unsafe {
      nix_gc_decref(self.ctx._ctx.as_ptr(), self._store.as_ptr() as *const c_void);
    }
  }
}

impl Clone for NixStore {
  fn clone(&self) -> Self {
    unsafe {
      nix_gc_incref(self.ctx._ctx.as_ptr(), self._store.as_ptr() as *const c_void);
    }
    NixStore { _store: self._store.clone(), ctx: self.ctx.clone() }
  }
}

