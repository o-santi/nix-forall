{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
    nocargo = {
      url = "gitlab:deltaex/nocargo";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.registry-crates-io.follows = "registry-crates-io";
    };
    registry-crates-io = {
      url = "github:rust-lang/crates.io-index";
      flake = false;
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix.url = "github:nixos/nix/2.27.0";
  };

  outputs = inputs @ { nixpkgs, flake-utils, nocargo, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        overlays = [ (import rust-overlay) nix-for-py-overlay ];
        inherit system;
      };
      nix = inputs.nix.packages.${system};
      nix-deps = with nix; [ nix-store-c nix-expr-c nix-util-c ];
      bindgen_args = with pkgs; ''
          export BINDGEN_EXTRA_CLANG_ARGS="$(< ${stdenv.cc}/nix-support/libc-crt1-cflags) \
            $(< ${stdenv.cc}/nix-support/libc-cflags) \
            $(< ${stdenv.cc}/nix-support/cc-cflags) \
            $(< ${stdenv.cc}/nix-support/libcxx-cxxflags) \
            ${lib.optionalString stdenv.cc.isClang "-idirafter ${stdenv.cc.cc}/lib/clang/${lib.getVersion stdenv.cc.cc}/include"} \
            ${lib.optionalString stdenv.cc.isGNU "-isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc} -isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc}/${stdenv.hostPlatform.config} -idirafter ${stdenv.cc.cc}/lib/gcc/${stdenv.hostPlatform.config}/${lib.getVersion stdenv.cc.cc}/include"} \
          "
          '';
      libclang = pkgs.llvmPackages_18.libclang.lib;
      make-workspace-for-python = python: nocargo.lib.${system}.mkRustPackageOrWorkspace {
        src = ./.;
        rustc = pkgs.rust-bin.stable.latest.minimal;
        buildCrateOverrides = with pkgs; {
          "pyo3-build-config 0.22.4 (registry+https://github.com/rust-lang/crates.io-index)" = old: {
            nativeBuildInputs = [ python ];
            propagatedBuildInputs = [ python ];
          };
          "zerovec-derive 0.10.3 (registry+https://github.com/rust-lang/crates.io-index)" = old: {
            procMacro = true;
          };
          "yoke-derive 0.7.5 (registry+https://github.com/rust-lang/crates.io-index)" = old: {
            procMacro = true;
          };
          "zerofrom-derive 0.1.5 (registry+https://github.com/rust-lang/crates.io-index)" = old: {
            procMacro = true;
          };
          "doctest-file 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)" = old: {
            procMacro = true;
          };
          "nix-for-rust" = old: {
            preBuild = bindgen_args;
            LIBCLANG_PATH = "${libclang}/lib";
            buildInputs = [ libclang libseccomp ] ++ nix-deps;
            nativeBuildInputs = [ pkg-config ] ++ nix-deps;
          };
          "nix-for-py" = old: {
            nativeBuildInputs = [ pkg-config ] ++ nix-deps;
          };
        };
      };
      nix-for-py = { buildPythonPackage, lib, python, system, setuptools }:
        buildPythonPackage {
          pname = "nix_for_py";
          version = "0.0.1";
          pyproject = false;
          src = (make-workspace-for-python python).release.nix-for-py;
          doCheck = false;
          postInstall = ''
            mkdir -p $out/${python.sitePackages}
            cp $src/lib/*.so $out/${python.sitePackages}/nix_for_py.so
          '';
        };
      nix-for-py-overlay = self: super: {
        pythonPackagesExtensions = super.pythonPackagesExtensions ++ [(python-final: python-prev: {
          nix-for-py = python-final.callPackage nix-for-py {};
        })];
      };
      ws = make-workspace-for-python pkgs.python312;
    in rec {
      apps.default = {
        type = "app";
        program = "${packages.nix-for-rust.bin}/bin/nix_for_rust";
      };
      packages = {
        inherit (ws.release) nix-for-rust nix-for-py;
        default = packages.nix-for-rust.bin;
      };
      overlays = rec {
        default = nix-for-py-overlay;
        inherit nix-for-py-overlay;
      };
      devShells.default = with pkgs; mkShell {
        LIBCLANG_PATH = "${libclang}/lib";
        buildInputs = [
          # (python3.withPackages (p: [ p.nix-for-py ]))
          libseccomp
          gdb
          pkg-config
          libclang
          (rust-bin.stable.latest.default.override {
            extensions = ["rust-src" "rust-analyzer"];
          })
        ] ++ nix-deps;
        shellHook=bindgen_args;
      };
    });
}
