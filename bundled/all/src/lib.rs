//! The plugins that ship inside the binary. They use the same `Plugin` interface as plugins
//! installed from a repository; this crate is only the list.

use shellcraft_bundled_git::Git;
use shellcraft_bundled_versions::Versions;
use shellcraft_plugin::Plugin;

pub use shellcraft_bundled_versions::{refresh as refresh_versions, TTL_SECS as VERSIONS_TTL_SECS};

static BUNDLED: &[&dyn Plugin] = &[&Git, &Versions];

/// All bundled plugins.
pub fn bundled() -> &'static [&'static dyn Plugin] {
    BUNDLED
}

/// Bundled plugin by name.
pub fn find(name: &str) -> Option<&'static dyn Plugin> {
    BUNDLED.iter().copied().find(|p| p.name() == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellcraft_plugin::manifest::RESERVED;

    #[test]
    fn reserved_names_match_the_bundled_plugins() {
        let mut have: Vec<&str> = bundled().iter().map(|p| p.name()).collect();
        let mut want: Vec<&str> = RESERVED.to_vec();
        have.sort_unstable();
        want.sort_unstable();
        assert_eq!(
            have, want,
            "update manifest::RESERVED when a plugin is added"
        );
    }
}
