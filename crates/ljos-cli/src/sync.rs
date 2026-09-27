//! Sharing the seat's memory across machines through the tracker repository
//! that already travels between them.
//!
//! Each tracker repository names one scope in `.ljos/sync.toml`: the scope's
//! name and the age recipients allowed to read it. Every machine writes only
//! its own log, `.ljos/atoms/<host>.jsonl.age`, sealed to those recipients,
//! so two machines never touch one file and git never has to merge a log.
//! A log carries the machine's live atoms of that scope without their
//! vectors; the receiving pack encodes them again. An atom's scope is its
//! `scope:NAME` entity, else the machine's default scope.
//!
//! Import posts every atom of every other machine's log to this seat's pack,
//! which answers a text it already holds with the claim it has, so an import
//! twice is an import once. Retirement travels by difference: the texts an
//! import took from a host are remembered, and a text that host no longer
//! sends retires the local copy that came from it, never a claim this
//! machine wrote itself.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::Value;

/// The scope a tracker repository shares: `.ljos/sync.toml`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Scope {
    /// The scope's name; an atom carrying `scope:NAME` belongs to it.
    pub name: String,
    /// The age public keys of the machines allowed to read this scope.
    pub recipients: Vec<String>,
}

/// This machine's defaults: `~/.config/ljos/sync.toml`, `default_scope`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
struct Local {
    #[serde(default)]
    default_scope: Option<String>,
    /// The tracker repositories this machine shares, when more than the
    /// tracker root: a machine can hold one scope's repository beside
    /// another's.
    #[serde(default)]
    repos: Vec<String>,
}

fn local() -> Local {
    std::fs::read_to_string(config_dir().join("sync.toml"))
        .ok()
        .and_then(|t| toml::from_str::<Local>(&t).ok())
        .unwrap_or_default()
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map_or_else(|| PathBuf::from(path), |h| PathBuf::from(h).join(rest)),
        None => PathBuf::from(path),
    }
}

/// The entity an imported atom carries: which machine sent it.
#[must_use]
pub fn sender_entity(host: &str) -> String {
    format!("sync:{host}")
}

/// This machine's short host name.
#[must_use]
pub fn host() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_string())
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "host".into())
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("ljos")
}

fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
        .join("ljos")
        .join("sync")
}

/// This machine's age identity, made on first use.
#[must_use]
pub fn identity_path() -> PathBuf {
    config_dir().join("age.key")
}

/// The public key of this machine's identity, making the identity first when
/// there is none. This is what goes into a scope's `recipients`.
///
/// # Errors
///
/// `age-keygen` missing or refusing.
pub fn public_key() -> Result<String> {
    let key = identity_path();
    if !key.exists() {
        if let Some(dir) = key.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let made = Command::new("age-keygen")
            .arg("-o")
            .arg(&key)
            .stdin(Stdio::null())
            .output()
            .context("age-keygen not on PATH; install age")?;
        if !made.status.success() {
            bail!(
                "age-keygen: {}",
                String::from_utf8_lossy(&made.stderr).trim()
            );
        }
    }
    let out = Command::new("age-keygen")
        .arg("-y")
        .arg(&key)
        .output()
        .context("age-keygen not on PATH; install age")?;
    if !out.status.success() {
        bail!(
            "age-keygen -y: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The scope a tracker repository declares, when it declares one.
///
/// # Errors
///
/// A `.ljos/sync.toml` that is present and does not parse.
pub fn scope_of_repo(root: &Path) -> Result<Option<Scope>> {
    let path = root.join(".ljos").join("sync.toml");
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(
            toml::from_str(&text).with_context(|| path.display().to_string())?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| path.display().to_string()),
    }
}

fn default_scope(repo_scope: &str) -> String {
    local()
        .default_scope
        .unwrap_or_else(|| repo_scope.to_string())
}

/// The scope an atom belongs to: its `scope:NAME` entity, else `default`.
#[must_use]
pub fn atom_scope(atom: &Value, default: &str) -> String {
    atom["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find_map(|e| e.strip_prefix("scope:"))
        .map_or_else(|| default.to_string(), str::to_string)
}

/// An atom as it travels: without its vector, and without a sender tag of
/// another machine, since a host sends only what it holds as its own.
#[must_use]
pub fn travelling(atom: &Value) -> Value {
    let mut a = atom.clone();
    if let Some(map) = a.as_object_mut() {
        map.remove("embedding");
    }
    a
}

/// Whether this machine holds an atom as its own rather than as an import.
#[must_use]
pub fn is_own(atom: &Value) -> bool {
    !atom["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|e| e.starts_with("sync:"))
}

/// The plaintext of this machine's log for a scope: its own live atoms of
/// that scope, sorted by id, one JSON object a line.
#[must_use]
pub fn log_plaintext(atoms: &[Value], scope: &str, default: &str) -> String {
    let mut mine: Vec<Value> = atoms
        .iter()
        .filter(|a| is_own(a) && atom_scope(a, default) == scope)
        .map(travelling)
        .collect();
    mine.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    mine.iter().map(|a| format!("{a}\n")).collect()
}

fn log_dir(root: &Path) -> PathBuf {
    root.join(".ljos").join("atoms")
}

/// Seal `plain` to the recipients into `out`.
fn seal(plain: &str, recipients: &[String], out: &Path) -> Result<()> {
    if recipients.is_empty() {
        bail!("sync: the scope names no recipients; add this machine's `ljos sync --key` to .ljos/sync.toml");
    }
    let mut cmd = Command::new("age");
    for r in recipients {
        cmd.arg("-r").arg(r);
    }
    let tmp = out.with_extension("age.tmp");
    let mut child = cmd
        .arg("-o")
        .arg(&tmp)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("age not on PATH; install age")?;
    use std::io::Write;
    child
        .stdin
        .take()
        .context("age: no stdin")?
        .write_all(plain.as_bytes())?;
    let done = child.wait_with_output()?;
    if !done.status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("age: {}", String::from_utf8_lossy(&done.stderr).trim());
    }
    std::fs::rename(&tmp, out)?;
    Ok(())
}

/// Open a sealed log with this machine's identity; none when this machine
/// is not one of its recipients.
fn open(path: &Path, identity: &Path) -> Result<Option<String>> {
    let out = Command::new("age")
        .arg("-d")
        .arg("-i")
        .arg(identity)
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .context("age not on PATH; install age")?;
    if out.status.success() {
        Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
    } else if String::from_utf8_lossy(&out.stderr).contains("no identity matched") {
        Ok(None)
    } else {
        bail!(
            "age -d {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )
    }
}

/// Write this machine's log for the repository's scope when its atoms
/// changed since the last write. Returns the line that says what happened.
///
/// # Errors
///
/// The pack not answering, or age refusing to seal.
pub fn export(root: &Path, scope: &Scope, atoms: &[Value]) -> Result<String> {
    let host = host();
    let plain = log_plaintext(atoms, &scope.name, &default_scope(&scope.name));
    let count = plain.lines().count();
    let dir = log_dir(root);
    std::fs::create_dir_all(&dir)?;
    let stamp = dir.join(format!("{host}.digest"));
    let digest = format!(
        "{} {}\n",
        crate::work_id(&plain),
        crate::work_id(&scope.recipients.join(","))
    );
    if std::fs::read_to_string(&stamp).is_ok_and(|d| d == digest) {
        return Ok(format!(
            "sync: {count} {} atoms unchanged since the last export\n",
            scope.name
        ));
    }
    seal(
        &plain,
        &scope.recipients,
        &dir.join(format!("{host}.jsonl.age")),
    )?;
    std::fs::write(&stamp, digest)?;
    Ok(format!(
        "sync: exported {count} {} atoms to .ljos/atoms/{host}.jsonl.age, sealed to {} recipient{}\n",
        scope.name,
        scope.recipients.len(),
        if scope.recipients.len() == 1 { "" } else { "s" }
    ))
}

/// What one host's log asks of this pack: the atoms to post, and the texts
/// it sent last time and sends no longer.
#[derive(Debug, Default, PartialEq)]
pub struct Incoming {
    pub post: Vec<Value>,
    pub retired: Vec<String>,
}

/// Read one host's log against the texts taken from it last time.
#[must_use]
pub fn incoming(plain: &str, host: &str, before: &BTreeSet<String>) -> Incoming {
    let mut post = Vec::new();
    let mut now = BTreeSet::new();
    for line in plain.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(mut atom) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let text = atom["text"].as_str().unwrap_or("").to_string();
        if text.is_empty() {
            continue;
        }
        now.insert(crate::work_id(&text));
        if let Some(map) = atom.as_object_mut() {
            let mut entities: Vec<Value> = map
                .get("entities")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let tag = sender_entity(host);
            if !entities.iter().any(|e| e.as_str() == Some(tag.as_str())) {
                entities.push(Value::String(tag));
            }
            map.insert("entities".into(), Value::Array(entities));
            map.remove("id");
        }
        post.push(atom);
    }
    let retired = before.difference(&now).cloned().collect();
    Incoming { post, retired }
}

fn taken_path(scope: &str, host: &str) -> PathBuf {
    state_dir().join(scope).join(format!("{host}.taken"))
}

fn read_taken(scope: &str, host: &str) -> BTreeSet<String> {
    std::fs::read_to_string(taken_path(scope, host))
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn write_taken(scope: &str, host: &str, plain: &str) -> Result<()> {
    let path = taken_path(scope, host);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let ids: BTreeSet<String> = plain
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|a| a["text"].as_str().map(crate::work_id))
        .collect();
    std::fs::write(path, ids.into_iter().map(|i| i + "\n").collect::<String>())?;
    Ok(())
}

/// Import every other machine's log of the repository's scope into this
/// seat's pack. Returns one line per host.
///
/// # Errors
///
/// The pack not answering, or a log this machine should open failing to.
pub fn import(root: &Path, scope: &Scope) -> Result<String> {
    let me = host();
    let identity = identity_path();
    if !identity.exists() {
        return Ok("sync: no age identity on this machine; `ljos sync --key` makes one\n".into());
    }
    let dir = log_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(format!("sync: no {} logs yet\n", scope.name));
    };
    let client = crate::pack()?;
    let mut out = String::new();
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".jsonl.age"))
        .collect();
    paths.sort();
    for path in paths {
        let host = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".jsonl.age"))
            .unwrap_or("")
            .to_string();
        if host.is_empty() || host == me {
            continue;
        }
        let Some(plain) = open(&path, &identity)? else {
            out.push_str(&format!(
                "sync: {host}'s {} log is not sealed to this machine\n",
                scope.name
            ));
            continue;
        };
        let got = incoming(&plain, &host, &read_taken(&scope.name, &host));
        let (mut kept, mut refused) = (0usize, 0usize);
        let workspace = client.workspace();
        for mut atom in got.post {
            if let Some(map) = atom.as_object_mut() {
                map.insert("workspace".into(), Value::String(workspace.clone()));
            }
            match client.post_atom(&atom) {
                Ok(_) => kept += 1,
                Err(_) => refused += 1,
            }
        }
        let mut retired = 0usize;
        if !got.retired.is_empty() {
            let tag = sender_entity(&host);
            for atom in crate::atoms_lean(&client, &workspace)? {
                let text = atom["text"].as_str().unwrap_or("");
                let from_host = atom["entities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|e| e.as_str() == Some(tag.as_str()));
                if from_host && got.retired.contains(&crate::work_id(text)) {
                    if let Some(id) = atom["id"].as_str() {
                        if crate::packset_forget(id, None).is_ok() {
                            retired += 1;
                        }
                    }
                }
            }
        }
        write_taken(&scope.name, &host, &plain)?;
        out.push_str(&format!(
            "sync: {host}: {kept} {} atoms taken, {refused} refused, {retired} retired\n",
            scope.name
        ));
    }
    Ok(out)
}

/// The scope of the repository that holds an issue, when it declares one.
#[must_use]
pub fn scope_for_issue(issue: &str) -> Option<String> {
    let hit = vissue_core::Layout::resolve(None, None)
        .and_then(vissue_core::Router::load)
        .and_then(|router| router.find_by_id(issue))
        .ok()?;
    let root = hit
        .path
        .ancestors()
        .find(|d| d.join(".git").exists())?
        .to_path_buf();
    scope_of_repo(&root).ok().flatten().map(|s| s.name)
}

/// The tracker repository this seat writes, its root directory.
fn tracker_root() -> Result<PathBuf> {
    let layout = vissue_core::Layout::resolve(None, None).map_err(anyhow::Error::from)?;
    Ok(layout.root().to_path_buf())
}

fn git(root: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

/// Commit this host's log alone and push it to the upstream and every other
/// remote that carries the branch.
fn commit_log(root: &Path, scope: &str) -> String {
    let host = host();
    let files = [
        format!(".ljos/atoms/{host}.jsonl.age"),
        format!(".ljos/atoms/{host}.digest"),
    ];
    let mut add = vec!["add", "--"];
    add.extend(files.iter().map(String::as_str));
    if git(root, &add).map_or(true, |o| !o.status.success()) {
        return "sync: could not stage the log\n".into();
    }
    let staged = git(
        root,
        &["diff", "--cached", "--quiet", "--", &files[0], &files[1]],
    );
    if staged.is_ok_and(|o| o.status.success()) {
        return String::new();
    }
    let message = format!("chore(sync): {host} {scope} atoms");
    let mut commit = vec!["commit", "-q", "--only", "-m", message.as_str(), "--"];
    commit.extend(files.iter().map(String::as_str));
    match git(root, &commit) {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            return format!(
                "sync: commit refused: {}\n",
                String::from_utf8_lossy(&o.stderr)
                    .lines()
                    .next()
                    .unwrap_or("")
            )
        }
        Err(e) => return format!("sync: git: {e}\n"),
    }
    let mut pushed = vec![];
    if git(root, &["push", "-q"]).is_ok_and(|o| o.status.success()) {
        pushed.push("upstream".to_string());
    }
    if let Some(up) = crate::tracker_upstream(root) {
        for (remote, branch) in crate::tracker_mirrors(root, &up).unwrap_or_default() {
            let refspec = format!("HEAD:refs/heads/{branch}");
            if git(root, &["push", "-q", &remote, &refspec]).is_ok_and(|o| o.status.success()) {
                pushed.push(remote);
            }
        }
    }
    format!(
        "sync: committed {message}; pushed to {}\n",
        if pushed.is_empty() {
            "nothing".to_string()
        } else {
            pushed.join(", ")
        }
    )
}

/// Fetch every remote and fast-forward to each one that is ahead. Machines
/// that push to different remotes of one tracker then read each other's
/// logs whenever the history is linear; a real divergence stays for the
/// doctor's split row and a person.
fn catch_up(root: &Path) -> String {
    if git(root, &["fetch", "-q", "--all"]).map_or(true, |o| !o.status.success()) {
        return "sync: fetch failed; reading the logs as they are\n".into();
    }
    let Some(up) = crate::tracker_upstream(root) else {
        return String::new();
    };
    let mut refs = vec![up.clone()];
    refs.extend(
        crate::tracker_mirrors(root, &up)
            .unwrap_or_default()
            .into_iter()
            .map(|(r, b)| format!("{r}/{b}")),
    );
    let mut stuck = Vec::new();
    for r in refs {
        let merged = git(root, &["merge", "-q", "--ff-only", &r]);
        if !merged.is_ok_and(|o| o.status.success()) {
            stuck.push(r);
        }
    }
    if stuck.is_empty() {
        String::new()
    } else {
        format!(
            "sync: could not fast-forward to {}; `ljos doctor` names the split\n",
            stuck.join(", ")
        )
    }
}

/// One pass over the tracker repository's scope: pull, take every other
/// machine's log, write and push this machine's. `pull_import` and `export`
/// pick the halves; the sitting takes the first, finish the second.
///
/// # Errors
///
/// A scope file that does not parse, the pack not answering, or age
/// refusing to seal.
pub fn sync_repo(pull_import: bool, export_log: bool) -> Result<String> {
    let listed = local().repos;
    let roots: Vec<PathBuf> = if listed.is_empty() {
        vec![tracker_root()?]
    } else {
        listed.iter().map(|r| expand(r)).collect()
    };
    let mut out = String::new();
    for root in roots {
        out.push_str(&sync_one(&root, pull_import, export_log)?);
    }
    Ok(out)
}

fn sync_one(root: &Path, pull_import: bool, export_log: bool) -> Result<String> {
    let root = root.to_path_buf();
    let Some(scope) = scope_of_repo(&root)? else {
        return Ok(format!(
            "sync: {} names no scope; write .ljos/sync.toml with name and recipients (`ljos sync --key` prints this machine's)\n",
            root.display()
        ));
    };
    let mut out = String::new();
    if pull_import {
        out.push_str(&catch_up(&root));
        out.push_str(&import(&root, &scope)?);
    }
    if export_log {
        let client = crate::pack()?;
        let atoms = crate::atoms_lean(&client, &client.workspace())?;
        out.push_str(&export(&root, &scope, &atoms)?);
        out.push_str(&commit_log(&root, &scope.name));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(id: &str, text: &str, entities: &[&str]) -> Value {
        serde_json::json!({"id": id, "text": text, "kind": "lesson",
            "entities": entities, "embedding": [0.1, 0.2]})
    }

    #[test]
    fn a_log_carries_own_atoms_of_its_scope_without_vectors() {
        let atoms = vec![
            atom("b", "second", &["seat:x"]),
            atom("a", "first", &[]),
            atom("c", "personal", &["scope:personal"]),
            atom("d", "imported", &["sync:otherhost"]),
        ];
        let plain = log_plaintext(&atoms, "surf", "surf");
        let lines: Vec<Value> = plain
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2, "{plain}");
        assert_eq!(lines[0]["id"], "a");
        assert_eq!(lines[1]["id"], "b");
        assert!(lines.iter().all(|a| a.get("embedding").is_none()));
        assert_eq!(log_plaintext(&atoms, "personal", "surf").lines().count(), 1);
    }

    #[test]
    fn an_import_tags_the_sender_and_names_what_it_retired() {
        let plain = format!(
            "{}\n{}\n",
            atom("a", "kept", &["seat:x"]),
            atom("b", "new", &[])
        );
        let before: BTreeSet<String> = [crate::work_id("kept"), crate::work_id("dropped")].into();
        let got = incoming(&plain, "rglat", &before);
        assert_eq!(got.post.len(), 2);
        assert!(got.post.iter().all(|a| a.get("id").is_none()));
        assert!(got.post.iter().all(|a| a["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "sync:rglat")));
        assert_eq!(got.retired, vec![crate::work_id("dropped")]);
    }

    #[test]
    fn a_sealed_log_opens_for_a_recipient_and_not_for_another() {
        let dir = tempfile::tempdir().unwrap();
        let key = |name: &str| {
            let path = dir.path().join(name);
            let made = Command::new("age-keygen")
                .arg("-o")
                .arg(&path)
                .output()
                .unwrap();
            assert!(made.status.success(), "age-keygen must be installed");
            let public = Command::new("age-keygen")
                .arg("-y")
                .arg(&path)
                .output()
                .unwrap();
            (
                path,
                String::from_utf8_lossy(&public.stdout).trim().to_string(),
            )
        };
        let (mine, my_public) = key("mine.key");
        let (other, _) = key("other.key");
        let out = dir.path().join("host.jsonl.age");
        seal("{\"text\":\"x\"}\n", &[my_public], &out).unwrap();
        assert_eq!(
            open(&out, &mine).unwrap().as_deref(),
            Some("{\"text\":\"x\"}\n")
        );
        assert_eq!(open(&out, &other).unwrap(), None);
        assert!(seal("x", &[], &out).is_err(), "no recipients is refused");
    }

    #[test]
    fn an_atoms_scope_is_its_entity_or_the_default() {
        assert_eq!(
            atom_scope(&atom("a", "t", &["scope:personal"]), "surf"),
            "personal"
        );
        assert_eq!(atom_scope(&atom("a", "t", &["seat:x"]), "surf"), "surf");
    }
}
