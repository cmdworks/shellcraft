//! Command history with exit status, duration and directory: the log (`db`) and `shellcraft hist ...`.

mod cmd;
pub mod db;

pub use cmd::{run, HistCmd};
