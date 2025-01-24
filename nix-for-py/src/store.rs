use pyo3::prelude::*;
use std::{collections::HashMap, path::PathBuf, sync::{Arc, Mutex, MutexGuard}};
use nix_for_rust::store::NixStore;
use anyhow::Result;

#[derive(Clone)]
#[pyclass]
pub struct PyNixStore(pub Arc<Mutex<NixStore>>);
unsafe impl Send for PyNixStore {}

impl PyNixStore {
  fn lock(&self) -> MutexGuard<'_, NixStore> {
    self.0.lock().expect("Another thread panic'd while holding the lock")
  }
}

#[pymethods]
impl PyNixStore {

  pub fn version(&self) -> Result<String> {
    self.lock().version()
  }

  pub fn build(&self, path: &str) -> Result<HashMap<String, String>> {
    let store = self.lock();
    let path = store.parse_path(path)?;
    store.build(&path)
      .map_err(|e| e.into())
  }

  pub fn is_valid_path(&self, path: &str) -> Result<bool> {
    let store = self.lock();
    let path = store.parse_path(path)?;
    store.is_valid_path(&path)
  }

  pub fn store_dir(&self) -> Result<PathBuf> {
    self.lock().store_dir()
  }

  pub fn copy_closure(&self, destination: &PyNixStore, path: &str) -> Result<()> {
    let store = self.lock();
    let path = store.parse_path(path)?;
    let dst = destination.lock();
    store.copy_closure(&dst, &path)
  }
}
