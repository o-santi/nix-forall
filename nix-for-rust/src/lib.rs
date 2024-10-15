pub mod eval;
pub mod store;
pub mod term;
pub mod error;
pub mod bindings;
mod utils;

pub use utils::{get_nix_version, eval_from_str};
