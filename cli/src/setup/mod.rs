//! Config and shell integration: `init`, `apply`, `uninstall`, `setup` (wizard) and `theme`.

pub mod apply;
pub mod init;
pub mod theme;
#[cfg(feature = "tui")]
pub mod tui;
pub mod uninstall;
pub mod wizard;
