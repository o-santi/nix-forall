use nix_in_rust::{store::{NixStore, NixContext}, eval::NixEvalState, term::NixTerm};
use anyhow::Result;

pub fn main() -> Result<()> {
  let context = NixContext::default();
  let store = NixStore::new(context, "");
  let mut state = NixEvalState::new(store);
  let res = state.eval_from_string("./${toString (1 + 1)}")?;
  let term = NixTerm::from(res);
  println!("{term}");
  Ok(())
}
