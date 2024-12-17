use crate::utils::{callback_get_result_string, callback_get_result_string_data};
use crate::bindings::{err, err_info_msg, err_msg, err_name};
use crate::store::NixContext;
use std::fmt::Display;
use std::ffi::{c_uint, CStr};
use thiserror::Error;

#[derive(Error)]
pub struct NixError {
  code: err::Type,
  msg: String,
  kind: NixErrorKind
}

#[derive(Debug, Error)]
pub enum NixErrorKind {
  #[error("Unknown error")]
  UnknownError,
  #[error("An overflow has occured")]
  OverflowError,
  #[error("Key does not exist")]
  KeyError,
  #[error("{name}: {info_msg}")]
  GenericError { info_msg: String, name: String }
}

impl std::fmt::Debug for NixError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "({}) {self}", self.code as u32)
  }
}

impl Display for NixError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let res = write!(f, "{}", self.kind);
    if !self.msg.is_empty() {
      write!(f, "\nNixInfo: {}", self.msg)
    } else {
      res
    }
  }
}

pub fn handle_nix_error(error: err::Type, ctx: &NixContext) -> NixError {
  let msg= unsafe {
    let mut len : c_uint = 0;
    let extra_ctx = NixContext::default();
    let buf = err_msg(extra_ctx._ctx.as_ptr(), ctx._ctx.as_ptr(), &mut len as *mut c_uint);
    let c_str = CStr::from_ptr(buf);
    c_str.to_str().expect("Error msg is not a valid string").to_owned()
  };
  let kind = match error {
    err::NIX_ERR_KEY => NixErrorKind::KeyError,
    err::NIX_ERR_OVERFLOW => NixErrorKind::OverflowError,
    err::NIX_ERR_UNKNOWN => NixErrorKind::UnknownError,
    err::NIX_ERR_NIX_ERROR => {
      let temp_ctx = NixContext::default();
      let mut name: anyhow::Result<String> = Err(anyhow::anyhow!("Nix C API didn't return string"));
      unsafe {
        err_name(temp_ctx._ctx.as_ptr(), ctx._ctx.as_ptr(), Some(callback_get_result_string), callback_get_result_string_data(&mut name));
      }
      temp_ctx.check_call().expect("error thrown when reading error name");
      let name = name.expect("Nix should always return valid strings");
      let mut info_msg: anyhow::Result<String> = Err(anyhow::anyhow!("Nix C API didn't return string"));
      unsafe { err_info_msg(temp_ctx._ctx.as_ptr(), ctx._ctx.as_ptr(), Some(callback_get_result_string), callback_get_result_string_data(&mut info_msg)) };
      temp_ctx.check_call().expect("error thrown when reading error info");
      let info_msg = info_msg.expect("Nix should always return valid strings");
      NixErrorKind::GenericError { name, info_msg }
    }
    _otherwise => panic!("Unrecognized error code."),
  };
  NixError { code: error, msg, kind }
}
