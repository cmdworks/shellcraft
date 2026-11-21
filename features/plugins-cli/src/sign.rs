//! Registry signatures: Ed25519 over the exact bytes of `index.toml`, stored hex-encoded in
//! `index.toml.sig`. Keys are 32 bytes written as 64 hex digits.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

pub const SIG_FILE: &str = "index.toml.sig";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex<const N: usize>(s: &str) -> Result<[u8; N]> {
    let s = s.trim();
    if s.len() != N * 2 || !s.is_ascii() {
        bail!("expected {} hex digits", N * 2);
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).context("not hex")?;
    }
    Ok(out)
}

/// Create a key pair: the secret goes to `file` (owner-only), the public key is returned.
pub fn keygen(file: &Path) -> Result<String> {
    if file.exists() {
        bail!("{} already exists", file.display());
    }
    let key = SigningKey::generate(&mut rand_core::OsRng);
    fs::write(file, format!("{}\n", hex(&key.to_bytes())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file, fs::Permissions::from_mode(0o600))?;
    }
    Ok(hex(key.verifying_key().as_bytes()))
}

/// Sign `<registry>/index.toml` with the secret key in `key_file`; writes `index.toml.sig`.
pub fn sign(registry: &Path, key_file: &Path) -> Result<String> {
    let secret: [u8; 32] = unhex(&fs::read_to_string(key_file).context("reading the key file")?)?;
    let key = SigningKey::from_bytes(&secret);
    let index = fs::read(registry.join(super::registry::INDEX_FILE))?;
    fs::write(
        registry.join(SIG_FILE),
        format!("{}\n", hex(&key.sign(&index).to_bytes())),
    )?;
    Ok(hex(key.verifying_key().as_bytes()))
}

/// Check the signature file in `dir` against the registry's `index` text and the trusted `keys`.
pub fn verify(dir: &Path, index: &str, keys: &[String]) -> Result<()> {
    let sig = fs::read_to_string(dir.join(SIG_FILE)).map_err(|_| {
        anyhow::anyhow!("not signed (no {SIG_FILE}) but signing keys are configured")
    })?;
    let sig = Signature::from_bytes(&unhex::<64>(&sig).context("bad signature file")?);
    for k in keys {
        let Ok(bytes) = unhex::<32>(k) else { continue };
        let Ok(vk) = VerifyingKey::from_bytes(&bytes) else {
            continue;
        };
        if vk.verify(index.as_bytes(), &sig).is_ok() {
            return Ok(());
        }
    }
    bail!("the signature does not match any trusted key")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_and_tamper() {
        let dir = std::env::temp_dir().join(format!("sc-sign-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("index.toml"), "# index\n").expect("write");
        let pubkey = keygen(&dir.join("k")).expect("keygen");
        assert!(keygen(&dir.join("k")).is_err(), "never overwrites a key");
        assert_eq!(sign(&dir, &dir.join("k")).expect("sign"), pubkey);
        let keys = vec![pubkey.clone()];
        verify(&dir, "# index\n", &keys).expect("valid");
        assert!(
            verify(&dir, "# index\n# evil\n", &keys).is_err(),
            "changed index"
        );
        let other = keygen(&dir.join("k2")).expect("keygen");
        assert!(verify(&dir, "# index\n", &[other]).is_err(), "wrong key");
        fs::remove_file(dir.join(SIG_FILE)).expect("rm");
        assert!(verify(&dir, "# index\n", &keys)
            .unwrap_err()
            .to_string()
            .contains("not signed"));
        let _ = fs::remove_dir_all(&dir);
    }
}
