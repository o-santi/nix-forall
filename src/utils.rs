use std::{collections::HashMap, ffi::{c_char, c_void, CStr}};

use crate::bindings::nix_version_get;

pub fn get_nix_version() -> String {
  unsafe {
    let version = nix_version_get();
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
