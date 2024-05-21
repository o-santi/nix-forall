use nix_in_rust::{eval_from_str, term::AttrSet};

pub fn main() -> anyhow::Result<()> {
  let pkgs = eval_from_str("import <nixpkgs>")?
    .call_with(AttrSet::default())?;
  let valid_pkgs = pkgs.items()?
    .filter_map(|(name, term)| term.ok().map(|t| (name, t)))
    .count();
  println!("{valid_pkgs}");
  Ok(())
}
