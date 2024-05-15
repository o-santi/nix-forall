#![allow(non_upper_case_globals)]
use std::collections::HashMap;
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::ptr::NonNull;
use std::path::PathBuf;
use crate::bindings::{nix_bindings_builder_insert, nix_get_attr_byidx, nix_get_attr_byname, nix_get_attrs_size, nix_get_bool, nix_get_float, nix_get_int, nix_get_list_byidx, nix_get_list_size, nix_get_path_string, nix_get_string, nix_get_type, nix_init_bool, nix_init_float, nix_init_int, nix_init_null, nix_init_path_string, nix_init_string, nix_list_builder_insert, nix_make_attrs, nix_make_bindings_builder, nix_make_list, nix_make_list_builder, nix_value_call, ValueType_NIX_TYPE_ATTRS, ValueType_NIX_TYPE_BOOL, ValueType_NIX_TYPE_EXTERNAL, ValueType_NIX_TYPE_FLOAT, ValueType_NIX_TYPE_FUNCTION, ValueType_NIX_TYPE_INT, ValueType_NIX_TYPE_LIST, ValueType_NIX_TYPE_NULL, ValueType_NIX_TYPE_PATH, ValueType_NIX_TYPE_STRING, ValueType_NIX_TYPE_THUNK
};
use crate::error::NixError;
use crate::eval::{NixEvalState, RawValue};
use crate::store::{NixContext, NixStore};
use crate::utils::callback_get_vec_u8;
use thiserror::Error;

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
          if let NixTerm::List(_) = term {
            write!(f, "[ ... ]")?;
          } else if let NixTerm::AttrSet(_) = term {
            write!(f, "{{ ... }}")?;
          } else {
            write!(f, " {term} ")?;
          }
        }
        write!(f, "]")
      },
      NixTerm::AttrSet(_) => {
        write!(f, "{{\n")?;
        for (key, val) in self.items().unwrap() {
          write!(f, "  {key} = ")?;
          if let NixTerm::List(_) = val {
            write!(f, "[ ... ]")?;
          } else if let NixTerm::AttrSet(_) = val {
            write!(f, "{{ ... }}")?;
          } else {
            write!(f, "{val}")?;
          };
          write!(f, ";\n")?;
        }
        write!(f, "}}")
      },
      NixTerm::External(_) => todo!(),
      NixTerm::Function(_) => write!(f, "<lambda ...>"),
    }
  }
}

#[derive(Debug, Error)]
pub enum NixEvalError {
  #[error("\n{0}")]
  RuntimeError(NixError),
  #[error("Cannot call term of type {term_type}")]
  CannotCall { term_type: String },
  #[error("Cannot iterate term of type {term_type}")]
  CannotIter { term_type: String },
  #[error("Cannot get items of term of type {term_type}")]
  CannotGetItems { term_type: String },
  #[error("Cannot build non-derivation")]
  NotADerivation,
  #[error("Build error")]
  BuildError(NixError)
}

pub struct NixListIterator {
  val: RawValue,
  len: u32,
  idx: u32
}

pub struct NixAttrSetIterator {
  val: RawValue,
  len: u32,
  idx: u32
}

impl NixTerm {

  pub fn build(&self) -> Result<HashMap<String, String>, NixEvalError> {
    let term_type = self.get("type").map_err(|_| NixEvalError::NotADerivation)?;
    let NixTerm::AttrSet(ref rawvalue) = self else { panic!("Object should always be attrset") };
    let NixTerm::String(s) = term_type else { return Err(NixEvalError::NotADerivation) };
    if &s == "derivation" {
      let Ok(NixTerm::String (path)) = self.get("drvPath") else { return Err(NixEvalError::NotADerivation) };
      rawvalue._state.store.build(&path)
    } else {
      Err(NixEvalError::NotADerivation)
    }
  }

  pub fn get_typename(&self) -> String {
    match self {
      NixTerm::Null => "null",
      NixTerm::Thunk(_) => "thunk",
      NixTerm::Int(_) => "int",
      NixTerm::Float(_) => "float",
      NixTerm::Bool(_) => "bool",
      NixTerm::List(_) => "list",
      NixTerm::Path(_) => "path",
      NixTerm::AttrSet(_) => "attrset",
      NixTerm::String(_) => "string",
      NixTerm::External(_) => "external",
      NixTerm::Function(_) => "function",
    }.to_string()
  }

  pub fn items(&self) -> Result<NixAttrSetIterator, NixEvalError> {
    if let NixTerm::AttrSet(rawvalue) = self {
      let len = unsafe { nix_get_attrs_size(rawvalue._state.store.ctx._ctx.as_ptr(), rawvalue.value.as_ptr()) };
      let iterator = NixAttrSetIterator {
        val: rawvalue.clone(), len, idx: 0
      };
      Ok(iterator)
    } else {
      Err(NixEvalError::CannotGetItems { term_type: self.get_typename() })
    }
  }
  
  pub fn iter(&self) -> Result<NixListIterator, NixEvalError> {
    if let NixTerm::List(rawvalue) = self {
      let len = unsafe { nix_get_list_size(rawvalue._state.store.ctx._ctx.as_ptr(), rawvalue.value.as_ptr()) };
      let iterator = NixListIterator {
        val: rawvalue.clone(), len, idx: 0
      };
      Ok(iterator)
    }
    else {
      Err(NixEvalError::CannotIter { term_type: self.get_typename() })
    } 
  }
  pub fn to_raw_value(self, _state: &NixEvalState) -> RawValue {
    let ctx = _state.store.ctx._ctx.as_ptr();
    let state = _state._eval_state.as_ptr();
    let mut rawval = RawValue::empty(_state.clone());
    let val_ptr = rawval.value.as_ptr();
    match self {
      NixTerm::Thunk(raw) |
      NixTerm::List(raw) |
      NixTerm::AttrSet(raw) |
      NixTerm::External(raw) |
      NixTerm::Function(raw) => {
        rawval = raw;
      },
      NixTerm::Null =>  unsafe {
        nix_init_null(ctx, val_ptr);
      }
      NixTerm::Int(i) => unsafe {
        nix_init_int(ctx, val_ptr, i);
      }
      NixTerm::Float(f) => unsafe {
        nix_init_float(ctx, val_ptr, f);
      }
      NixTerm::Bool(b) => unsafe {
        nix_init_bool(ctx, val_ptr, b);
      }
      NixTerm::Path(p) => {
        let string = p.to_str().expect("path is not a valid string");
        let c_str = CString::new(string).expect("path is not a valid C String");
        unsafe {
          nix_init_path_string(ctx, state, val_ptr, c_str.as_ptr());
        }
      },
      NixTerm::String(s) => {
        let c_str = CString::new(s.to_owned()).expect("path is not a valid C String");
        unsafe {
          nix_init_string(ctx, val_ptr, c_str.as_ptr());
        }
      },
    };
    rawval
  }
  
  pub fn call_with<I: Into<NixTerm>>(self, arg: I) -> Result<NixTerm, NixEvalError> {
    if let NixTerm::Function(func) = self {
      let ctx = func._state.store.ctx._ctx.as_ptr();
      let state = func._state._eval_state.as_ptr();
      let arg = arg.into().to_raw_value(&func._state);
      let ret = RawValue::empty(func._state.clone());
      unsafe {
        nix_value_call(ctx, state, func.value.as_ptr(), arg.value.as_ptr(), ret.value.as_ptr());
      }
      func._state.store.ctx.check_call()?;
      Ok(NixTerm::from(ret))
    } else {
      Err(NixEvalError::CannotCall { term_type: self.get_typename() })
    }
  }

  pub fn get(&self, name: &str) -> Result<NixTerm, NixEvalError> { // 
    if let NixTerm::AttrSet(attrset) = self {
      let ctx = &attrset._state.store.ctx;
      let state = attrset._state._eval_state;
      let name = CString::new(name).expect("String is not a valid C string");
      let val = unsafe {
        nix_get_attr_byname(ctx._ctx.as_ptr(), attrset.value.as_ptr(), state.as_ptr(), name.as_ptr())
      };
      ctx.check_call()?;
      let value = NonNull::new(val).expect("nix_get_attr_by_name returned null");
      let rawvalue = RawValue {
        value,
        _state: attrset._state.clone()
      };
      Ok(rawvalue.into())
    } else {
      Err(NixEvalError::CannotGetItems { term_type: self.get_typename() })
    }
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
      None => {
        self.val._state.store.ctx.check_call().expect("what happened");
        todo!();
      }
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
    self.val._state.store.ctx.check_call().expect("something went wrong");
    let elem = NonNull::new(elem).expect("nix_get_attr_byidx returned null");
    let rawvalue = RawValue {
      _state: self.val._state.clone(),
      value: elem
    };
    let name = unsafe { CStr::from_ptr(name) }.to_str().expect("Nix returned an invalid string").to_owned();
    self.idx = self.idx + 1;
    Some((name, rawvalue.into()))
  }
}

impl From<&str> for NixTerm {
  fn from(val: &str) -> Self {
    NixTerm::String(val.to_string())
  }
}

impl From<i64> for NixTerm {
  fn from(val: i64) -> Self {
    NixTerm::Int(val)
  }
}

impl From<PathBuf> for NixTerm {
  fn from(val: PathBuf) -> Self {
    NixTerm::Path(val)
  }
}

impl From<bool> for NixTerm {
  fn from(val: bool) -> Self {
    NixTerm::Bool(val)
  }
}

impl<T: Into<NixTerm>> From<Vec<T>> for NixTerm {
  fn from(val: Vec<T>) -> Self {
    let context = NixContext::default();
    let store = NixStore::new(context, "");
    let state = NixEvalState::new(store);
    let ctx = state.store.ctx._ctx.as_ptr();
    let list_builder = unsafe {
      nix_make_list_builder(ctx, state._eval_state.as_ptr(), val.len())
    };
    for (idx, elem) in val.into_iter().enumerate() {
      let value = Into::<NixTerm>::into(elem).to_raw_value(&state);
      unsafe {
        nix_list_builder_insert(ctx, list_builder, idx as c_uint, value.value.as_ptr());
      }
    }
    let value = RawValue::empty(state);
    unsafe { nix_make_list(ctx, list_builder, value.value.as_ptr()) };
    value.into()
  }
}

impl<T: Into<NixTerm>> From<HashMap<&str, T>> for NixTerm {
  fn from(val: HashMap<&str, T>) -> Self {
    let context = NixContext::default();
    let store = NixStore::new(context, "");
    let state = NixEvalState::new(store);
    let ctx = state.store.ctx._ctx.as_ptr();
    let bindings_builder = unsafe {
      nix_make_bindings_builder(ctx, state._eval_state.as_ptr(), val.len())
    };
    for (key, val) in val.into_iter() {
      let value = Into::<NixTerm>::into(val).to_raw_value(&state);
      let name = CString::new(key).expect("Key must be valid C string");
      unsafe {
        nix_bindings_builder_insert(ctx, bindings_builder, name.as_ptr(), value.value.as_ptr());
      }
    }
    let value = RawValue::empty(state);
    unsafe { nix_make_attrs(ctx, value.value.as_ptr(), bindings_builder) };
    value.into()
  }
}

impl From<NixError> for NixEvalError {
  fn from(val: NixError) -> NixEvalError {
    NixEvalError::RuntimeError(val)
  }
}
