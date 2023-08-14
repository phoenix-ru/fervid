//! This crate is used for generating the final Module code of the SFC.
//!
//! The main structure of this crate is [CodegenContext].

#[macro_use]
extern crate lazy_static;

mod atoms;
mod attributes;
mod comments;
mod components;
mod context;
mod control_flow;
mod directives;
mod interpolation;
mod elements;
mod imports;
mod text;
mod utils;

#[cfg(test)]
mod test_utils;

pub use context::CodegenContext;
