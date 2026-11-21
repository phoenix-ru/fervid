//! This crate is used for generating the final Module code of the SFC.
//!
//! The main structure of this crate is [CodegenContext].

#[macro_use]
extern crate lazy_static;

mod atoms;
mod attributes;
mod comments;
mod context;
mod control_flow;
mod directives;
mod imports;
mod interpolation;
mod text;
mod utils;
mod vnode;

#[cfg(test)]
mod test_utils;

pub use context::CodegenContext;
