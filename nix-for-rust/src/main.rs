use nix_for_rust::{settings::NixSettings, term::AttrSet};

pub fn main() -> anyhow::Result<()> {
  let mut state = NixSettings::empty().with_default_store()?;
  let valid_pkgs = state.eval_from_string("import <nixpkgs>", std::env::current_dir()?)?
    .call_with(AttrSet::default())?
    .items()?
    .filter_map(|(_name, term)| term.ok())
    .count();
  println!("{valid_pkgs}");
  Ok(())
}

