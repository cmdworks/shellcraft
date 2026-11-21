//! macOS specifics.

use super::Manager;

/// Package managers to try, in order.
pub const MANAGERS: &[Manager] = &[Manager::Brew];

/// Homebrew `(prefix, repository)`: Apple silicon, then Intel.
pub const HOMEBREW: &[(&str, &str)] = &[
    ("/opt/homebrew", "/opt/homebrew"),
    ("/usr/local", "/usr/local/Homebrew"),
];

/// Release triple for tool downloads.
pub fn release_triple(arch: &str) -> Option<&'static str> {
    match arch {
        "aarch64" => Some("aarch64-apple-darwin"),
        "x86_64" => Some("x86_64-apple-darwin"),
        _ => None,
    }
}
