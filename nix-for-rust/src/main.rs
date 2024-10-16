use nix_for_rust::{settings::NixSettings, term::{AttrSet, Repr}};

pub fn main() -> anyhow::Result<()> {
  let mut state = NixSettings::default_conf()?
    .with_setting("experimental-features", "flakes")
    .with_default_store()?;
  // let pkgs = state.eval_from_string("import <nixpkgs>", std::env::current_dir()?)?
  //   .call_with(AttrSet::default())?
  //   .items()?
  //   .filter_map(|(_, pkg)| pkg.ok())
  //   .count();
  let pkgs = state.eval_from_string("builtins.toJSON builtins.nixPath", std::env::current_dir()?)?;
  println!("{}", pkgs.repr()?);
  Ok(())
}

