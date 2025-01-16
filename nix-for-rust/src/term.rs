#![allow(non_upper_case_globals)]
use std::collections::HashMap;
use std::ffi::{c_char, c_uint, CStr, CString};
use std::ptr::NonNull;
use std::path::PathBuf;
use crate::bindings::{bindings_builder_free, bindings_builder_insert, get_attr_byidx, get_attr_byname, get_attr_name_byidx, get_attrs_size, get_bool, get_float, get_int, get_list_byidx, get_list_size, get_path_string, get_string, get_type, init_bool, init_float, init_int, init_null, init_path_string, init_string, list_builder_insert, make_attrs, make_bindings_builder, make_list, make_list_builder, value_call, value_force, ValueType};
use crate::error::NixError;
use crate::eval::{NixEvalState, RawValue};
use crate::store::NixContext;
use crate::utils::{callback_get_result_string, callback_get_result_string_data};
use thiserror::Error;

/// Type of hashmaps that can be represented as a nix attrset
pub type AttrSet<'str, 'state> = std::collections::HashMap<&'str str, NixTerm<'state>>;

/// An error that might happen when evaluating nix expressions.
#[derive(Debug, Error)]
pub enum NixEvalError {
  #[error("{0}")]
  RuntimeError(NixError),
  #[error("Type error, expected '{expected}' but got '{got}'")]
  TypeError { expected: String, got: String },
  #[error("Cannot build non-derivation")]
  NotADerivation,
  #[error("Index out of bounds!")]
  IndexOutOfBounds,
  #[error("Nix returned invalid string")]
  InvalidString,
  #[error("Invalid path")]
  InvalidPath(String)
}

pub type NixResult<T> = Result<T, NixEvalError>;


/// Wrapper around a pointer to nix attribute set.
#[derive(Clone)]
pub struct NixAttrSet<'state>(pub(crate) RawValue<'state>);
/// Wrapper around a pointer to nix list.
#[derive(Clone)]
pub struct NixList<'state>(pub(crate) RawValue<'state>);
/// Wrapper around a pointer to nix function.
#[derive(Clone)]
pub struct NixFunction<'state>(pub(crate) RawValue<'state>);
/// Wrapper around a pointer to nix thunk
#[derive(Clone)]
pub struct NixThunk<'state>(pub(crate) RawValue<'state>);

/// A nix term represented as a rust value.
pub enum NixTerm<'state> {
  Null,
  Thunk(NixThunk<'state>),
  Int(i64),
  Float(f64),
  Bool(bool),
  List(NixList<'state>),
  Path(PathBuf),
  AttrSet(NixAttrSet<'state>),
  String(String),
  External(RawValue<'state>),
  Function(NixFunction<'state>)
}

  /// Conversion trait between rust objects and nix values.
pub trait ToNix<'state> {
  fn to_nix(self, eval_state: &'state NixEvalState) -> NixResult<NixTerm<'state>>;
}

/// Trait to print a nix term, which may throw errors during evaluation time.
pub trait Repr {
  fn repr_rec(&self, s: &mut String) -> NixResult<()>;
  /// Returns a string with the objects representation, or an error that happened during evaluation
  fn repr(&self) -> NixResult<String> {
    let mut buf = String::new();
    self.repr_rec(&mut buf)?;
    Ok(buf)
  }
}

impl<'state> ToNix<'state> for NixTerm<'state> {
  fn to_nix(self, _eval_state: &'state NixEvalState) -> NixResult<NixTerm<'state>> {
    Ok(self)
  }
}

impl<'state> ToNix<'state> for RawValue<'state> {
  fn to_nix(self, _eval_state: &'state NixEvalState) -> NixResult<NixTerm<'state>> {
    let context = NixContext::default();
    let ctx = context.ptr();
    let value = self.value.as_ptr();
    let value_type = unsafe { get_type(ctx, value) };
    let res = match value_type {
      ValueType::NIX_TYPE_NULL => NixTerm::Null,
      ValueType::NIX_TYPE_INT => {
        let v = unsafe { get_int(ctx, value) };
        NixTerm::Int(v)
      },
      ValueType::NIX_TYPE_BOOL => {
        let b = unsafe { get_bool(ctx, value) };
        NixTerm::Bool(b)
      },
      ValueType::NIX_TYPE_FLOAT => {
        let f = unsafe { get_float(ctx, value) };
        NixTerm::Float(f)
      },
      ValueType::NIX_TYPE_STRING => {
        let mut raw_buffer: anyhow::Result<String> = Err(anyhow::format_err!("Nix C API didn't return a string."));
        unsafe {
          get_string(ctx, value, Some(callback_get_result_string), callback_get_result_string_data(&mut raw_buffer))
        };
        NixTerm::String(raw_buffer.map_err(|_| NixEvalError::InvalidString)?)
      },
      ValueType::NIX_TYPE_PATH => {
        let path = unsafe { get_path_string(ctx, value) };
        let path = unsafe { CStr::from_ptr(path) };
        let path = path.to_str().map_err(|_| NixEvalError::InvalidString)?;
        let path = PathBuf::from(path);
        NixTerm::Path(path)
      },
      ValueType::NIX_TYPE_LIST => NixTerm::List(NixList(self)),
      ValueType::NIX_TYPE_ATTRS => NixTerm::AttrSet(NixAttrSet(self)),
      ValueType::NIX_TYPE_FUNCTION => NixTerm::Function(NixFunction(self)),
      ValueType::NIX_TYPE_THUNK =>  NixTerm::Thunk(NixThunk(self)),
      ValueType::NIX_TYPE_EXTERNAL => todo!("Cannot handle external values yet"),
      _ => panic!("Unknown value type"),
    };
    context.check_call()?;
    Ok(res)
  }
}

/// Iterator over elements in a nix list
pub struct NixListIterator<'state, 'val: 'state> {
  pub len: u32,
  pub(crate) val: &'val NixList<'state>,
  pub(crate) idx: u32
}

/// Iterator over items in a nix attribute set
pub struct NixItemsIterator<'state, 'val: 'state> {
  pub len: u32,
  pub(crate) val: &'val NixAttrSet<'state>,
  pub(crate) idx: u32
}

/// Iterator over keys in a nix attribute set
pub struct NixNamesIterator<'state, 'val: 'state> {
  pub len: u32,
  pub(crate) val: &'val NixAttrSet<'state>,
  pub(crate) idx: u32
}

impl<'state> Repr for NixAttrSet<'state> {
  fn repr_rec(&self, s: &mut String) -> NixResult<()> {
    s.push('{');
    for (key, val) in self.items()? {
      let val = val?;
      s.push(' ');
      s.push_str(&key);
      s.push_str(" = ");
      if let NixTerm::List(_) = val {
        s.push_str(" [ ... ]");
      } else if let NixTerm::AttrSet(_) = val {
        s.push_str(" { ... }");
      } else {
        val.repr_rec(s)?;
      };
      s.push(';');
    }
    s.push_str(" }");
    Ok(())
  }
}

impl<'state> NixAttrSet<'state> {
  
  /// Tries to build an attribute set as if it was a derivation.
  /// 
  /// Throws [`NotADerivation`][NixEvalError] if the attrset is not a derivation
  pub fn build(&self) -> NixResult<HashMap<String, String>> {
    let term_type = self.get("type").map_err(|_| NixEvalError::NotADerivation)?;
    let NixTerm::String(s) = term_type else { return Err(NixEvalError::NotADerivation) };
    if &s == "derivation" {
      let drv = self.get("drvPath")?;
      let path = drv.as_string()?;
      let store_path = self.0._state.store.parse_path(&s).map_err(|_| NixEvalError::InvalidPath(path))?;
      self.0._state.store.build(&store_path)
    } else {
      Err(NixEvalError::NotADerivation)
    }
  }

  /// Gets an attribute from the underlying attribute set.
  ///
  /// Throws [`RuntimeError(KeyError)`][NixError] when the key doesn't exist.
  pub fn get(&self, name: &str) -> NixResult<NixTerm<'state>> {
    let ctx = &self.0._state.store.ctx;
    let state = self.0._state;
    let name = CString::new(name).map_err(|_| NixEvalError::InvalidString)?;
    let val = unsafe {
      get_attr_byname(ctx.ptr(), self.0.value.as_ptr(), state.state_ptr(), name.as_ptr())
    };
    ctx.check_call()?;
    let value = NonNull::new(val).expect("get_attr_by_name returned null");
    let rawvalue = RawValue {
      value,
      _state: state
    };
    rawvalue.to_nix(&self.0._state)
  }

  /// How many elements there are in the attribute set.
  pub fn len(&self) -> NixResult<u32> {
    let len = unsafe { get_attrs_size(self.0._state.store.ctx.ptr(), self.0.value.as_ptr()) };
    self.0._state.store.ctx.check_call()?;
    Ok(len)
  }

  /// Whether it's empty or not.
  pub fn is_empty(&self) -> NixResult<bool> {
    Ok(self.len()? == 0)
  }

  /// Returns an iterator over the keys of the attribute set.
  pub fn names(&self) -> NixResult<NixNamesIterator> {
    let iterator = NixNamesIterator {
      val: self, len: self.len()?, idx: 0
    };
    Ok(iterator)
  }

  /// Returns an iterator over the pairs `(String, NixTerm)` of the attribute set.
  pub fn items(&self) -> NixResult<NixItemsIterator> {
    let iterator = NixItemsIterator {
      val: self, len: self.len()?, idx: 0
    };
    Ok(iterator)
  }
}

impl<'state> NixThunk<'state> {
  /// Forces the evaluation of the thunk and resolves it into
  /// a non-thunk term.
  pub fn force(self) -> NixResult<NixTerm<'state>> {
    let rawvalue = &self.0;
    let context = rawvalue._state.store.ctx.ptr();
    let state = rawvalue._state.state_ptr();
    let value = rawvalue.value.as_ptr();
    unsafe {
      value_force(context, state, value)
    };
    rawvalue._state.store.ctx.check_call()?;
    rawvalue.clone().to_nix(&rawvalue._state)
  }
}

impl<'state> NixFunction<'state> {

  /// Calls the nix function with the argument converted to nix.
  pub fn call_with<T: ToNix<'state>>(&self, arg: T) -> NixResult<NixTerm<'state>> {
    let state = self.0._state.state_ptr();
    let arg = arg.to_nix(&self.0._state)?.to_raw_value(&self.0._state);
    let ret = RawValue::empty(self.0._state);
    let ctx = NixContext::default();
    unsafe {
      value_call(ctx.ptr(), state, self.0.value.as_ptr(), arg.value.as_ptr(), ret.value.as_ptr());
    }
    ctx.check_call()?;
    ret.to_nix(&self.0._state)
  }
}

impl<'state> NixList<'state> {

  /// How many elements are there in the list.
  pub fn len(&self) -> NixResult<u32> {
    let len = unsafe { get_list_size(self.0._state.store.ctx.ptr(), self.0.value.as_ptr()) };
    self.0._state.store.ctx.check_call()?;
    Ok(len)
  }

  /// Is the list empty?
  pub fn is_empty(&self) -> NixResult<bool> {
    Ok(self.len()? == 0)
  }

  /// Returns the iterator over the elements in a list
  pub fn iter(&self) -> NixResult<NixListIterator> {
    let iterator = NixListIterator {
      val: self, len: self.len()?, idx: 0
    };
    Ok(iterator)
  }

  /// Returns the element at idx `idx` or throws an `IndexOutOfBounds` error.
  pub fn get_idx(&self, idx: u32) -> NixResult<NixTerm> {
    let raw = &self.0;
    let size = self.len()?;
    if idx > size -1 {
      return Err(NixEvalError::IndexOutOfBounds)
    }
    let elem = unsafe { get_list_byidx(raw._state.store.ctx.ptr(), raw.value.as_ptr(), raw._state.state_ptr(), idx as c_uint) };
    let elem = NonNull::new(elem).expect("get_list_byidx returned null");
    let rawvalue = RawValue {
      _state: raw._state,
      value: elem
    };
    rawvalue.to_nix(&raw._state)
  }
}

impl<'state> Repr for NixList<'state> {
  fn repr_rec(&self, s: &mut String) -> NixResult<()> {
    s.push('[');
    for t in self.iter()? {
      let t = t?;
      s.push(' ');
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

impl<'state> NixTerm<'state> {

  /// Builds the term if the term is an attribute set, otherwise type error.
  pub fn build(&self) -> NixResult<HashMap<String, String>> {
    if let NixTerm::AttrSet(attrset) = self {
      attrset.build()
    } else {
      Err(NixEvalError::TypeError { expected: "attrset".to_string(), got: self.get_typename() })
    }
  }

  /// Returns the name of the type of the term.
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

  /// Returns the iterator over names if the element is an attrset, otherwise type error.
  pub fn names(&self) -> NixResult<NixNamesIterator> {
    if let NixTerm::AttrSet(attrset) = self {
      attrset.names()
    } else {
      Err(NixEvalError::TypeError { expected: "attrset".into(), got: self.get_typename() })
    }
  }

  /// Returns the iterator over items if the element is an attrset, otherwise type error.
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
  
  pub fn to_raw_value(self, _state: &'state NixEvalState) -> RawValue<'state> {
    let ctx = _state.store.ctx.ptr();
    let state = _state.state_ptr();
    let mut rawval = RawValue::empty(_state);
    let val_ptr = rawval.value.as_ptr();
    match self {
      NixTerm::External(raw)  => { rawval = raw; }
      NixTerm::Thunk(thunk) => { rawval = thunk.0; }
      NixTerm::List(list) => { rawval = list.0; }
      NixTerm::AttrSet(attrset) => { rawval = attrset.0; }
      NixTerm::Function(func) => { rawval = func.0; },
      NixTerm::Null =>  unsafe {
        init_null(ctx, val_ptr);
      }
      NixTerm::Int(i) => unsafe {
        init_int(ctx, val_ptr, i);
      }
      NixTerm::Float(f) => unsafe {
        init_float(ctx, val_ptr, f);
      }
      NixTerm::Bool(b) => unsafe {
        init_bool(ctx, val_ptr, b);
      }
      NixTerm::Path(p) => {
        let string = p.to_str().expect("path is not a valid string");
        let c_str = CString::new(string).expect("path is not a valid C String");
        unsafe {
          init_path_string(ctx, state, val_ptr, c_str.as_ptr());
        }
      },
      NixTerm::String(s) => {
        let c_str = CString::new(s.to_owned()).expect("path is not a valid C String");
        unsafe {
          init_string(ctx, val_ptr, c_str.as_ptr());
        }
      },
    };
    _state.store.ctx.check_call().expect("error transforming to raw value");
    rawval
  }
  
  pub fn call_with<T: ToNix<'state>>(self, arg: T) -> NixResult<NixTerm<'state>> {
    if let NixTerm::Function(func) = self {
      func.call_with(arg)
    } else {
      Err(NixEvalError::TypeError { expected: "function".into(), got: self.get_typename() })
    }
  }
  
  pub fn get(&self, name: &str) -> NixResult<NixTerm<'state>> {
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

impl<'state> Repr for NixTerm<'state> {
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

impl<'state, 'val: 'state> Iterator for NixListIterator<'val, 'state> {
  type Item = NixResult<NixTerm<'state>>;
  
  fn next(&mut self) -> Option<Self::Item> {
    if self.idx == self.len {
      return None;
    }
    let item = self.val.get_idx(self.idx);
    self.idx += 1;
    Some(item)
  }
}

impl<'state, 'val: 'state> Iterator for NixItemsIterator<'state, 'val> {
  type Item = (String, NixResult<NixTerm<'state>>);
  
  fn next(&mut self) -> Option<Self::Item> {
    if self.idx == self.len {
      return None;
    }
    let raw = &self.val.0;
    let mut name: *const c_char = std::ptr::null();
    let elem = unsafe { get_attr_byidx(
      raw._state.store.ctx.ptr(),
      raw.value.as_ptr(),
      raw._state.state_ptr(),
      self.idx as c_uint,
      &mut name
    )};
    self.idx += 1;
    let name = unsafe { CStr::from_ptr(name) }.to_str().expect("Nix returned an invalid string").to_owned();
    if let Err(err) = raw._state.store.ctx.check_call() {
      return Some((name, Err(err.into())));
    };
    let elem = NonNull::new(elem).expect("get_attr_byidx returned null");
    let rawvalue = RawValue {
      _state: raw._state,
      value: elem
    };
    Some((name, rawvalue.to_nix(&raw._state)))
  }
}

impl<'state, 'val: 'state> Iterator for NixNamesIterator<'state, 'val> {
  type Item = String;
  
  fn next(&mut self) -> Option<Self::Item> {
    if self.idx == self.len {
      return None;
    }
    let raw = &self.val.0;
    let ctx = &raw._state.store.ctx;
    let name = unsafe { get_attr_name_byidx(
      ctx.ptr(),
      raw.value.as_ptr(),
      raw._state.state_ptr(),
      self.idx as c_uint
    )};
    raw._state.store.ctx.check_call().expect("something went wrong");
    let name = unsafe { CStr::from_ptr(name) }.to_str().expect("Nix returned an invalid string").to_owned();
    self.idx += 1;
    Some(name)
  }
}

impl<'state> From<&str> for NixTerm<'state> {
  fn from(val: &str) -> Self {
    NixTerm::String(val.to_string())
  }
}

impl<'state> From<i64> for NixTerm<'state> {
  fn from(val: i64) -> Self {
    NixTerm::Int(val)
  }
}

impl<'state> From<PathBuf> for NixTerm<'state> {
  fn from(val: PathBuf) -> Self {
    NixTerm::Path(val)
  }
}

impl<'state> From<bool> for NixTerm<'state> {
  fn from(val: bool) -> Self {
    NixTerm::Bool(val)
  }
}

impl<'state, T: ToNix<'state>> ToNix<'state> for Vec<T> {
  fn to_nix(self, state: &'state NixEvalState) -> NixResult<NixTerm<'state>> {
    let ctx = state.store.ctx.ptr();
    let list_builder = unsafe {
      make_list_builder(ctx, state.state_ptr(), self.len())
    };
    for (idx, elem) in self.into_iter().enumerate() {
      let value = elem.to_nix(state)?.to_raw_value(state);
      unsafe {
        list_builder_insert(ctx, list_builder, idx as c_uint, value.value.as_ptr());
      }
    }
    let value = RawValue::empty(state);
    unsafe { make_list(ctx, list_builder, value.value.as_ptr()) };
    value.to_nix(state)
  }
}

impl<'state, S: AsRef<str>, T: ToNix<'state>> ToNix<'state> for HashMap<S, T> {
  fn to_nix(self, state: &'state NixEvalState) -> NixResult<NixTerm<'state>> {
    let ctx = state.store.ctx.ptr();
    let bindings_builder = unsafe {
      make_bindings_builder(ctx, state.state_ptr(), self.len())
    };
    state.store.ctx.check_call().unwrap();
    for (key, val) in self.into_iter() {
      let name = CString::new(key.as_ref()).expect("Key must be valid C string");
      let value = val.to_nix(state)?.to_raw_value(state);
      unsafe {
        bindings_builder_insert(ctx, bindings_builder, name.as_ptr(), value.value.as_ptr());
      }
      state.store.ctx.check_call().unwrap();
    }
    let ctx = NixContext::default();
    let value = RawValue::empty(state);
    unsafe { make_attrs(ctx.ptr(), value.value.as_ptr(), bindings_builder) };
    ctx.check_call().unwrap();
    unsafe { bindings_builder_free(bindings_builder); }
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
//       alloc_primop(context.ptr(),
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
//       init_primop(tmp_ctx.ptr(), value.value.as_ptr(), primop);
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
