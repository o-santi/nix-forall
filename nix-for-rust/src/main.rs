use nix_for_rust::settings::NixSettings;

pub fn main() -> anyhow::Result<()> {
  let mut state = NixSettings::default()
    .with_setting("extra-experimental-features", "flakes")
    .with_default_store()?;
  let valid_pkgs = state.eval_flake("github:NixOS/nixpkgs")?
    .get("legacyPackages")?
    .get("x86_64-linux")?
    .items()?
    .filter_map(|(_name, term)| term.ok())
    .count();
  println!("Rejoice! You can build {valid_pkgs} packages from nixpkgs.");
  Ok(())
}

