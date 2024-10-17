use pyo3::prelude::*;
use nix_for_rust::eval::NixEvalState;
use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use crate::{py_to_nix_term, nix_term_to_py};

#[derive(Clone)]
#[pyclass]
pub struct PyEvalState(pub Arc<Mutex<NixEvalState>>);

// Safety: we can only access the rawpointers through the Mutex,
// which means that only one thread will have access to each at a time
unsafe impl Send for PyEvalState {}

#[pymethods]
impl PyEvalState {

  pub fn eval_file(&mut self, py:Python<'_>, file: std::path::PathBuf) -> anyhow::Result<PyObject> {
    let mut state = self.0.lock().expect("Another thread panic'd while holding the lock");
    let term = state.eval_file(&file)?;
    nix_term_to_py(py, term)
  }

  pub fn eval_flake(&mut self, py:Python<'_>, flake_path: &str) -> anyhow::Result<PyObject> {
    let mut state = self.0.lock().expect("Another thread panic'd while holding the lock");
    let term = state.eval_flake(flake_path)?;
    nix_term_to_py(py, term)
  }
}
