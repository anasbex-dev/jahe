//! Runtime services exported through JAHE's versioned C ABI.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod abi;
pub mod error;
pub mod io;
pub mod memory;
pub mod panic;

pub use abi::{ABI_PREFIX, ABI_VERSION};
pub use error::RuntimeStatus;
