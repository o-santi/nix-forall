use std::{collections::HashMap, ffi::{c_char, c_void, CStr}, path::PathBuf, ptr::NonNull};

use crate::{bindings::{c_context, copy_value, init_bool, init_int, value_force, version_get, EvalState, Value}, eval::{NixEvalState, RawValue, StateWrapper}, store::{NixContext, NixStore}, term::{NixEvalError, NixTerm, ToNix}};

pub fn get_nix_version() -> String {
  unsafe {
    let version = version_get();
    CStr::from_ptr(version)
      .to_str()
      .expect("nix version should be valid utf8")
      .to_owned()
  }
}

pub unsafe extern "C" fn callback_get_vec_u8(
    start: *const c_char,
    n: std::os::raw::c_uint,
    user_data: *mut c_void,
) {
    let ret = user_data as *mut Vec<u8>;
    let slice = std::slice::from_raw_parts(start as *const u8, n as usize);
    if !(*ret).is_empty() {
        panic!("callback_get_vec_u8: slice must be empty. Were we called twice?");
    }
    (*ret).extend_from_slice(slice);
}

pub extern "C" fn read_into_hashmap(map: *mut c_void, outname: *const c_char, out: *const c_char) {
  let map: &mut HashMap<String, String> = unsafe { std::mem::transmute(map) };
  let key = unsafe { CStr::from_ptr(outname)}.to_str().expect("nix key should be valid string");
  let path = unsafe { CStr::from_ptr(out)}.to_str().expect("nix path should be valid string");
  map.insert(key.to_string(), path.to_string());
}

// pub unsafe extern "C" fn call_rust_closure<F>(
//   func: *mut c_void,
//   context: *mut c_context,
//   state: *mut EvalState,
//   args: *mut *mut Value,
//   mut ret: *mut Value
// )
// where F: Fn(NixTerm) -> Result<NixTerm, NixEvalError> {
//   let closure: &Box<F> = std::mem::transmute(func);
//   let ctx = NixContext { _ctx: NonNull::new(context).expect("context should never be null") };
//   let store = NixStore::new(ctx, "");
//   let state = NonNull::new(state).expect("state should never be null");
//   let value = {
//     value_force(state.store.ctx.ptr(), state.state_ptr(), *args);
//     NonNull::new(*args).expect("Expected at least one argument")
//   };
//   state.store.ctx.check_call().unwrap();
//   let rawvalue = RawValue {
//     value,
//     _state: state.clone()
//   };
//   let argument: NixTerm = rawvalue.to_nix(&state).unwrap();
//   let func_ret: NixTerm = closure(argument).expect("Closure returned an error");
//   let rawvalue: RawValue = func_ret.to_raw_value(&state);
//   unsafe {
//     copy_value(state.store.ctx.ptr(), ret, rawvalue.value.as_ptr());
//   }
//   state.store.ctx.check_call().unwrap()
// }

pub fn eval_from_str(str: &str, cwd: PathBuf) -> anyhow::Result<NixTerm> {
  let context = NixContext::default();
  let store = NixStore::new(context, "");
  let mut state = NixEvalState::new(store);
  state.eval_from_string(str, cwd)
}
