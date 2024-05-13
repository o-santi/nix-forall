use crate::{callback_get_vec_u8, get_string_callback};
use crate::bindings::{NIX_OK, nix_err, nix_err_info_msg, nix_err_msg, nix_err_name, NIX_ERR_KEY, NIX_ERR_OVERFLOW, NIX_ERR_NIX_ERROR, NIX_ERR_UNKNOWN};
use crate::eval::NixEvalState;
use crate::store::NixContext;
use std::fmt::Display;
use std::ffi::{CString, c_void, c_uint};

#[derive(Debug)]
pub struct NixError {
  code: i32,
  msg: String,
  kind: NixErrorKind
}

#[derive(Debug)]
pub enum NixErrorKind {
  UnknownError,
  OverflowError,
  KeyError,
  GenericError { info_msg: String, name: String }
}

impl Display for NixError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[Nix Error] {} {}\n{}", self.code, self.msg, self.kind) 
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

pub fn handle_nix_error(error: nix_err, eval_state: &NixEvalState) -> NixError {
  let msg= unsafe {
    let mut len : c_uint = 0;
    let extra_ctx = NixContext::default();
    let buf = nix_err_msg(extra_ctx._ctx.as_ptr(), eval_state.store.ctx._ctx.as_ptr(), &mut len as *mut c_uint);
    String::from_raw_parts(buf as *mut u8, len as usize, len as usize)
  };
  let kind = match error {
    NIX_ERR_KEY => NixErrorKind::KeyError,
    NIX_ERR_OVERFLOW => NixErrorKind::OverflowError,
    NIX_ERR_UNKNOWN => NixErrorKind::UnknownError,
    NIX_ERR_NIX_ERROR => {
      let temp_ctx = NixContext::default();
      let name = CString::new([1; 256]).unwrap();
      let result = unsafe {
        nix_err_name(temp_ctx._ctx.as_ptr(), eval_state.store.ctx._ctx.as_ptr(), Some(get_string_callback), name.as_ptr() as *mut c_void)
      };
      if result as u32 != NIX_OK {
        panic!("Error thrown when reading error name");
      }
      let name = name.into_string().expect("Nix should always return valid strings");
      let temp_ctx = NixContext::default();
      let mut info_msg: Vec<u8> = Vec::new();
      let result = unsafe { nix_err_info_msg(temp_ctx._ctx.as_ptr(), eval_state.store.ctx._ctx.as_ptr(), Some(callback_get_vec_u8), &mut info_msg as *mut Vec<u8> as *mut c_void) };
      if result as u32 != NIX_OK {
        panic!("Error thrown when reading error info msg");
      }
      let info_msg = String::from_utf8(info_msg).expect("Nix should always return valid strings");
      NixErrorKind::GenericError { name, info_msg }
    }
    e => panic!("Unknow error kind: {e}")
  };
  NixError { code: error, msg, kind }
}
