//! # Nix For Rust
//!
//! Use nix values directly from rust.
pub mod eval;
pub mod store;
pub mod term;
pub mod error;
pub mod settings;
mod bindings;
mod utils;

pub use utils::get_nix_version;
