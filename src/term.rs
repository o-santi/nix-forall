#![allow(non_upper_case_globals)]
use std::ffi::{c_void, CStr, CString};
use std::path;
use std::{collections::HashMap, path::PathBuf};
use crate::bindings::{nix_get_bool, nix_get_float, nix_get_int, nix_get_path_string, nix_get_string, nix_get_type, nix_libutil_init, ValueType_NIX_TYPE_ATTRS, ValueType_NIX_TYPE_BOOL, ValueType_NIX_TYPE_EXTERNAL, ValueType_NIX_TYPE_FLOAT, ValueType_NIX_TYPE_FUNCTION, ValueType_NIX_TYPE_INT, ValueType_NIX_TYPE_LIST, ValueType_NIX_TYPE_NULL, ValueType_NIX_TYPE_PATH, ValueType_NIX_TYPE_STRING, ValueType_NIX_TYPE_THUNK
};
use crate::eval::{NixEvalState, RawValue};
use crate::callback_get_vec_u8;

pub enum NixTerm {
  Null,
  Thunk { val: RawValue },
  Int(i64),
  Float(f64),
  Bool(bool),
  List(Vec<NixTerm>),
  Path(PathBuf),
  AttrSet(HashMap<String, NixTerm>),
  String(String),
  External,
  Function,
}

impl From<RawValue> for NixTerm {
  fn from(value: RawValue) -> Self {
    let ctx = value._state.store.ctx._ctx.as_ptr();
    let value = value.value.as_ptr();
    let value_type = unsafe { nix_get_type(ctx, value) };
    match value_type {
      ValueType_NIX_TYPE_NULL => NixTerm::Null,
      ValueType_NIX_TYPE_INT => {
        let v = unsafe { nix_get_int(ctx, value) };
        NixTerm::Int(v)
      },
      ValueType_NIX_TYPE_BOOL => {
        let b = unsafe { nix_get_bool(ctx, value) };
        NixTerm::Bool(b)
      },
      ValueType_NIX_TYPE_FLOAT => {
        let f = unsafe { nix_get_float(ctx, value) };
        NixTerm::Float(f)
      },
      ValueType_NIX_TYPE_STRING => {
        let mut raw_buffer: Vec<u8> = Vec::new();
        unsafe {
          nix_get_string(ctx, value, Some(callback_get_vec_u8), &mut raw_buffer as *mut Vec<u8> as *mut c_void)
        };
        let s = String::from_utf8(raw_buffer).expect("Nix string is not a valid utf8 string");
        NixTerm::String(s)
      },
      ValueType_NIX_TYPE_PATH => {
        let path = unsafe { nix_get_path_string(ctx, value) };
        let path = unsafe { CStr::from_ptr(path) };
        let path = path.to_str().expect("Nix path must be valid string");
        let path = PathBuf::from(path);
        NixTerm::Path(path)
      },
      ValueType_NIX_TYPE_ATTRS => todo!("Not done yet!"),
      ValueType_NIX_TYPE_EXTERNAL => todo!("Not done yet!"),
      ValueType_NIX_TYPE_FUNCTION => todo!("Not done yet!"),
      ValueType_NIX_TYPE_LIST => todo!("Not done yet!"),
      ValueType_NIX_TYPE_THUNK => todo!("Not done yet!"),
      _ => panic!("Unknown value type"),
    }
  }
}

impl std::fmt::Display for NixTerm {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      NixTerm::Null => write!(f, "null"),
      NixTerm::Thunk { val: _val } => write!(f, "[...]"),
      NixTerm::Int(i) => write!(f, "{i}"),
      NixTerm::Float(float) => write!(f, "{float}"),
      NixTerm::Bool(b) => write!(f, "{b}"),
      NixTerm::Path(path) => write!(f, "{}", path.to_str().expect("Cannot convert path to string")),
      NixTerm::String(s) => write!(f, "\"{s}\""),
      NixTerm::List(_) => todo!(),
      NixTerm::AttrSet(_) => todo!(),
      NixTerm::External => todo!(),
      NixTerm::Function => todo!(),
    }
  }
}
