use std::sync::{Arc, Mutex, MutexGuard};

use nix_for_rust::term::{NixFunction, NixTerm};
use pyo3::{prelude::*, types::PyTuple};

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
  #[pyo3(signature=(*args))]
  pub fn __call__(&self, args: &Bound<'_, PyTuple>) -> anyhow::Result<PyObject> {
    let function = self.0.lock().map_err(|e| anyhow::format_err!("{e}"))?;
    let mut ret = NixTerm::Function(function.clone());
    let mut args = args.iter();
    while let Some(arg) = args.next() {
      let f: NixFunction = match ret {
        NixTerm::Function(f) => Ok::<NixFunction, anyhow::Error>(f),
        _ => anyhow::bail!("Cannot call non-function argument")
      }?;
      let arg = py_to_nix_term(&arg, &f.0._state)?;
      ret = f.call_with(arg)?;
    }
    Python::with_gil(|py| nix_term_to_py(py, ret))
  }
}
