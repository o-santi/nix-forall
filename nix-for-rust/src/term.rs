#![allow(non_upper_case_globals)]
use std::collections::HashMap;
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::ptr::NonNull;
use std::path::PathBuf;
use crate::bindings::{nix_bindings_builder_free, nix_bindings_builder_insert, nix_get_attr_byidx, nix_get_attr_byname, nix_get_attr_name_byidx, nix_get_attrs_size, nix_get_bool, nix_get_float, nix_get_int, nix_get_list_byidx, nix_get_list_size, nix_get_path_string, nix_get_string, nix_get_type, nix_init_bool, nix_init_float, nix_init_int, nix_init_null, nix_init_path_string, nix_init_string, nix_list_builder_insert, nix_make_attrs, nix_make_bindings_builder, nix_make_list, nix_make_list_builder, nix_value_call, ValueType_NIX_TYPE_ATTRS, ValueType_NIX_TYPE_BOOL, ValueType_NIX_TYPE_EXTERNAL, ValueType_NIX_TYPE_FLOAT, ValueType_NIX_TYPE_FUNCTION, ValueType_NIX_TYPE_INT, ValueType_NIX_TYPE_LIST, ValueType_NIX_TYPE_NULL, ValueType_NIX_TYPE_PATH, ValueType_NIX_TYPE_STRING, ValueType_NIX_TYPE_THUNK
};
use crate::error::NixError;
use crate::eval::{NixEvalState, RawValue};
use crate::store::NixContext;
use crate::utils::callback_get_vec_u8;
use thiserror::Error;

pub type AttrSet<'str> = std::collections::HashMap<&'str str, NixTerm>;

#[derive(Debug, Error)]
pub enum NixEvalError {
  #[error("{0}")]
  RuntimeError(NixError),
  #[error("Type error, expected '{expected}' but got '{got}'")]
  TypeError { expected: String, got: String },
  #[error("Cannot build non-derivation")]
  NotADerivation,
  #[error("Build error")]
  BuildError(NixError),
  #[error("Index out of bounds!")]
  IndexOutOfBounds,
}

pub type NixResult<T> = Result<T, NixEvalError>;

#[derive(Clone)]
pub struct NixAttrSet(pub RawValue);
#[derive(Clone)]
pub struct NixList(pub RawValue);
#[derive(Clone)]
pub struct NixFunction(pub RawValue);

pub enum NixTerm {
  Null,
  Thunk(RawValue),
  Int(i64),
  Float(f64),
  Bool(bool),
  List(NixList),
  Path(PathBuf),
  AttrSet(NixAttrSet),
  String(String),
  External(RawValue),
  Function(NixFunction)
}

pub trait ToNix {
  fn to_nix<'s>(self, eval_state: &'s NixEvalState) -> NixResult<NixTerm>;
}

pub trait Repr {
  fn repr_rec<'s>(&self, s: &'s mut String) -> NixResult<()>;
  fn repr(&self) -> NixResult<String> {
    let mut buf = String::new();
    self.repr_rec(&mut buf)?;
    Ok(buf)
  }
}

impl ToNix for NixTerm {
  fn to_nix<'s>(self, _eval_state: &'s NixEvalState) -> NixResult<NixTerm> {
    Ok(self)
  }
}

impl ToNix for RawValue {
  fn to_nix(self, _eval_state: &NixEvalState) -> NixResult<NixTerm> {
    let context = NixContext::default();
    let ctx = context.ptr();
    let value = self.value.as_ptr();
    let value_type = unsafe { nix_get_type(ctx, value) };
    let res = match value_type {
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
      ValueType_NIX_TYPE_LIST => NixTerm::List(NixList(self)),
      ValueType_NIX_TYPE_ATTRS => NixTerm::AttrSet(NixAttrSet(self)),
      ValueType_NIX_TYPE_EXTERNAL => todo!("Not done yet!"),
      ValueType_NIX_TYPE_FUNCTION => {
        NixTerm::Function(NixFunction(self))
      },
      ValueType_NIX_TYPE_THUNK => {
        NixTerm::Thunk(self)
      },
      _ => panic!("Unknown value type"),
    };
    context.check_call()?;
    Ok(res)
  }
}

pub struct NixListIterator {
  pub val: NixList,
  pub len: u32,
  pub idx: u32
}

pub struct NixItemsIterator {
  pub val: NixAttrSet,
  pub len: u32,
  pub idx: u32
}

pub struct NixNamesIterator {
  pub val: NixAttrSet,
  pub len: u32,
  pub idx: u32
}

impl Repr for NixAttrSet {
  fn repr_rec<'s>(&self, s: &'s mut String) -> NixResult<()> {
    s.push_str("{");
    for (key, val) in self.items()? {
      let val = val?;
      s.push_str(" ");
      s.push_str(&key);
      s.push_str(" = ");
      if let NixTerm::List(_) = val {
        s.push_str(" [ ... ]");
      } else if let NixTerm::AttrSet(_) = val {
        s.push_str(" { ... }");
      } else {
        val.repr_rec(s)?;
      };
      s.push_str(";");
    }
    s.push_str(" }");
    Ok(())
  }
}

impl NixAttrSet {
  pub fn build(&self) -> NixResult<HashMap<String, String>> {
    let term_type = self.get("type").map_err(|_| NixEvalError::NotADerivation)?;
    let NixTerm::String(s) = term_type else { return Err(NixEvalError::NotADerivation) };
    if &s == "derivation" {
      let Ok(NixTerm::String (path)) = self.get("drvPath") else { return Err(NixEvalError::NotADerivation) };
      self.0._state.store.build(&path)
    } else {
      Err(NixEvalError::NotADerivation)
    }
  }

  pub fn get(&self, name: &str) -> NixResult<NixTerm> {
    let ctx = &self.0._state.store.ctx;
    let state = &self.0._state;
    let name = CString::new(name).expect("String is not a valid C string");
    let val = unsafe {
      nix_get_attr_byname(ctx.ptr(), self.0.value.as_ptr(), state.state_ptr(), name.as_ptr())
    };
    ctx.check_call()?;
    let value = NonNull::new(val).expect("nix_get_attr_by_name returned null");
    let rawvalue = RawValue {
      value,
      _state: state.clone()
    };
    rawvalue.to_nix(&self.0._state)
  }

  pub fn len(&self) -> NixResult<u32> {
    let len = unsafe { nix_get_attrs_size(self.0._state.store.ctx.ptr(), self.0.value.as_ptr()) };
    self.0._state.store.ctx.check_call()?;
    Ok(len)
  }

  pub fn names(&self) -> NixResult<NixNamesIterator> {
    let iterator = NixNamesIterator {
      val: self.clone(), len: self.len()?, idx: 0
    };
    Ok(iterator)
  }

  pub fn items(&self) -> NixResult<NixItemsIterator> {
    let iterator = NixItemsIterator {
      val: self.clone(), len: self.len()?, idx: 0
    };
    Ok(iterator)
  }
}

impl NixFunction {
  pub fn call_with<T: ToNix>(&self, arg: T) -> NixResult<NixTerm> {
    let state = self.0._state.state_ptr();
    let arg = arg.to_nix(&self.0._state)?.to_raw_value(&self.0._state);
    let ret = RawValue::empty(self.0._state.clone());
    let ctx = NixContext::default();
    unsafe {
      nix_value_call(ctx.ptr(), state, self.0.value.as_ptr(), arg.value.as_ptr(), ret.value.as_ptr());
    }
    ctx.check_call()?;
    ret.to_nix(&self.0._state)
  }
}

impl NixList {
  pub fn len(&self) -> NixResult<u32> {
    let len = unsafe { nix_get_list_size(self.0._state.store.ctx.ptr(), self.0.value.as_ptr()) };
    self.0._state.store.ctx.check_call()?;
    Ok(len)
  }
  
  pub fn iter(&self) -> NixResult<NixListIterator> {
    let iterator = NixListIterator {
      val: self.clone(), len: self.len()?, idx: 0
    };
    Ok(iterator)
  }

  pub fn get_idx(&self, idx: u32) -> NixResult<NixTerm> {
    let raw = &self.0;
    let size = self.len()?;
    if idx > size -1 {
      return Err(NixEvalError::IndexOutOfBounds)
    }
    let elem = unsafe { nix_get_list_byidx(raw._state.store.ctx.ptr(), raw.value.as_ptr(), raw._state.state_ptr(), idx as c_uint) };
    let elem = NonNull::new(elem).expect("nix_get_list_byidx returned null");
    let rawvalue = RawValue {
      _state: raw._state.clone(),
      value: elem
    };
    rawvalue.to_nix(&raw._state)
  }
}

impl Repr for NixList {
  fn repr_rec<'s>(&self, s: &'s mut String) -> NixResult<()> {
    s.push_str("[");
    for t in self.iter()? {
      let t = t?;
      s.push_str(" ");
      if let NixTerm::List(_) = t {
        s.push_str("[ ... ]");
      } else if let NixTerm::AttrSet(_) = t {
        s.push_str("{ ... }");
      } else {
        t.repr_rec(s)?;
      }
    }
    s.push_str(" ]");
    Ok(())
  }
}

impl NixTerm {

  pub fn build(&self) -> NixResult<HashMap<String, String>> {
    if let NixTerm::AttrSet(attrset) = self {
      attrset.build()
    } else {
      Err(NixEvalError::TypeError { expected: "attrset".to_string(), got: self.get_typename() })
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
      NixTerm::Function(_) => "function"
    }.to_string()
  }

  pub fn names(&self) -> NixResult<NixNamesIterator> {
    if let NixTerm::AttrSet(attrset) = self {
      attrset.names()
    } else {
      Err(NixEvalError::TypeError { expected: "attrset".into(), got: self.get_typename() })
    }
  }
  
  pub fn items(&self) -> NixResult<NixItemsIterator> {
    if let NixTerm::AttrSet(attrset) = self {
      attrset.items()
    } else {
      Err(NixEvalError::TypeError { expected: "attrset".into(), got: self.get_typename() })
    }
  }
  
  pub fn iter(&self) -> NixResult<NixListIterator> {
    if let NixTerm::List(list) = self {
      list.iter()
    }
    else {
      Err(NixEvalError::TypeError { expected: "list".into(), got: self.get_typename() })
    }
  }
  
  pub fn to_raw_value(self, _state: &NixEvalState) -> RawValue {
    let ctx = _state.store.ctx.ptr();
    let state = _state.state_ptr();
    let mut rawval = RawValue::empty(_state.clone());
    let val_ptr = rawval.value.as_ptr();
    match self {
      NixTerm::Thunk(raw) | 
      NixTerm::External(raw)  => { rawval = raw;}
      NixTerm::List(list) => { rawval = list.0; }
      NixTerm::AttrSet(attrset) => { rawval = attrset.0; }
      NixTerm::Function(func) => { rawval = func.0; },
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
    _state.store.ctx.check_call().expect("error transforming to raw value");
    rawval
  }
  
  pub fn call_with<T: ToNix>(self, arg: T) -> NixResult<NixTerm> {
    if let NixTerm::Function(func) = self {
      func.call_with(arg)
    } else {
      Err(NixEvalError::TypeError { expected: "function".into(), got: self.get_typename() })
    }
  }
  
  pub fn get(&self, name: &str) -> NixResult<NixTerm> {
    if let NixTerm::AttrSet(attrset) = self {
      attrset.get(name)
    } else {
      Err(NixEvalError::TypeError { expected: "attrset".into(), got: self.get_typename() })
    }
  }

  pub fn as_bool(&self) -> NixResult<bool> {
    let NixTerm::Bool(b) = self else {
      return Err(NixEvalError::TypeError { expected: "bool".into(), got: self.get_typename() });
    };
    Ok(*b)
  }
  
  pub fn as_int(&self) -> NixResult<i64> {
    let NixTerm::Int(i) = self else {
      return Err(NixEvalError::TypeError { expected: "float".into(), got: self.get_typename() });
    };
    Ok(*i)
  }

  pub fn as_float(&self) -> NixResult<f64> {
    let NixTerm::Float(f) = self else {
      return Err(NixEvalError::TypeError { expected: "float".into(), got: self.get_typename() });
    };
    Ok(*f)
  }

  pub fn as_string(&self) -> NixResult<String> {
    let NixTerm::String(s) = self else {
      return Err(NixEvalError::TypeError { expected: "string".into(), got: self.get_typename() });
    };
    Ok(s.to_string())
  }

  pub fn as_list(&self) -> NixResult<Vec<NixTerm>> {
    self.iter()?.collect::<NixResult<_>>()
  }

  pub fn as_hashmap(&self) -> NixResult<HashMap<String, NixResult<NixTerm>>> {
    Ok(self.items()?.collect())
  }

  pub fn as_path(&self) -> NixResult<&PathBuf> {
    let NixTerm::Path(p) = &self else {
      return Err(NixEvalError::TypeError { expected: "path".into(), got: self.get_typename() });
    };
    Ok(p)
  }

}

impl Repr for NixTerm {
  fn repr_rec(&self, s: &mut String) -> NixResult<()> {
    match self {
      NixTerm::Null => s.push_str("null"),
      NixTerm::Thunk(_) => s.push_str("<...>"),
      NixTerm::Int(i) => s.push_str(&i.to_string()),
      NixTerm::Float(float) => s.push_str(&float.to_string()),
      NixTerm::Bool(b) => s.push_str(&b.to_string()),
      NixTerm::Path(path) => s.push_str(path.to_str().expect("Cannot convert path to string")),
      NixTerm::String(str) => s.push_str(str),
      NixTerm::List(list) => list.repr_rec(s)?,
      NixTerm::AttrSet(attrset) => attrset.repr_rec(s)?,
      NixTerm::External(_) => todo!(),
      NixTerm::Function(_) => s.push_str("<lambda>")
    };
    Ok(())
  }
}

impl Iterator for NixListIterator {
  type Item = NixResult<NixTerm>;
  
  fn next(&mut self) -> Option<Self::Item> {
    if self.idx == self.len {
      return None;
    }
    let item = self.val.get_idx(self.idx);
    self.idx = self.idx + 1;
    Some(item)
  }
}

impl Iterator for NixItemsIterator {
  type Item = (String, NixResult<NixTerm>);
  
  fn next(&mut self) -> Option<Self::Item> {
    if self.idx == self.len {
      return None;
    }
    let raw = &self.val.0;
    let mut name: *const c_char = std::ptr::null();
    let elem = unsafe { nix_get_attr_byidx(
      raw._state.store.ctx.ptr(),
      raw.value.as_ptr(),
      raw._state.state_ptr(),
      self.idx as c_uint,
      &mut name
    )};
    self.idx = self.idx + 1;
    let name = unsafe { CStr::from_ptr(name) }.to_str().expect("Nix returned an invalid string").to_owned();
    if let Err(err) = raw._state.store.ctx.check_call() {
      return Some((name, Err(err.into())));
    };
    let elem = NonNull::new(elem).expect("nix_get_attr_byidx returned null");
    let rawvalue = RawValue {
      _state: raw._state.clone(),
      value: elem
    };
    Some((name, rawvalue.to_nix(&raw._state)))
  }
}

impl Iterator for NixNamesIterator {
  type Item = String;
  
  fn next(&mut self) -> Option<Self::Item> {
    if self.idx == self.len {
      return None;
    }
    let raw = &self.val.0;
    let ctx = &raw._state.store.ctx;
    let name = unsafe { nix_get_attr_name_byidx(
      ctx.ptr(),
      raw.value.as_ptr(),
      raw._state.state_ptr(),
      self.idx as c_uint
    )};
    raw._state.store.ctx.check_call().expect("something went wrong");
    let name = unsafe { CStr::from_ptr(name) }.to_str().expect("Nix returned an invalid string").to_owned();
    self.idx = self.idx + 1;
    Some(name)
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

impl<T: ToNix> ToNix for Vec<T> {
  fn to_nix(self, state: &NixEvalState) -> NixResult<NixTerm> {
    let ctx = state.store.ctx.ptr();
    let list_builder = unsafe {
      nix_make_list_builder(ctx, state.state_ptr(), self.len())
    };
    for (idx, elem) in self.into_iter().enumerate() {
      let value = elem.to_nix(state)?.to_raw_value(&state);
      unsafe {
        nix_list_builder_insert(ctx, list_builder, idx as c_uint, value.value.as_ptr());
      }
    }
    let value = RawValue::empty(state.clone());
    unsafe { nix_make_list(ctx, list_builder, value.value.as_ptr()) };
    value.to_nix(state)
  }
}

impl<S: AsRef<str>, T: ToNix> ToNix for HashMap<S, T> {
  fn to_nix(self, state: &NixEvalState) -> NixResult<NixTerm> {
    let ctx = state.store.ctx.ptr();
    let bindings_builder = unsafe {
      nix_make_bindings_builder(ctx, state.state_ptr(), self.len())
    };
    state.store.ctx.check_call().unwrap();
    for (key, val) in self.into_iter() {
      let name = CString::new(key.as_ref()).expect("Key must be valid C string");
      let value = val.to_nix(state)?.to_raw_value(state);
      unsafe {
        nix_bindings_builder_insert(ctx, bindings_builder, name.as_ptr(), value.value.as_ptr());
      }
      state.store.ctx.check_call().unwrap();
    }
    let ctx = NixContext::default();
    let value = RawValue::empty(state.clone());
    unsafe { nix_make_attrs(ctx.ptr(), value.value.as_ptr(), bindings_builder) };
    ctx.check_call().unwrap();
    unsafe { nix_bindings_builder_free(bindings_builder); }
    ctx.check_call().unwrap();
    value.to_nix(state)
  }
}

// impl<F> ToNix for F where F: Fn(NixTerm) -> NixResult<NixTerm> {
//   fn to_nix<'s>(self, state: &'s NixEvalState) -> NixResult<NixTerm> {
//     let context = &state.store.ctx;
//     let name = CString::new("rust-closure").unwrap();
//     let doc = CString::new("rust closure").unwrap();
//     let argname = CString::new("argument").unwrap();
//     let mut args = Vec::from([argname.as_ptr(), std::ptr::null()]);
//     let box_closure = Box::new(Box::new(self));
//     let ptr = Box::into_raw(box_closure);
//     let primop = unsafe {
//       nix_alloc_primop(context.ptr(),
//         Some(call_rust_closure::<F>),
//         1,
//         name.as_ptr(),
//         args.as_mut_ptr(),
//         doc.as_ptr(),
//         ptr as *mut c_void
//       )
//     };
//     context.check_call().expect("Could not allocate primop");
//     let value = RawValue::empty(state.clone());
//     let tmp_ctx = NixContext::default();
//     unsafe {
//       nix_init_primop(tmp_ctx.ptr(), value.value.as_ptr(), primop);
//     };
//     tmp_ctx.check_call().expect("Could not set primop");
//     value.to_nix(state)
//   }

// }

impl From<NixError> for NixEvalError {
  fn from(val: NixError) -> NixEvalError {
    NixEvalError::RuntimeError(val)
  }
}

impl From<std::convert::Infallible> for NixEvalError {
  fn from(value: std::convert::Infallible) -> Self {
    match value {}
  }
}
