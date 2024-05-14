#![allow(non_upper_case_globals)]
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::num::NonZeroI128;
use std::path;
use std::ptr::NonNull;
use std::str::SplitTerminator;
use std::{collections::HashMap, path::PathBuf};
use crate::bindings::{nix_get_attr_byidx, nix_get_attrs_size, nix_get_bool, nix_get_float, nix_get_int, nix_get_list_byidx, nix_get_list_size, nix_get_path_string, nix_get_string, nix_get_type, nix_libutil_init, nix_value_call, Value, ValueType_NIX_TYPE_ATTRS, ValueType_NIX_TYPE_BOOL, ValueType_NIX_TYPE_EXTERNAL, ValueType_NIX_TYPE_FLOAT, ValueType_NIX_TYPE_FUNCTION, ValueType_NIX_TYPE_INT, ValueType_NIX_TYPE_LIST, ValueType_NIX_TYPE_NULL, ValueType_NIX_TYPE_PATH, ValueType_NIX_TYPE_STRING, ValueType_NIX_TYPE_THUNK
};
use crate::eval::{NixEvalState, RawValue};
use crate::callback_get_vec_u8;

pub enum NixTerm {
  Null,
  Thunk(RawValue),
  Int(i64),
  Float(f64),
  Bool(bool),
  List(RawValue),
  Path(PathBuf),
  AttrSet(RawValue),
  String(String),
  External(RawValue),
  Function(RawValue),
}

impl From<RawValue> for NixTerm {
  fn from(rawvalue: RawValue) -> Self {
    let ctx = rawvalue._state.store.ctx._ctx.as_ptr();
    let value = rawvalue.value.as_ptr();
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
      ValueType_NIX_TYPE_LIST => NixTerm::List(rawvalue),
      ValueType_NIX_TYPE_ATTRS => NixTerm::AttrSet(rawvalue),
      ValueType_NIX_TYPE_EXTERNAL => todo!("Not done yet!"),
      ValueType_NIX_TYPE_FUNCTION => {
        NixTerm::Function(rawvalue)
      },
      ValueType_NIX_TYPE_THUNK => {
        NixTerm::Thunk(rawvalue)
      },
      _ => panic!("Unknown value type"),
    }
  }
}

impl From<NixTerm> for RawValue {
  fn from(value: NixTerm) -> Self {
    match value {
      NixTerm::Null => todo!(),
      NixTerm::Thunk(_) => todo!(),
      NixTerm::Int(_) => todo!(),
      NixTerm::Float(_) => todo!(),
      NixTerm::Bool(_) => todo!(),
      NixTerm::List(_) => todo!(),
      NixTerm::Path(_) => todo!(),
      NixTerm::AttrSet(_) => todo!(),
      NixTerm::String(_) => todo!(),
      NixTerm::External(_) => todo!(),
      NixTerm::Function(_) => todo!(),
    }
  }
}

impl std::fmt::Display for NixTerm {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      NixTerm::Null => write!(f, "null"),
      NixTerm::Thunk(_) => write!(f, "[...]"),
      NixTerm::Int(i) => write!(f, "{i}"),
      NixTerm::Float(float) => write!(f, "{float}"),
      NixTerm::Bool(b) => write!(f, "{b}"),
      NixTerm::Path(path) => write!(f, "{}", path.to_str().expect("Cannot convert path to string")),
      NixTerm::String(s) => write!(f, "\"{s}\""),
      NixTerm::List(_) => {
        write!(f, "[")?;
        for term in self.iter().unwrap() {
          write!(f, " {term} ")?;
        }
        write!(f, "]")
      },
      NixTerm::AttrSet(_) => {
        write!(f, "{{")?;
        for (key, val) in self.items().unwrap() {
          write!(f, " {key}={val}; ")?;
        }
        write!(f, "}}")
      },
      NixTerm::External(_) => todo!(),
      NixTerm::Function(_) => write!(f, "<lambda ...>"),
    }
  }
}

#[derive(Debug)]
enum NixEvalError {
  CannotCall,
  CannotIter,
  CannotGetItems
}

struct NixListIterator {
  val: RawValue,
  len: u32,
  idx: u32
}

struct NixAttrSetIterator {
  val: RawValue,
  len: u32,
  idx: u32
}

impl NixTerm {

  fn items(&self) -> Result<NixAttrSetIterator, NixEvalError> {
    if let NixTerm::AttrSet(rawvalue) = self {
      let len = unsafe { nix_get_attrs_size(rawvalue._state.store.ctx._ctx.as_ptr(), rawvalue.value.as_ptr()) };
      let iterator = NixAttrSetIterator {
        val: rawvalue.clone(), len, idx: 0
      };
      Ok(iterator)
    } else {
      Err(NixEvalError::CannotGetItems)
    }
  }
  
  fn iter(&self) -> Result<NixListIterator, NixEvalError> {
    if let NixTerm::List(rawvalue) = self {
      let len = unsafe { nix_get_list_size(rawvalue._state.store.ctx._ctx.as_ptr(), rawvalue.value.as_ptr()) };
      let iterator = NixListIterator {
        val: rawvalue.clone(), len, idx: 0
      };
      Ok(iterator)
    }
    else {
      Err(NixEvalError::CannotIter)
    } 
  }
  
  fn call(arg: NixTerm) -> Result<NixTerm, NixEvalError> {
    todo!()
  }
}

impl Iterator for NixListIterator {
  type Item = NixTerm;
  
  fn next(&mut self) -> Option<Self::Item> {
    if self.idx == self.len {
      return None;
    }
    let elem = unsafe { nix_get_list_byidx(self.val._state.store.ctx._ctx.as_ptr(), self.val.value.as_ptr(), self.val._state._eval_state.as_ptr(), self.idx as c_uint) };
    let elem = match NonNull::new(elem) {
      Some(e) => e,
      None => panic!("nix_get_list_by_idx returned null.")
    };
    let rawvalue = RawValue {
      _state: self.val._state.clone(),
      value: elem
    };
    self.idx = self.idx + 1;
    Some(NixTerm::from(rawvalue))
  }
}

impl Iterator for NixAttrSetIterator {
  type Item = (String, NixTerm);
  
  fn next(&mut self) -> Option<Self::Item> {
    if self.idx == self.len {
      return None;
    }
    let mut name: *const c_char = std::ptr::null();
    let elem = unsafe { nix_get_attr_byidx(
      self.val._state.store.ctx._ctx.as_ptr(),
      self.val.value.as_ptr(),
      self.val._state._eval_state.as_ptr(),
      self.idx as c_uint,
      &mut name
    )};
    let elem = match NonNull::new(elem) {
      Some(e) => e,
      None => panic!("nix_get_list_by_idx returned null.")
    };
    let rawvalue = RawValue {
      _state: self.val._state.clone(),
      value: elem
    };
    let name = unsafe { CString::from_raw(name as *mut c_char) }.into_string().expect("Nix returned an invalid string");
    self.idx = self.idx + 1;
    Some((name, rawvalue.into()))
  }
}
