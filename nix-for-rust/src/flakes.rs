use std::{path::Path, ptr::{null_mut, NonNull}};
use anyhow::Result;
use nix::NixPath;

use crate::bindings::{fetchers_settings, fetchers_settings_free, fetchers_settings_new, flake_lock, flake_lock_flags, flake_lock_flags_add_input_override, flake_lock_flags_free, flake_lock_flags_new, flake_lock_flags_set_mode_check, flake_lock_flags_set_mode_virtual, flake_reference, flake_reference_and_fragment_from_string, flake_reference_free, flake_reference_parse_flags, flake_reference_parse_flags_new, flake_reference_parse_flags_set_base_directory, flake_settings, flake_settings_free, flake_settings_new, locked_flake, locked_flake_free, locked_flake_get_output_attrs, setting_set};
  use crate::{eval::{NixEvalState, RawValue}, store::NixContext, term::{NixTerm, ToNix}, utils::{callback_get_result_string, callback_get_result_string_data}};

#[derive(Clone)]
pub struct FetchersSettings {
  ptr: NonNull<fetchers_settings>
}

impl FetchersSettings {
  pub fn new() -> Result<Self> {
    let ptr = NixContext::non_null(|ctx| unsafe {
      fetchers_settings_new(ctx.ptr())
    })?;
    Ok(FetchersSettings { ptr })
  }
}

impl Drop for FetchersSettings {
  fn drop(&mut self) {
    unsafe {
      fetchers_settings_free(self.ptr.as_mut());
    }
  }
}

pub struct FlakeRefSettings {
  ptr: NonNull<flake_reference_parse_flags>,
  settings: FlakeSettings,
}

impl FlakeRefSettings {
  pub fn new(settings: FlakeSettings) -> Result<Self> {
    let ptr = NixContext::non_null(move |ctx| unsafe {
      flake_reference_parse_flags_new(ctx.ptr(), settings.settings_ptr.as_ptr())
    })?;
    Ok(FlakeRefSettings { ptr, settings })
  }

  pub fn set_basedir(&mut self, dir: &Path) -> Result<()> {
    let dir_len = dir.len();
    let dir_bytes  = dir.as_os_str().as_encoded_bytes().as_ptr();
    NixContext::checking(|ctx| unsafe {
      flake_reference_parse_flags_set_base_directory(
        ctx.ptr(),
        self.ptr.as_mut(),
        dir_bytes as *const i8,
        dir_len);
    })
  }

  pub fn parse(self, uri: &str) -> Result<FlakeRef> {
    let mut fragment: Result<String> = Err(anyhow::anyhow!("Nix C API didn't return a string."));
    let mut ptr: *mut flake_reference = null_mut();
    NixContext::checking(|ctx| unsafe {
      flake_reference_and_fragment_from_string(
        ctx.ptr(),
        self.settings.fetchers_settings.ptr.as_ptr(),
        self.settings.settings_ptr.as_ptr(),
        self.ptr.as_ptr(),
        uri.as_ptr() as *const i8,
        uri.len(),
        &mut ptr,
        Some(callback_get_result_string),
        callback_get_result_string_data(&mut fragment))
    })?;
    let ref_ptr = NonNull::new(ptr)
      .ok_or(anyhow::format_err!("flake_reference_and_fragment_from_string returned null"))?;
    Ok(FlakeRef {
      settings: self,
      fragment: fragment?,
      ref_ptr
    })
  }
}

pub struct FlakeRef {
  ref_ptr: NonNull<flake_reference>,
  pub settings: FlakeRefSettings,
  pub fragment: String,
}

impl Drop for FlakeRef {
  fn drop(&mut self) {
    unsafe {
      flake_reference_free(self.ref_ptr.as_mut())
    }
  }
}

#[derive(Clone)]
pub struct FlakeSettings {
  pub(crate) settings_ptr: NonNull<flake_settings>,
  pub(crate) fetchers_settings: FetchersSettings
}

impl FlakeSettings {
  pub fn new(fetchers_settings: FetchersSettings) -> Result<Self> {
    let settings_ptr = NixContext::non_null(|ctx| unsafe {

      flake_settings_new(ctx.ptr())
    })?;
    // TODO: is this really necessary? doesn't make much sense to me
    NixContext::checking(|ctx| unsafe {
      let key = "experimental-features".to_string();
      let val = "flakes".to_string();
      setting_set(ctx.ptr(), key.as_ptr() as *const i8, val.as_ptr() as *const i8)
    })?;
    Ok(FlakeSettings { settings_ptr, fetchers_settings })
  }
}

impl Drop for FlakeSettings {
  fn drop(&mut self) {
    unsafe {
      flake_settings_free(self.settings_ptr.as_ptr());
    }
  }
}

pub struct FlakeLockFlags {
  ptr: NonNull<flake_lock_flags>
}

impl FlakeLockFlags {
  pub fn new(flake_settings: &FlakeSettings) -> Result<Self> {
    let ptr = NixContext::non_null(|ctx| unsafe {
      flake_lock_flags_new(ctx.ptr(), flake_settings.settings_ptr.as_ptr())
    })?;
    Ok(FlakeLockFlags { ptr })
  }

  pub fn check_up_to_date(&mut self) -> Result<()> {
    NixContext::checking(|ctx| unsafe {
      flake_lock_flags_set_mode_check(ctx.ptr(), self.ptr.as_ptr());
    })
  }

  pub fn update_in_memory(&mut self) -> Result<()> {
    NixContext::checking(|ctx| unsafe {
      flake_lock_flags_set_mode_virtual(ctx.ptr(), self.ptr.as_ptr());
    })
  }

  pub fn write_as_needed(&mut self) -> Result<()> {
    NixContext::checking(|ctx| unsafe {
      flake_lock_flags_set_mode_check(ctx.ptr(), self.ptr.as_ptr());
    })
  }
  
  pub fn add_input_override(&mut self, input_path: &str, flake_ref: &FlakeRef) -> Result<()> {
    NixContext::checking(|ctx| unsafe {
      flake_lock_flags_add_input_override(ctx.ptr(), self.ptr.as_ptr(), input_path.as_ptr() as *const i8, flake_ref.ref_ptr.as_ptr());
    })
  }

}

impl Drop for FlakeLockFlags {
  fn drop(&mut self) {
    unsafe {
      flake_lock_flags_free(self.ptr.as_ptr());
    }
  }
}


pub struct LockedFlake<'state> {
  ptr: NonNull<locked_flake>,
  state: &'state NixEvalState,
  pub flags: FlakeLockFlags,
  pub flake_ref: FlakeRef
}

impl<'state> LockedFlake<'state> {
  pub fn outputs(&self) -> Result<NixTerm> {
    let value = NixContext::non_null(|ctx| unsafe {
      locked_flake_get_output_attrs(
        ctx.ptr(),
        self.flake_ref.settings.settings.settings_ptr.as_ptr(),
        self.state.state_ptr(),
        self.ptr.as_ptr())
    })?;
    let raw_value = RawValue {
      _state: self.state,
      value
    };
    raw_value.to_nix(&self.state)
      .map_err(|e| e.into())
  }
}

impl<'state> Drop for LockedFlake<'state> {
  fn drop(&mut self) {
    unsafe {
      locked_flake_free(self.ptr.as_ptr());
    }
  }
}

impl NixEvalState {

  pub fn flake_settings(&self) -> Result<&FlakeSettings> {
    self.settings.flake_settings
      .as_ref()
      .ok_or(anyhow::format_err!("NixEvalState was not initialized with flakes enabled."))
  }
  
  pub fn lock_flake<'state>(&'state self, flake_ref: FlakeRef, lock_flags: FlakeLockFlags) -> Result<LockedFlake<'state>> {
    let ptr = NixContext::non_null(|ctx| unsafe {
      flake_lock(
        ctx.ptr(),
        flake_ref.settings.settings.fetchers_settings.ptr.as_ptr(),
        flake_ref.settings.settings.settings_ptr.as_ptr(),
        self.state_ptr(),
        lock_flags.ptr.as_ptr(),
        flake_ref.ref_ptr.as_ptr())
    })?;
    Ok(LockedFlake { ptr, flags: lock_flags, flake_ref, state: self })
  }
  
}
