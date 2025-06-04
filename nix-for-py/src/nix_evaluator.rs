use pyo3::prelude::*;
use nix_for_rust::eval::NixEvalState;
use std::sync::{Arc, Mutex, MutexGuard};
use crate::{nix_term_to_py, store::PyNixStore};

#[derive(Clone)]
#[pyclass]
pub struct PyEvalState(pub Arc<Mutex<&'static NixEvalState>>);

// Safety: we can only access the rawpointers through the Mutex,
// which means that only one thread will have access to each at a time
unsafe impl Send for PyEvalState {}

impl PyEvalState {
  fn lock(&self) -> MutexGuard<'_, &'static NixEvalState> {
    self.0.lock().expect("Another thread panic'd while holding the lock")
  }
}

#[pymethods]
impl PyEvalState {

  #[getter]
  pub fn store(&self) -> anyhow::Result<PyNixStore> {
    Ok(PyNixStore(Arc::new(Mutex::new(&self.lock().store))))
  }

  #[pyo3(signature=(string, cwd=None))]
  pub fn eval_string(&self, py: Python<'_>, string: &str, cwd: Option<std::path::PathBuf>)  -> anyhow::Result<PyObject> {
    let term = self.lock().eval_string(string, cwd.unwrap_or(std::env::current_dir()?))?;
    nix_term_to_py(py, term)
  }

  pub fn eval_file(&self, py:Python<'_>, file: std::path::PathBuf) -> anyhow::Result<PyObject> {
    let term = self.lock().eval_file(&file)?;
    nix_term_to_py(py, term)
  }

  // pub fn eval_flake(&self, py:Python<'_>, flake_path: &str) -> anyhow::Result<PyObject> {
  //   let term = self.lock().eval_flake(flake_path)?;
  //   nix_term_to_py(py, term)
  // }

  pub fn get_setting(&self, key: &str) -> Option<String> {
    self.lock().settings.get_setting(key)
  }

  pub fn eval_attr_from_file(&self, file: std::path::PathBuf, accessor_path: Vec<String>) -> anyhow::Result<String> {
    self.lock().eval_attr_from_file(file, accessor_path)
      .map_err(|e| e.into())
  }

  pub fn builtins(&self, py:Python<'_>) -> anyhow::Result<PyObject> {
    let term = self.lock().builtins()?;
    nix_term_to_py(py, term)
  }
}
