# Nix in Rust

Use nix values from rust as if they were native, and vice-versa.

```rs
use nix_in_rust::eval;

pub fn main() -> anyhow::Result<()> {
  let nixpkgs = eval("import <nixpkgs> {}")?;
  let drv = nixpkgs
    .get("hello")?;
  let outputs = drv.build()?;
  println!("{outputs:?}");
  Ok(())
}
```
Should generate the following output:
```
{"out": "/nix/store/bw9z0jxp5qcm7jfp4vr6ci9qynjyaaip-hello-2.12.1"}
```
