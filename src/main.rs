use nix_in_rust::eval;

pub fn main() -> anyhow::Result<()> {
  let nixpkgs = eval("import <nixpkgs> {}")?;
  let drv = nixpkgs
    .get("hello")?;
  let outputs = drv.build()?;
  println!("{outputs:?}");
  Ok(())
}
