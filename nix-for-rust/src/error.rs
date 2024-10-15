use crate::term::NixEvalError;
use crate::utils::callback_get_vec_u8;
use crate::bindings::{nix_err_NIX_OK, nix_err, nix_err_info_msg, nix_err_msg, nix_err_name, nix_err_NIX_ERR_KEY, nix_err_NIX_ERR_OVERFLOW, nix_err_NIX_ERR_NIX_ERROR, nix_err_NIX_ERR_UNKNOWN};
use crate::store::NixContext;
use std::fmt::Display;
use std::ffi::{c_uint, c_void, CStr};

pub struct NixError {
  code: i32,
  msg: String,
  kind: NixErrorKind
}

pub enum NixErrorKind {
  UnknownError,
  OverflowError,
  KeyError,
  GenericError { info_msg: String, name: String }
}

impl std::fmt::Debug for NixError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{self}")
  }
}

impl Display for NixError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.kind)
  }
}

impl Display for NixErrorKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      NixErrorKind::UnknownError => write!(f, "Unknown error"),
      NixErrorKind::OverflowError => write!(f, "An overflow has occured"),
      NixErrorKind::KeyError => write!(f, "Key does not exist"),
      NixErrorKind::GenericError { info_msg, name } => write!(f, "{name}: {info_msg}"),
    }
  }
}

pub fn handle_nix_error(error: nix_err, ctx: &NixContext) -> NixError {
  let msg= unsafe {
    let mut len : c_uint = 0;
    let extra_ctx = NixContext::default();
    let buf = nix_err_msg(extra_ctx._ctx.as_ptr(), ctx._ctx.as_ptr(), &mut len as *mut c_uint);
    let c_str = CStr::from_ptr(buf);
    c_str.to_str().expect("Error msg is not a valid string").to_owned()
  };
  let kind = match error {
    nix_err_NIX_ERR_KEY => NixErrorKind::KeyError,
    nix_err_NIX_ERR_OVERFLOW => NixErrorKind::OverflowError,
    nix_err_NIX_ERR_UNKNOWN => NixErrorKind::UnknownError,
    nix_err_NIX_ERR_NIX_ERROR => {
      let temp_ctx = NixContext::default();
      let mut name = Vec::new();
      let result = unsafe {
        nix_err_name(temp_ctx._ctx.as_ptr(), ctx._ctx.as_ptr(), Some(callback_get_vec_u8), &mut name as *mut Vec<u8> as *mut c_void)
      };
      if result != nix_err_NIX_OK {
        panic!("Error thrown when reading error name");
      }
      let name = String::from_utf8(name).expect("Nix should always return valid strings");
      let temp_ctx = NixContext::default();
      let mut info_msg: Vec<u8> = Vec::new();
      let result = unsafe { nix_err_info_msg(temp_ctx._ctx.as_ptr(), ctx._ctx.as_ptr(), Some(callback_get_vec_u8), &mut info_msg as *mut Vec<u8> as *mut c_void) };
      if result != nix_err_NIX_OK {
        panic!("Error thrown when reading error info msg");
      }
      let info_msg = String::from_utf8(info_msg).expect("Nix should always return valid strings");
      NixErrorKind::GenericError { name, info_msg }
    }
    e => panic!("Unknow error kind: {e}")
  };
  NixError { code: error, msg, kind }
}
