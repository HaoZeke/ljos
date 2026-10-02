//! `ljos upgrade`: replace the seat's binaries with a published release.
//!
//! The seat's guard refuses any agent's write to its own binaries, since an
//! agent that may rewrite them may rewrite the law. The binaries still have
//! to move forward, so this verb installs only what the project's release
//! workflow built and published for a tag: the archive is fetched from the
//! release, its sha256 is checked against the checksum published beside it,
//! each binary must be an executable that names the release's version, and
//! the old binaries are kept beside the new ones.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// The repository whose releases the seat installs from.
pub const RELEASE_REPO: &str = "leidarljos/ljos";

/// The binaries a release carries that the seat installs, when present
/// beside the running `ljos`: `ljos` and `ljos-mcp` always.
pub const BINARIES: &[&str] = &["ljos", "ljos-mcp", "ljos-hud"];

/// The target triple of the published archive for this machine.
#[must_use]
pub fn target() -> Option<&'static str> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some("x86_64-unknown-linux-gnu"),
        ("aarch64", "macos") => Some("aarch64-apple-darwin"),
        _ => None,
    }
}

/// The archive name the release workflow publishes for `version` on
/// `target`: `ljos-v0.22.3-x86_64-unknown-linux-gnu.tar.gz`.
#[must_use]
pub fn asset_name(version: &str, target: &str) -> String {
    format!("ljos-v{}-{target}.tar.gz", version.trim_start_matches('v'))
}

/// The digest in a `sha256sum` line: its first field, when it is 64 hex
/// characters.
#[must_use]
pub fn digest_of(line: &str) -> Option<String> {
    let d = line.split_whitespace().next()?.to_ascii_lowercase();
    (d.len() == 64 && d.chars().all(|c| c.is_ascii_hexdigit())).then_some(d)
}

/// Whether `--version` output names this binary at `version`.
#[must_use]
pub fn names_version(output: &str, binary: &str, version: &str) -> bool {
    let want = format!("{binary} {}", version.trim_start_matches('v'));
    output.lines().next().is_some_and(|l| l.trim() == want)
}

fn fetch(url: &str, to: &Path) -> Result<()> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .with_context(|| format!("upgrade: GET {url}"))?;
    let mut body = Vec::new();
    resp.into_reader()
        .take(256 * 1024 * 1024)
        .read_to_end(&mut body)
        .with_context(|| format!("upgrade: reading {url}"))?;
    std::fs::write(to, body)?;
    Ok(())
}

/// The newest published release's version, from the forge.
fn latest() -> Result<String> {
    let url = format!("https://api.github.com/repos/{RELEASE_REPO}/releases/latest");
    let v: serde_json::Value = ureq::get(&url)
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .context("upgrade: asking for the latest release")?
        .into_json()?;
    v["tag_name"]
        .as_str()
        .map(|t| t.trim_start_matches('v').to_string())
        .context("upgrade: the latest release has no tag")
}

fn run_ok(cmd: &str, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .with_context(|| format!("upgrade: running {cmd}"))?;
    if !out.status.success() {
        bail!(
            "upgrade: {cmd} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Install release `version` (the newest when `None`) into the directory
/// of the running `ljos`, or `dir`. Returns what it did, a line a binary.
///
/// # Errors
///
/// No archive for this machine, a download, checksum or version that does
/// not hold, or a file that cannot be written.
pub fn upgrade(version: Option<&str>, dir: Option<&Path>) -> Result<String> {
    let version = match version {
        Some(v) => v.trim_start_matches('v').to_string(),
        None => latest()?,
    };
    let target = target().context("upgrade: no published archive for this machine")?;
    let dir: PathBuf = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_exe()?
            .canonicalize()?
            .parent()
            .context("upgrade: the running ljos has no directory")?
            .to_path_buf(),
    };
    let asset = asset_name(&version, target);
    let base = format!("https://github.com/{RELEASE_REPO}/releases/download/v{version}");
    let work = std::env::temp_dir().join(format!("ljos-upgrade-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let archive = work.join(&asset);
    let sum = work.join(format!("{asset}.sha256"));
    fetch(&format!("{base}/{asset}"), &archive)?;
    fetch(&format!("{base}/{asset}.sha256"), &sum)?;
    let want = digest_of(&std::fs::read_to_string(&sum)?)
        .context("upgrade: the published checksum is not a sha256 line")?;
    let have = digest_of(&run_ok("sha256sum", &[&archive.display().to_string()])?)
        .context("upgrade: sha256sum gave no digest")?;
    if want != have {
        bail!(
            "upgrade: {asset} has sha256 {have}, the release publishes {want}; nothing installed"
        );
    }
    run_ok(
        "tar",
        &[
            "-xzf",
            &archive.display().to_string(),
            "-C",
            &work.display().to_string(),
        ],
    )?;
    let unpacked = work.join(asset.trim_end_matches(".tar.gz"));
    // Check every binary before replacing any, so a bad archive changes nothing.
    let mut ready = Vec::new();
    for name in BINARIES {
        let new = unpacked.join(name);
        let installed = dir.join(name);
        if !new.is_file() {
            if *name == "ljos-hud" {
                continue;
            }
            bail!("upgrade: {asset} carries no {name}; nothing installed");
        }
        if *name == "ljos-hud" && !installed.exists() {
            continue;
        }
        let bytes = std::fs::read(&new)?;
        if !bytes.starts_with(b"\x7fELF") && !bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]) {
            bail!("upgrade: {name} in {asset} is not an executable; nothing installed");
        }
        let said = run_ok(&new.display().to_string(), &["--version"])?;
        if !names_version(&said, name, &version) {
            bail!(
                "upgrade: {name} in {asset} says {:?}, not {name} {version}; nothing installed",
                said.lines().next().unwrap_or("")
            );
        }
        ready.push((name, new, installed));
    }
    let mut out = String::new();
    for (name, new, installed) in ready {
        if installed.exists() {
            let keep = dir.join(format!("{name}.bak-pre-{version}"));
            std::fs::copy(&installed, &keep)?;
        }
        let staged = dir.join(format!("{name}.new"));
        std::fs::copy(&new, &staged)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&staged, &installed)?;
        out.push_str(&format!(
            "installed {name} {version} at {}\n",
            installed.display()
        ));
    }
    let _ = std::fs::remove_dir_all(&work);
    out.push_str(&format!(
        "from {base}/{asset}, sha256 {have}; the old binaries are kept as NAME.bak-pre-{version}\n"
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_archive_name_and_its_checks_are_the_release_workflow_s() {
        assert_eq!(
            asset_name("0.22.3", "x86_64-unknown-linux-gnu"),
            "ljos-v0.22.3-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            asset_name("v0.22.3", "aarch64-apple-darwin"),
            "ljos-v0.22.3-aarch64-apple-darwin.tar.gz"
        );
        let d = "d6e305288c898a34ec65e8f7c067bb680e77f49eeb041692be2e6be2e68d0f85";
        assert_eq!(
            digest_of(&format!("{d}  ljos-v0.22.3.tar.gz\n")).as_deref(),
            Some(d)
        );
        assert_eq!(digest_of("not a digest"), None);
        assert_eq!(digest_of(&d[..63]), None, "a short digest is no digest");
        assert!(names_version("ljos 0.22.3\n", "ljos", "0.22.3"));
        assert!(names_version("ljos-mcp 0.22.3", "ljos-mcp", "v0.22.3"));
        assert!(!names_version("ljos 0.22.2", "ljos", "0.22.3"));
        assert!(
            !names_version("#!/usr/bin/env python3", "ljos", "0.22.3"),
            "a script is not the release"
        );
    }
}
