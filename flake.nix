{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nocargo = {
      url = "github:o-santi/nocargo";
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
    nix.url = "github:nixos/nix";
  };

  outputs = inputs @ { nixpkgs, flake-utils, nocargo, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        overlays = [ (import rust-overlay) nix-for-py-overlay ];
        inherit system;
      };
      nix = inputs.nix.packages.${system}.nix.libs;
      nix-deps = with nix; [ nix-store-c nix-expr-c nix-util-c nix-flake-c ];
      rust-tools = pkgs.rust-bin.stable.latest;
      rustPlatform = pkgs.makeRustPlatform {
        cargo = rust-tools.minimal;
        rustc = rust-tools.minimal;
      };
      bindgen_gcc_args = ''
        echo "Extending BINDGEN_EXTRA_CLANG_ARGS with system include paths..." 2>&1
        BINDGEN_EXTRA_CLANG_ARGS="$${BINDGEN_EXTRA_CLANG_ARGS:-}"
        include_paths=$(
          echo | $NIX_CC_UNWRAPPED -v -E -x c - 2>&1 \
          | awk '/#include <...> search starts here:/{flag=1;next} \
                /End of search list./{flag=0} \
                flag==1 {print $1}'
        )
        for path in $include_paths; do
          echo " - $path" 2>&1
          BINDGEN_EXTRA_CLANG_ARGS="$BINDGEN_EXTRA_CLANG_ARGS -I$path"
        done
      '';
      make-workspace-for-python = python: nocargo.lib.${system}.mkRustPackageOrWorkspace {
        src = ./.;
        rustc = rust-tools.minimal;
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
            LIBCLANG_PATH = lib.makeLibraryPath [ libclang ];
            buildInputs = [ ] ++ nix-deps;
            nativeBuildInputs = [ pkg-config pkgs.stdenv.cc ] ++ nix-deps;
          } // lib.optionalAttrs pkgs.stdenv.cc.isGNU {
            postConfigure = bindgen_gcc_args;
            BINDGEN_EXTRA_CLANG_ARGS="-x c++ -std=c++2a";
            # Avoid cc wrapper, because we only need to add the compiler/"system" dirs
            NIX_CC_UNWRAPPED = "${pkgs.stdenv.cc.cc}/bin/gcc";
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
        buildInputs = [
          python3
          gdb
          pkg-config
          libclang
          (rust-tools.default.override {
            extensions = ["rust-src" "rust-analyzer"];
          })
        ] ++ nix-deps;
        nativeBuildInputs = [
          rustPlatform.bindgenHook
        ];
      };
    });
}
