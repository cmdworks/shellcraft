//! `ls`, `cat`, `files` and `cheat`: small replacements for eza, bat, fd and cheat sheets.

pub mod cat;
pub mod cheat;
pub mod files;
pub mod ls;

pub use cat::CatArgs;
pub use cheat::CheatArgs;
pub use files::FilesArgs;
pub use ls::LsArgs;
