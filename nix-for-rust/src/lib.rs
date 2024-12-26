//! # Nix For Rust
//!
//! Use nix values directly from rust.
//!
//! The entry point for nix evaluation is the [`NixSettings`][settings::NixSettings] builder,
//! which builds a Nix evaluator with the provided settings.
//! 
//! # Example
//! ```
//! use nix_for_rust::settings::NixSettings;
//! let mut state = NixSettings::default()
//!   .with_setting("experimental-features", "flakes")
//!   .with_default_store()?;
//! ```
//! After passing the [nix store](https://nix.dev/manual/nix/2.17/command-ref/nix-store) path, it will return
//! a [`NixEvalState`][eval::NixEvalState] instance, that can be used to evaluate code. 

pub mod eval;
pub mod store;
pub mod term;
pub mod error;
pub mod settings;
mod bindings;
mod utils;
#[cfg(feature="eval-cache")]
mod eval_cache;
#[cfg(feature="derivation")]
pub mod derivation;

pub use utils::get_nix_version;
