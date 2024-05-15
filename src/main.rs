use std::collections::HashMap;

use nix_in_rust::{eval_from_str, term::NixTerm};

pub fn main() -> anyhow::Result<()> {
  let drv = eval_from_str("import <nixpkgs>")?
    .call_with(HashMap::<&str, NixTerm>::new())?
    .get("hello")?
    .get("outPath")?;
  
  println!("{drv}");
  Ok(())
}
