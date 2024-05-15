# Nix in Rust

Use nix values from rust as if they were native, and vice-versa. An example program is given :

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
and when ran, it should give you the following output.
```
> nix run
{"out": "/nix/store/bw9z0jxp5qcm7jfp4vr6ci9qynjyaaip-hello-2.12.1"}
```

Behind the curtains, it uses Nix's C API, in order to provide fast and seamless integration betweens the two languages.
