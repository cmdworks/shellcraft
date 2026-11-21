//! Linux specifics.

use super::Manager;

/// Package managers to try, in order.
pub const MANAGERS: &[Manager] = &[Manager::Brew, Manager::Apt, Manager::Pacman];

/// Homebrew on Linux (`(prefix, repository)`).
pub const HOMEBREW: &[(&str, &str)] = &[(
    "/home/linuxbrew/.linuxbrew",
    "/home/linuxbrew/.linuxbrew/Homebrew",
)];

/// Release triple for tool downloads (static musl builds run on every distribution).
pub fn release_triple(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("x86_64-unknown-linux-musl"),
        "aarch64" => Some("aarch64-unknown-linux-musl"),
        _ => None,
    }
}
