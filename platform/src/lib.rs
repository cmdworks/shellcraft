//! Everything that differs by operating system, in one place: one file per OS.
//!
//! The functions are pure (they take the OS or CPU name as input) so every OS module is tested on
//! every machine. `current()`, `release_triple()` and `homebrew_prefixes()` use the OS we run on.

pub mod linux;
pub mod macos;

/// System package managers shellcraft can drive to install tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Brew,
    Apt,
    Pacman,
}

impl Manager {
    /// Program whose presence on PATH means this manager is available.
    pub fn binary(self) -> &'static str {
        match self {
            Self::Brew => "brew",
            Self::Apt => "apt-get",
            Self::Pacman => "pacman",
        }
    }

    /// Command (before the package name) that installs one package.
    pub fn install_command(self) -> &'static [&'static str] {
        match self {
            Self::Brew => &["brew", "install"],
            Self::Apt => &["sudo", "apt-get", "install", "-y"],
            Self::Pacman => &["sudo", "pacman", "-S", "--needed", "--noconfirm"],
        }
    }
}

/// Package managers worth trying on `os`, in order of preference.
pub fn package_managers_for(os: Os) -> &'static [Manager] {
    match os {
        Os::MacOs => macos::MANAGERS,
        Os::Linux => linux::MANAGERS,
        Os::Other => &[],
    }
}

/// Operating systems shellcraft knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    Other,
}

impl Os {
    /// Parse a `std::env::consts::OS` value.
    pub fn parse(name: &str) -> Self {
        match name {
            "macos" => Self::MacOs,
            "linux" => Self::Linux,
            _ => Self::Other,
        }
    }
}

/// The OS this binary runs on.
pub fn current() -> Os {
    Os::parse(std::env::consts::OS)
}

/// Rust target triple used in tool release asset names for `os` and `arch`.
pub fn release_triple_for(os: Os, arch: &str) -> Option<&'static str> {
    match os {
        Os::MacOs => macos::release_triple(arch),
        Os::Linux => linux::release_triple(arch),
        Os::Other => None,
    }
}

/// Release triple for the machine we run on.
pub fn release_triple() -> Option<&'static str> {
    release_triple_for(current(), std::env::consts::ARCH)
}

/// Directories where Homebrew may live on `os`, as `(prefix, repository)` pairs.
pub fn homebrew_prefixes_for(os: Os) -> &'static [(&'static str, &'static str)] {
    match os {
        Os::MacOs => macos::HOMEBREW,
        Os::Linux => linux::HOMEBREW,
        Os::Other => &[],
    }
}

/// Homebrew locations for the machine we run on.
pub fn homebrew_prefixes() -> &'static [(&'static str, &'static str)] {
    homebrew_prefixes_for(current())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triples_per_os_and_cpu() {
        assert_eq!(
            release_triple_for(Os::MacOs, "aarch64"),
            Some("aarch64-apple-darwin")
        );
        assert_eq!(
            release_triple_for(Os::MacOs, "x86_64"),
            Some("x86_64-apple-darwin")
        );
        assert_eq!(
            release_triple_for(Os::Linux, "x86_64"),
            Some("x86_64-unknown-linux-musl")
        );
        assert_eq!(
            release_triple_for(Os::Linux, "aarch64"),
            Some("aarch64-unknown-linux-musl")
        );
        assert_eq!(release_triple_for(Os::Linux, "riscv64"), None);
        assert_eq!(release_triple_for(Os::Other, "x86_64"), None);
    }

    #[test]
    fn homebrew_locations() {
        assert!(homebrew_prefixes_for(Os::MacOs)
            .iter()
            .any(|p| p.0 == "/opt/homebrew"));
        assert!(homebrew_prefixes_for(Os::Linux)
            .iter()
            .any(|p| p.0 == "/home/linuxbrew/.linuxbrew"));
        assert!(homebrew_prefixes_for(Os::Other).is_empty());
        assert_eq!(Os::parse("macos"), Os::MacOs);
        assert_eq!(Os::parse("windows"), Os::Other);
    }

    #[test]
    fn managers_per_os() {
        assert_eq!(package_managers_for(Os::MacOs), [Manager::Brew]);
        assert_eq!(
            package_managers_for(Os::Linux),
            [Manager::Brew, Manager::Apt, Manager::Pacman]
        );
        assert!(package_managers_for(Os::Other).is_empty());
        assert_eq!(Manager::Apt.binary(), "apt-get");
        assert_eq!(Manager::Brew.install_command().last(), Some(&"install"));
    }

    #[test]
    fn this_machine_is_supported() {
        // Fails loudly if shellcraft is built on an OS/CPU without a release mapping.
        assert!(
            release_triple().is_some(),
            "{} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
    }
}
