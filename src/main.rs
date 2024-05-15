use nix_in_rust::{store::{NixStore, NixContext}, eval::NixEvalState};
use anyhow::Result;

pub fn main() -> Result<()> {
  let context = NixContext::default();
  let store = NixStore::new(context, "");
  let mut state = NixEvalState::new(store);
  let nixpkgs = state.eval_from_string("
    import <nixpkgs> {}
  ")?;
  let drv = nixpkgs
    .get("hello")?;
  let outputs = drv.build()?;
  println!("{outputs:?}");
  Ok(())
}
