use std::sync::{Arc, Mutex, MutexGuard};
use pyo3::{exceptions::PyIndexError, prelude::*};
use nix_for_rust::term::{NixList, NixListIterator, Repr};
use anyhow::Result;
use crate::nix_term_to_py;

#[pyclass(frozen)]
#[derive(Clone)]
pub struct PyNixList(pub Arc<Mutex<NixList>>);
#[pyclass]
pub struct PyNixListIterator(Mutex<NixListIterator>);

// Safety: we can only access the rawpointers through the Mutex,
// which means that only one thread will have access to each at a time
unsafe impl Send for PyNixList {}
unsafe impl Send for PyNixListIterator {}

impl PyNixList {
  pub fn lock(&self) -> MutexGuard<'_, NixList> {
    self.0.lock().expect("Another thread panic'd while holding the mutex!")
  }
}

#[pymethods]
impl PyNixList {

  fn __len__(&self) -> Result<usize> {
    let list = self.lock();
    let len = list.len()?;
    Ok(len as usize)
  }

  fn __iter__(&self) -> Result<PyNixListIterator> {
    let list = self.lock();
    let list_iter = list.iter()?;
    Ok(PyNixListIterator(Mutex::new(list_iter)))
  }

  fn __repr__(&self) -> Result<String> {
    let list = self.lock();
    let repr = list.repr()?;
    Ok(format!("<PyNixList {repr}>"))
  }

  fn __getitem__(&self, py: Python, item: u32) -> Result<PyObject> {
    let list = self.lock();
    let item = list.get_idx(item).map_err(|_| PyIndexError::new_err(item))?;
    nix_term_to_py(py, item)
  }
}

impl PyNixListIterator {
  fn lock(&self) -> Result<MutexGuard<'_, NixListIterator>> {
    self.0.lock().map_err(|e| anyhow::format_err!("{e}"))
  }
}

#[pymethods]
impl PyNixListIterator {
  
  fn __len__(&self) -> Result<usize> {
    let iterator = self.lock()?;
    Ok(iterator.len as usize)
  }

  fn __iter__(slf: PyRef<'_, Self>) -> PyRef<Self> {
    slf
  }

  fn __next__(&mut self, py: Python) -> Result<Option<PyObject>> {
    let next = self.lock()?.next();
    if let Some(term) = next {
      let term = term?;
      Ok(Some(nix_term_to_py(py, term)?))
    } else {
      Ok(None)
    }
  }
}
