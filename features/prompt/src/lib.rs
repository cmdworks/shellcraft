//! The prompt: the renderer (`render`) and `shellcraft prompt` (`cmd`).

mod cmd;
pub mod render;

pub use cmd::{run, PromptArgs};
pub use render::{render, Input};
