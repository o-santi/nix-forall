use nix_in_rust::{eval_from_str, term::AttrSet};

pub fn main() -> anyhow::Result<()> {
  let pkgs = eval_from_str("import <nixpkgs>", std::env::current_dir()?)?
    .call_with(AttrSet::default())?
    .items()?
    .filter_map(|(_, pkg)| pkg.ok())
    .count();
  println!("Rejoice! You can build {pkgs} packages from nixpkgs");
  Ok(())
}

