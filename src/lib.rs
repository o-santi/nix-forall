use std::ffi::c_char;
use std::ptr::copy_nonoverlapping;
use std::ffi::c_void;

pub mod eval;
pub mod store;
pub mod term;
mod bindings;
pub mod error;

unsafe extern "C" fn get_string_callback(input: *const c_char, len: u32, user_data: *mut c_void) {
  copy_nonoverlapping(input, user_data as *mut i8, len as usize);
}

unsafe extern "C" fn callback_get_vec_u8(
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
