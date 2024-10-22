mod attrset;
mod list;
mod function;
mod nix_evaluator;

use std::{collections::HashMap, sync::{Arc, Mutex}};
use attrset::PyNixAttrSet;
use function::PyNixFunction;
use list::PyNixList;
use nix_evaluator::PyEvalState;
use pyo3::{prelude::*, types::{PyList, PyDict}};
use nix_for_rust::{eval::NixEvalState, settings::NixSettings, term::{NixEvalError, NixTerm, ToNix}};
use nix_for_rust::term::NixResult;

fn nix_term_to_py(py: Python, term: NixTerm) -> anyhow::Result<PyObject> {
  match term {
    NixTerm::Null => Ok(py.None()),
    NixTerm::String(s) => Ok(s.into_py(py)),
    NixTerm::Int(i) => Ok(i.into_py(py)),
    NixTerm::Float(f) => Ok(f.into_py(py)),
    NixTerm::Bool(b) => Ok(b.into_py(py)),
    NixTerm::Path(p) => Ok(p.into_py(py)),
    NixTerm::List(list) => Ok(PyNixList(Arc::new(Mutex::new(list))).into_py(py)),
    NixTerm::Function(func) => Ok(PyNixFunction(Arc::new(Mutex::new(func))).into_py(py)),
    NixTerm::AttrSet(attrset) => Ok(PyNixAttrSet(Arc::new(Mutex::new(attrset))).into_py(py)),
    NixTerm::Thunk(thunk) => nix_term_to_py(py, thunk.force()?),
    NixTerm::External(_) => anyhow::bail!("Cannot turn external nix value to python yet."),
  }
}

struct PyTerm<'s, 'gil>(&'s Bound<'gil, PyAny>);

impl<'s, 'gil> ToNix for PyTerm<'s, 'gil> {
  fn to_nix(self, eval_state: &NixEvalState) -> NixResult<NixTerm> {
    let obj = self.0;
    if obj.is_none() {
      Ok(NixTerm::Null)
    } else if let Ok(i) = obj.extract::<i64>() {
      Ok(NixTerm::Int(i))
    } else if let Ok(f) = obj.extract::<f64>() {
      Ok(NixTerm::Float(f))
    } else if let Ok(s) = obj.extract::<String>() {
      Ok(NixTerm::String(s))
    } else if let Ok(b) = obj.extract::<bool>() {
      Ok(NixTerm::Bool(b))
    } else if let Ok(l) = obj.downcast::<PyList>() {
      let items: Vec<NixTerm> = l
        .into_iter()
        .map(|p| PyTerm(&p).to_nix(eval_state))
        .collect::<nix_for_rust::term::NixResult<_>>()?;
      let term: NixTerm = items.to_nix(eval_state)?;
      Ok(term)
    } else if let Ok(d) = obj.downcast::<PyDict>() {
      let items: HashMap<String, NixTerm> = d
        .into_iter()
        .map(|(key, val)| {
          let Ok(key) = key.extract::<String>() else {
            return Err(NixEvalError::TypeError {
              expected: "string".to_string(),
              got: key.get_type().name().expect("Name shouldn't throw error").to_string()
            });
          };
          let val = PyTerm(&val).to_nix(eval_state)?;
          Ok((key, val))
        })
        .collect::<NixResult<_>>()?;
      let term: NixTerm = items.to_nix(eval_state)?;
      Ok(term)
    } else if let Ok(d) = obj.extract::<PyNixAttrSet>() {
      let attr = d.lock();
      Ok(NixTerm::AttrSet(attr.clone()))
    } else if let Ok(d) = obj.extract::<PyNixList>() {
      let list = d.lock();
      Ok(NixTerm::List(list.clone()))
    } else if let Ok(d) = obj.extract::<PyNixFunction>() {
      let func = d.lock();
      Ok(NixTerm::Function(func.clone()))
    } else {
      Err(NixEvalError::TypeError {
        expected: "A Nix term".to_string(),
        got: obj.get_type().name().expect("Name shouldn't throw error").to_string()
      })
    }
  }
}

#[pymodule]
mod nix_for_py {
  use super::*;
  
  #[pyfunction]
  #[pyo3(signature = (store="auto", lookup_path=None, store_params=None, settings=None))]
  fn nix_evaluator(
    store: &str,
    lookup_path: Option<Vec<String>>,
    store_params: Option<HashMap<String, String>>,
    settings: Option<HashMap<String, String>>
  ) -> PyResult<PyEvalState> {
    let nix_settings = NixSettings {
      settings: settings.unwrap_or_default(),
      store_params: store_params.unwrap_or_default(),
      lookup_path: lookup_path.unwrap_or_default()
    };
    let eval_state = nix_settings.with_store(store)?;
    Ok(PyEvalState(Arc::new(Mutex::new(eval_state))))
  }
}
