use std::{path::PathBuf, sync::{Arc, Mutex, MutexGuard}};

use nix_for_rust::term::{NixAttrSet, NixItemsIterator, NixNamesIterator, NixRealisedString, Repr};
use pyo3::{exceptions::PyAttributeError, prelude::*};
use anyhow::Result;

use crate::nix_term_to_py;


#[pyclass(frozen)]
#[derive(Clone)]
pub struct PyNixAttrSet(pub Arc<Mutex<NixAttrSet>>);
#[pyclass]
pub struct PyNixNamesIterator(Mutex<NixNamesIterator>);
#[pyclass]
pub struct PyNixItemsIterator(Mutex<NixItemsIterator>);

#[pyclass]
pub struct PyNixRealisedString(Mutex<NixRealisedString>);

// Safety: we can only access the rawpointers through the Mutex,
// which means that only one thread will have access to each at a time
unsafe impl Send for PyNixAttrSet {}
unsafe impl Send for PyNixNamesIterator {}
unsafe impl Send for PyNixItemsIterator {}
unsafe impl Send for PyNixRealisedString {}

impl PyNixAttrSet {
  pub fn lock(&self) -> MutexGuard<'_, NixAttrSet> {
    self.0.lock().expect("Another thread panic'd while holding the lock")
  }
}

#[pymethods]
impl PyNixAttrSet {

  fn __getattr__(&self, py: Python, name: &str) -> Result<PyObject> {
    let attrset = self.lock();
    let term = attrset.get(name).map_err(|_| PyAttributeError::new_err(name.to_string()))?;
    let obj = nix_term_to_py(py, term)?;
    Ok(obj)
  }

  fn realise(&self) -> Result<PyNixRealisedString> {
    let attrset = self.lock();
    let realised = attrset.realise()?;
    Ok(PyNixRealisedString(Mutex::new(realised)))
  }

  fn __getitem__(&self, py: Python, name: &str) -> Result<PyObject> {
    self.__getattr__(py, name)
  }

  fn __iter__(&self) -> Result<PyNixNamesIterator> {
    let attrset = self.lock();
    let names_iter = attrset.names()?;
    Ok(PyNixNamesIterator(Mutex::new(names_iter)))
  }

  fn keys(&self) -> Result<PyNixNamesIterator> {
    self.__iter__()
  }

  fn __len__(&self) -> Result<usize> {
    let attrset = self.lock();
    let len = attrset.len()?;
    Ok(len as usize)
  }

  fn items(&self) -> Result<PyNixItemsIterator> {
    let attrset = self.lock();
    let items_iter = attrset.items()?;
    Ok(PyNixItemsIterator(Mutex::new(items_iter)))
  }

  fn __repr__(&self) -> Result<String> {
    let attrset = self.lock();
    let repr = attrset.repr()?;
    Ok(format!("<PyNixAtrSet {repr}>"))
  }
}


#[pymethods]
impl PyNixNamesIterator {
  fn __len__(&self) -> Result<usize> {
    let iterator = self.0.lock().expect("Another thread panic'd while holding the lock");
    Ok(iterator.len as usize)
  }

  fn __iter__(slf: PyRef<'_, Self>) -> PyRef<Self> {
    slf
  }

  fn __next__(&mut self) -> PyResult<Option<String>> {
    let next = self.0.lock().expect("Another thread panic'd while holding the lock").next();
    Ok(next)
  }
}

#[pymethods]
impl PyNixItemsIterator {
  
  fn __len__(&self) -> Result<usize> {
    let iterator = self.0.lock().expect("Another thread panic'd while holding the lock");
    Ok(iterator.len as usize)
  }

  fn __iter__(slf: PyRef<'_, Self>) -> PyRef<Self> {
    slf
  }

  fn __next__(&mut self, py: Python) -> Result<Option<(String, PyObject)>> {
    let next = self.0.lock().expect("Another thread panic'd while holding the lock").next();
    if let Some((name, term)) = next {
      let term = term?;
      Ok(Some((name, nix_term_to_py(py, term)?)))
    } else {
      Ok(None)
    }
  }
}

impl PyNixRealisedString {
  pub fn lock(&self) -> MutexGuard<'_, NixRealisedString> {
    self.0.lock().expect("Another thread panic'd while holding the lock")
  }
}

#[pymethods]
impl PyNixRealisedString {

  #[getter]
  fn string(&self) -> String {
    self.lock().string.clone()
  }

  #[getter]
  fn paths(&self) -> Vec<PathBuf> {
    self.lock()
      .paths
      .iter()
      .map(|p| p.path.clone())
      .collect()
  }
}
