//! Frecency directory jumping: the database (`db`) and `shellcraft jump ...` (`cmd`).

mod cmd;
pub mod db;

pub use cmd::{run, JumpCmd};
