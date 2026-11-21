//! `plugin new`: skeletons for the five plugin kinds.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use shellcraft_plugin::manifest::{Manifest, FILE_NAME};

fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

pub(super) fn scaffold(name: &str, kind: &str) -> Result<()> {
    let probe = format!("name = \"{name}\"\nversion = \"0.1.0\"\n");
    Manifest::parse(&probe).map_err(|e| anyhow::anyhow!(e))?;
    let dir = PathBuf::from(name);
    if dir.exists() {
        bail!("{name} already exists");
    }
    fs::create_dir_all(dir.join("bin"))?;
    let mut manifest = format!("api = 1\nname = \"{name}\"\nversion = \"0.1.0\"\ndescription = \"TODO: one line about {name}\"\nlicense = \"MIT\"\n");
    match kind {
        "segment" => {
            manifest += "\n[segment.main]\ncommand = \"bin/segment\"\nttl_ms = 10000\nwhen = { files = [\".git\"] }   # show only in projects that have one of these\n\n[permissions]\nexec = []\nnetwork = false\n";
            let script = dir.join("bin/segment");
            fs::write(&script, format!("#!/bin/sh\n# Print one line. Optional tone prefix: [git] [version] [tool] [warn] [dim]\necho \"[tool] {name}\"\n"))?;
            make_executable(&script)?;
        }
        "snippet" => {
            fs::create_dir_all(dir.join("snippets"))?;
            manifest += "\n[snippets]\nzsh = \"snippets/init.zsh\"\nbash = \"snippets/init.bash\"\nfish = \"snippets/init.fish\"\n";
            fs::write(
                dir.join("snippets/init.zsh"),
                format!(
                    "# runs inside zsh at startup\nalias {name}-hello='echo hello from {name}'\n"
                ),
            )?;
            fs::write(
                dir.join("snippets/init.bash"),
                format!(
                    "# runs inside bash at startup\nalias {name}-hello='echo hello from {name}'\n"
                ),
            )?;
            fs::write(
                dir.join("snippets/init.fish"),
                format!(
                    "# runs inside fish at startup\nalias {name}-hello 'echo hello from {name}'\n"
                ),
            )?;
        }
        "command" => {
            manifest += &format!("\n[command.{name}]\nrun = \"bin/{name}\"\nabout = \"TODO: what `shellcraft {name}` does\"\n");
            let script = dir.join("bin").join(name);
            fs::write(
                &script,
                format!("#!/bin/sh\n# Runs as `shellcraft {name} <args>`.\necho \"{name}: $*\"\n"),
            )?;
            make_executable(&script)?;
        }
        "hook" => {
            manifest += "\n[hook.main]\nevent = \"cd\"          # cd (directory changed) or post (a command finished)\nrun = \"bin/hook\"\n";
            let script = dir.join("bin/hook");
            fs::write(&script, "#!/bin/sh\n# Started in the background. See SHELLCRAFT_EVENT, SHELLCRAFT_CWD, SHELLCRAFT_EXIT,\n# SHELLCRAFT_DURATION_MS and SHELLCRAFT_COMMAND in the environment.\n: \"$SHELLCRAFT_CWD\"\n")?;
            make_executable(&script)?;
        }
        "data" => {
            manifest += "\n[aliases]\n# ll = \"ls -lah\"\n\n[abbr]\n# gco = \"git checkout\"\n\n[env]\n# EDITOR = \"nvim\"\n";
        }
        other => bail!("unknown kind `{other}` (segment, snippet, command, hook or data)"),
    }
    fs::write(dir.join(FILE_NAME), manifest)?;
    println!("created ./{name}\nnext: shellcraft plugin check ./{name}   then   shellcraft plugin add ./{name}");
    Ok(())
}
