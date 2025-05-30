use nix_for_rust::settings::NixSettings;
use nix_for_rust::flakes::{FetchersSettings, FlakeLockFlags, FlakeRefSettings, FlakeSettings};
use nix_for_rust::term::Repr;

pub fn main() -> anyhow::Result<()> {
  let mut settings = FlakeSettings::new(FetchersSettings::new()?)?;

  let mut flags = FlakeRefSettings::new(settings)?;
  flags.set_basedir(&std::env::current_dir()?)?;
  let flake_ref = flags.parse("..#hello.nixosConfigurations")?;

  let mut nix = NixSettings::default()
    // .with_flakes(settings)
    .with_default_store()?;

  let mut settings = FlakeSettings::new(FetchersSettings::new()?)?;
  
  let locked = nix.lock_flake(flake_ref, FlakeLockFlags::new(&settings)?)?;
  
  println!("{}", locked.outputs()?.repr()?);
  Ok(())
}
