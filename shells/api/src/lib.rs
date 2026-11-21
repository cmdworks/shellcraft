//! The interface every shell implements, and the helpers they share.
//!
//! A shell crate (`shells/<shell>`) implements [`ShellEmitter`]: its templates, tool-init
//! commands, key syntax and rc file. This crate holds what is the same for all of them: the trait
//! and rendering, the template context, the key parser, quoting, and the marked rc block.

mod colors;
mod context;
mod emitter;
pub mod keys;
mod quote;
pub mod rc;

pub use emitter::{config_home, var, EmitError, ShellEmitter};
pub use quote::quote_for;
pub use shellcraft_core::Shell as ShellKind;
