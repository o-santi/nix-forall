use pyo3::prelude::*;
use nix_for_rust::eval::NixEvalState;
use std::sync::{Arc, Mutex, MutexGuard};
use crate::{nix_term_to_py, store::PyNixStore};

#[derive(Clone)]
#[pyclass]
pub struct PyEvalState(pub Arc<Mutex<NixEvalState>>);

// Safety: we can only access the rawpointers through the Mutex,
// which means that only one thread will have access to each at a time
unsafe impl Send for PyEvalState {}

impl PyEvalState {
  fn lock(&self) -> MutexGuard<'_, NixEvalState> {
    self.0.lock().expect("Another thread panic'd while holding the lock")
  }
}

#[pymethods]
impl PyEvalState {

  #[getter]
  pub fn store(&self) -> anyhow::Result<PyNixStore> {
    Ok(PyNixStore(Arc::new(Mutex::new(self.lock().store.clone()))))
  }

  #[pyo3(signature=(string, cwd=None))]
  pub fn eval_string(&mut self, py: Python<'_>, string: &str, cwd: Option<std::path::PathBuf>)  -> anyhow::Result<PyObject> {
    let term = self.lock().eval_string(string, cwd.unwrap_or(std::env::current_dir()?))?;
    nix_term_to_py(py, term)
  }

  pub fn eval_file(&mut self, py:Python<'_>, file: std::path::PathBuf) -> anyhow::Result<PyObject> {
    let term = self.lock().eval_file(&file)?;
    nix_term_to_py(py, term)
  }

  pub fn eval_flake(&mut self, py:Python<'_>, flake_path: &str) -> anyhow::Result<PyObject> {
    let term = self.lock().eval_flake(flake_path)?;
    nix_term_to_py(py, term)
  }

  pub fn get_setting(&self, key: &str) -> Option<String> {
    self.lock().settings.get_setting(key)
  }
}
