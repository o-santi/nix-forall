use std::{collections::HashMap, ffi::{c_char, c_void, CStr}};
use anyhow::Result;
use crate::bindings::version_get;

pub fn get_nix_version() -> String {
  unsafe {
    let version = version_get();
    CStr::from_ptr(version)
      .to_str()
      .expect("nix version should be valid utf8")
      .to_owned()
  }
}

// taken from https://github.com/nixops4/nixops4/blob/main/rust/nix-util/src/string_return.rs
pub unsafe extern "C" fn callback_get_result_string(
    start: *const ::std::os::raw::c_char,
    n: std::os::raw::c_uint,
    user_data: *mut std::os::raw::c_void,
) {
    let ret = user_data as *mut Result<String>;

    if start == std::ptr::null() {
        if n != 0 {
            panic!("callback_get_result_string: start is null but n is not zero");
        }
        *ret = Ok(String::new());
        return;
    }

    let slice = std::slice::from_raw_parts(start as *const u8, n as usize);

    if !(*ret).is_err() {
        panic!(
            "callback_get_result_string: Result must be initialized to Err. Did Nix call us twice?"
        );
    }

    *ret = String::from_utf8(slice.to_vec())
        .map_err(|e| anyhow::format_err!("Nix string is not valid UTF-8: {}", e));
}

// taken from https://github.com/nixops4/nixops4/blob/main/rust/nix-util/src/string_return.rs
pub fn callback_get_result_string_data(vec: &mut Result<String>) -> *mut std::os::raw::c_void {
    vec as *mut Result<String> as *mut std::os::raw::c_void
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
