use nix_in_rust::{eval_from_str, term::AttrSet};

pub fn main() -> anyhow::Result<()> {
  let pkgs = eval_from_str("import <nixpkgs>")?
    .call_with(AttrSet::default())?
    .items()?
    .filter_map(|(_, pkg)| pkg.ok())
    .count();
  println!("Rejoice! You can build {pkgs} packages from nixpkgs");
  Ok(())
}

