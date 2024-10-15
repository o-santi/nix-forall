use std::sync::{Arc, Mutex, MutexGuard};

use nix_for_rust::term::NixFunction;
use pyo3::prelude::*;

use crate::{nix_term_to_py, py_to_nix_term};


#[pyclass(frozen)]
#[derive(Clone)]
pub struct PyNixFunction(pub Arc<Mutex<NixFunction>>);
// Safety: we can only access the rawpointers through the Mutex,
// which means that only one thread will have access to each at a time
unsafe impl Send for PyNixFunction {}

impl PyNixFunction {
  pub fn lock(&self) -> MutexGuard<'_, NixFunction> {
    self.0.lock().expect("Another thread panic'd while holding the mutex!")
  }
}

#[pymethods]
impl PyNixFunction {
  pub fn __call__(&self, obj: &Bound<'_, PyAny>) -> anyhow::Result<PyObject> {
    let function = self.0.lock().map_err(|e| anyhow::format_err!("{e}"))?;
    let term = py_to_nix_term(obj, &function.0._state)?;
    let ret = function.call_with(term)?;
    Python::with_gil(|py| nix_term_to_py(py, ret))
  }
}
