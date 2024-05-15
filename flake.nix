{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
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
      inputs.flake-utils.follows = "flake-utils";
    };
    nix.url = "github:nixos/nix";
  };

  outputs = inputs @ { nixpkgs, flake-utils, nocargo, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      nix = inputs.nix.packages.${system}.default;
      pkgs = import nixpkgs {
        overlays = [ (import rust-overlay) ];
        inherit system;
      };
      ws = nocargo.lib.${system}.mkRustPackageOrWorkspace {
        src = ./.;
        rustc = rust-overlay.packages.${system}.rust;
        buildCrateOverrides = with pkgs; let libclang = llvmPackages_18.libclang.lib; in {
          "nix-in-rust" = old: {
            preBuild = ''
              # From: https://github.com/NixOS/nixpkgs/blob/1fab95f5190d087e66a3502481e34e15d62090aa/pkgs/applications/networking/browsers/firefox/common.nix#L247-L253
              # Set C flags for Rust's bindgen program. Unlike ordinary C
              # compilation, bindgen does not invoke $CC directly. Instead it
              # uses LLVM's libclang. To make sure all necessary flags are
              # included we need to look in a few places.
              export BINDGEN_EXTRA_CLANG_ARGS="$(< ${stdenv.cc}/nix-support/libc-crt1-cflags) \
                $(< ${stdenv.cc}/nix-support/libc-cflags) \
                $(< ${stdenv.cc}/nix-support/cc-cflags) \
                $(< ${stdenv.cc}/nix-support/libcxx-cxxflags) \
                ${lib.optionalString stdenv.cc.isClang "-idirafter ${stdenv.cc.cc}/lib/clang/${lib.getVersion stdenv.cc.cc}/include"} \
                ${lib.optionalString stdenv.cc.isGNU "-isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc} -isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc}/${stdenv.hostPlatform.config} -idirafter ${stdenv.cc.cc}/lib/gcc/${stdenv.hostPlatform.config}/${lib.getVersion stdenv.cc.cc}/include"} \
              "
            '';
            LIBCLANG_PATH = "${libclang}/lib";
            buildInputs = [ libclang nix ];
            nativeBuildInputs = [ pkg-config ];
          };
        };
      };
    in rec {
      apps.default = {
        type = "app";
        program = "${packages.default}/bin/nix_in_rust";
      };
      packages = {
        inherit (ws.release) nix-in-rust;
        default = ws.dev.nix-in-rust.bin; # release is segfaulting for some reason
      };
      devShells.default = with pkgs; 
        let libclang = llvmPackages_18.libclang.lib; in
        mkShell {
          LIBCLANG_PATH = "${libclang}/lib";
          buildInputs = [
            pkg-config
            nix
            libclang
            (rust-bin.stable.latest.default.override {
              extensions = ["rust-src" "rust-analyzer"];
            })
          ];
          shellHook=''
          export BINDGEN_EXTRA_CLANG_ARGS="$(< ${stdenv.cc}/nix-support/libc-crt1-cflags) \
            $(< ${stdenv.cc}/nix-support/libc-cflags) \
            $(< ${stdenv.cc}/nix-support/cc-cflags) \
            $(< ${stdenv.cc}/nix-support/libcxx-cxxflags) \
            ${lib.optionalString stdenv.cc.isClang "-idirafter ${stdenv.cc.cc}/lib/clang/${lib.getVersion stdenv.cc.cc}/include"} \
            ${lib.optionalString stdenv.cc.isGNU "-isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc} -isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc}/${stdenv.hostPlatform.config} -idirafter ${stdenv.cc.cc}/lib/gcc/${stdenv.hostPlatform.config}/${lib.getVersion stdenv.cc.cc}/include"} \
          "
          '';
        };
    });
}
