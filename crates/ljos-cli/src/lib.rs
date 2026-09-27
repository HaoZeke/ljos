//! One seat over the habitats. Each habitat keeps its own crate.
//!
//! Cards are read-only. Remember/Prefer POST `/v1/atoms` and never extract
//! on write. Consensus is a different crate, then the tracker verb. Policyd
//! is argv law: this process does not reload a pack as a check.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use packset_client::{Hit, PacksetClient};
use serde_json::Value;

pub mod hud;
pub mod sync;

/// Working-core files this seat will print. Nothing else, and never write.
pub const CARD_NAMES: &[&str] = &["USER.md", "MEMORY.md"];

/// The sitting protocol: which store answers which question, the order of
/// verbs before, during and after the work, and the refusals worth knowing.
/// `ljos protocol` prints it, `ljos onboard` installs it as a skill, and the
/// server serves it at `ljos://protocol`. Harness agnostic on purpose.
pub const PROTOCOL: &str = include_str!("../doc/protocol.md");

/// The skill file a harness loads: front matter, then the protocol.
#[must_use]
pub fn skill_text() -> String {
    format!(
        "---\nname: ljos\ndescription: >\n  The seat protocol for vissue, packset, deedar, claimdag and \
consensus through ljos: which store answers which question, the order of verbs in a \
sitting, and the refusals worth knowing. Load before any work that touches an issue, \
a memory, a deed, a claim or a vote.\n---\n\n{PROTOCOL}"
    )
}

/// One step an onboarding took, or would take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub what: String,
    pub detail: String,
    pub ok: bool,
}

/// One agent runner, as the seat's own configuration describes it. The seat
/// ships no runner's name: the file at [`harnesses_path`] names them, one
/// table each, and `onboard` and `doctor` read it.
///
/// A runner registers MCP servers one of two ways. `register` is a command
/// that does it (`{server}` is replaced by the path to `ljos-mcp`) and
/// `registered` a command that exits 0 once it is done. Or `config` is a
/// file the runner reads, `marker` a line that means the entry is present,
/// and `snippet` what to append when it is not. `skills` is the directory
/// the runner loads skills from; the protocol goes to `<skills>/ljos/SKILL.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Harness {
    pub name: String,
    #[serde(default)]
    pub register: Vec<String>,
    #[serde(default)]
    pub registered: Vec<String>,
    #[serde(default)]
    pub config: Option<String>,
    #[serde(default)]
    pub marker: Option<String>,
    #[serde(default)]
    pub snippet: Option<String>,
    /// A JSON config file the runner reads its MCP servers from, for a
    /// runner an appended snippet cannot serve.
    pub config_json: Option<String>,
    /// Where in that file the entry goes, as a JSON pointer (`/mcp/ljos`).
    pub json_pointer: Option<String>,
    /// The entry to set there, as JSON text; `{server}` and `{name}` are
    /// replaced.
    pub json_entry: Option<String>,
    #[serde(default)]
    pub skills: Option<String>,
    /// A JSON settings file the runner reads hooks from, in the shape
    /// `{"hooks": {"<Event>": [{"matcher": "...", "hooks": [{"type":
    /// "command", "command": "..."}]}]}}`. `onboard` merges the seat's
    /// memory hook into it, so what the seat knows about a command or a
    /// prompt reaches the agent at the point of action.
    #[serde(default)]
    pub hooks: Option<String>,
    /// The events the memory hook fires on. Empty means [`HOOK_EVENTS`],
    /// the prompt event alone: a panel of this seat's personas settled on
    /// prompts over tool calls, because a turn issues many shell commands
    /// and one prompt. `["UserPromptSubmit", "PreToolUse"]` injects on both.
    #[serde(default)]
    pub hook_events: Vec<String>,
    /// Where a runner whose hooks are code loads a plugin from, for a
    /// runner with no hooks file: the plugin carries the memory hook and
    /// argv law and shells to `ljos hook`.
    #[serde(default)]
    pub plugin: Option<String>,
    /// Which bundled plugin goes there: a name in [`PLUGIN_TEMPLATES`].
    #[serde(default)]
    pub plugin_template: Option<String>,
    /// A command that proves the runner loads the ljos tools, not only that
    /// its config names them: it must exit 0 and print `ljos_sitting`. A
    /// runner installed without its MCP support lists the entry and loads
    /// nothing.
    #[serde(default)]
    pub probe: Vec<String>,
    /// The names this runner's MCP client sends at initialize, when they are
    /// not the runner's name: the seat is then the harness's name, so one
    /// runner's memory, ballots and trust rows stay one voter instead of
    /// scattering over `acme` and `acme-mcp-client`.
    #[serde(default)]
    pub clients: Vec<String>,
}

/// The plugins `ljos` carries for runners whose hooks are code, by name.
/// `{ljos}` in each is filled with the absolute path at onboard.
pub const PLUGIN_TEMPLATES: &[(&str, &str)] = &[
    ("opencode", include_str!("../assets/opencode/ljos.ts")),
    ("omp", include_str!("../assets/omp/ljos.ts")),
];

/// A runner's plugin as it is written: the template, `{ljos}` filled.
fn plugin_text(h: &Harness, ljos: &Path) -> Option<String> {
    let name = h.plugin_template.as_deref()?;
    PLUGIN_TEMPLATES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t.replace("{ljos}", &ljos.display().to_string()))
}

fn plugin_step(h: &Harness, dest: &Path, dry: bool) -> Step {
    let what = "plugin".to_string();
    let ljos = match ljos_path() {
        Ok(l) => l,
        Err(e) => {
            return Step {
                what,
                detail: format!("{e:#}"),
                ok: false,
            };
        }
    };
    let Some(text) = plugin_text(h, &ljos) else {
        return Step {
            what,
            detail: format!(
                "plugin_template {:?} is not one of {}",
                h.plugin_template.as_deref().unwrap_or(""),
                PLUGIN_TEMPLATES
                    .iter()
                    .map(|(n, _)| *n)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ok: false,
        };
    };
    if std::fs::read_to_string(dest).is_ok_and(|have| have == text) {
        return Step {
            what,
            detail: format!("{} is current", dest.display()),
            ok: true,
        };
    }
    if dry {
        return Step {
            what,
            detail: format!("would write {}", dest.display()),
            ok: true,
        };
    }
    let written = dest
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(dest, text));
    match written {
        Ok(()) => Step {
            what,
            detail: format!("wrote {}", dest.display()),
            ok: true,
        },
        Err(e) => Step {
            what,
            detail: format!("{}: {e}", dest.display()),
            ok: false,
        },
    }
}

/// The whole file: `[[harness]]` tables.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Harnesses {
    #[serde(default)]
    pub harness: Vec<Harness>,
}

/// An example of the file, with placeholder names. `ljos onboard --example`
/// prints it; the two shapes are a registering command and a config file.
pub const HARNESSES_EXAMPLE: &str = r#"# ~/.config/ljos/harnesses.toml: runners this machine registers by command.
# Optional: `ljos onboard` alone prints the one entry any runner takes.
# {server} is replaced by the path to ljos-mcp, {name} by the runner's name.
# Paths may start with ~. The seat names itself after the client that
# connects; nothing is passed in env.

[[harness]]
name = "runner-with-a-command"
register = ["runner", "mcp", "add", "-s", "user", "ljos", "--", "{server}"]
registered = ["runner", "mcp", "get", "ljos"]
skills = "~/.runner/skills"
hooks = "~/.runner/settings.json"
# hook_events = ["UserPromptSubmit", "PreToolUse"]   # the default is the prompt alone

[[harness]]
name = "runner-with-a-config-file"
config = "~/.other/config.toml"
marker = "[mcp_servers.ljos]"
# A runner that rebuilds its servers' environment from a short list must be
# told to pass XDG_RUNTIME_DIR, where the seat records live.
snippet = "\n[mcp_servers.ljos]\ncommand = \"{server}\"\nargs = []\nenv_vars = [\"XDG_RUNTIME_DIR\"]\n"
skills = "~/.other/skills"
hooks = "~/.other/hooks.json"
# A runner with no SessionEnd event takes the prompt and the tool call.
hook_events = ["UserPromptSubmit", "PreToolUse"]

[[harness]]
name = "runner-with-a-json-config"
config_json = "~/.config/runner/runner.json"
json_pointer = "/mcp/ljos"
json_entry = '{"type": "local", "command": ["{server}"], "enabled": true, "environment": {"LJOS_SEAT": "{name}"}}'
skills = "~/.config/runner/skills"

# Runners this seat has carried through the same work, as they take the
# server on this machine: a runner with an `mcp add` of its own is the
# first shape above, a runner with a TOML config the second. Copy the
# ones you run.

[[harness]]
name = "opencode"
config_json = "~/.config/opencode/opencode.json"
json_pointer = "/mcp/ljos"
json_entry = '{"type": "local", "command": ["{server}"], "enabled": true, "timeout": 30000}'
skills = "~/.config/opencode/skills"
# opencode's hooks are a plugin: the memory hook on each prompt, argv law
# on each bash call, the session id in every shell it opens.
plugin = "~/.config/opencode/plugins/ljos.ts"
plugin_template = "opencode"

[[harness]]
name = "hermes"
# `hermes mcp add` asks which tools to enable; the answer is all of them.
register = ["sh", "-c", "printf 'Y\\n' | hermes mcp add ljos --command {server}"]
config = "~/.hermes/config.yaml"
marker = "\n  ljos:\n    command:"
skills = "~/.hermes/skills"
# A hermes installed without its MCP extra lists ljos and loads nothing.
probe = ["hermes", "mcp", "test", "ljos"]

[[harness]]
name = "omp"
config_json = "~/.omp/agent/mcp.json"
json_pointer = "/mcpServers/ljos"
json_entry = '{"type": "stdio", "command": "{server}", "args": []}'
# A host whose omp config sets enablePiUser false reads skills from its
# skills.customDirectories instead; name that directory here.
skills = "~/.omp/agent/skills"
plugin = "~/.omp/agent/extensions/ljos.ts"
plugin_template = "omp"

[[harness]]
name = "grok"
config = "~/.grok/config.toml"
marker = "[mcp_servers.ljos]"
snippet = "\n[mcp_servers.ljos]\ncommand = \"{server}\"\nargs = []\nenabled = true\n"
skills = "~/.grok/skills"
"#;

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME unset; onboard needs a home directory")
}

/// `~` at the start of a configured path is the home directory.
fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().map_or_else(|_| PathBuf::from(path), |h| h.join(rest)),
        None => PathBuf::from(path),
    }
}

/// Where the runners are described: `$XDG_CONFIG_HOME/ljos/harnesses.toml`.
#[must_use]
pub fn harnesses_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().ok().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("ljos")
        .join("harnesses.toml")
}

/// Parse the runners file. An absent file is no runners, not an error.
///
/// # Errors
///
/// A file that is present and not this shape.
pub fn harnesses_from(path: &Path) -> Result<Harnesses> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("{}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Harnesses::default()),
        Err(e) => Err(e).with_context(|| format!("{}", path.display())),
    }
}

/// Where `ljos-mcp` is, as the runner will start it.
fn server_path() -> Result<PathBuf> {
    which::which("ljos-mcp").context("ljos-mcp not on PATH; install it beside ljos")
}

/// The MCP server entry any runner that reads JSON accepts.
pub fn server_entry() -> Result<Value> {
    Ok(serde_json::json!({
        "mcpServers": {
            "ljos": {
                "type": "stdio",
                "command": server_path()?.display().to_string(),
                "args": [],
                "env": {}
            }
        }
    }))
}

fn write_skill(dir: &Path, dry: bool) -> Step {
    let path = dir.join("ljos").join("SKILL.md");
    let text = skill_text();
    if std::fs::read_to_string(&path).is_ok_and(|have| have == text) {
        return Step {
            what: "skill".into(),
            detail: format!("{} is current", path.display()),
            ok: true,
        };
    }
    if dry {
        return Step {
            what: "skill".into(),
            detail: format!("would write {}", path.display()),
            ok: true,
        };
    }
    let written = std::fs::create_dir_all(path.parent().unwrap_or(dir))
        .and_then(|()| std::fs::write(&path, text));
    match written {
        Ok(()) => Step {
            what: "skill".into(),
            detail: format!("wrote {}", path.display()),
            ok: true,
        },
        Err(e) => Step {
            what: "skill".into(),
            detail: format!("{}: {e}", path.display()),
            ok: false,
        },
    }
}

/// `{server}` is the path to `ljos-mcp`, `{name}` the runner's name from
/// the runners file, for a registering command that wants either.
fn filled(argv: &[String], server: &Path, name: &str) -> Vec<String> {
    argv.iter()
        .map(|a| a.replace("{server}", &server.display().to_string()))
        .map(|a| a.replace("{name}", name))
        .collect()
}

/// Pronouns and defaults, not product names. A runner's own `LJOS_SEAT`
/// is treated the same way in [`resolve_assignee`]: the process naming
/// itself is omitted, so occupancy falls through to the session.
fn omitted_actor_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "seat" | "you" | "agent"
    )
}

/// The process naming itself: its `LJOS_SEAT`, or the seat it resolved
/// to, passed back as an assignee. Omitted, so occupancy stays the
/// conversation's.
fn own_seat(name: &str) -> bool {
    let n = name.trim();
    std::env::var("LJOS_SEAT")
        .ok()
        .is_some_and(|s| s.trim() == n)
        || whoami().seat == n
}

/// The conversation this process belongs to: every `*_SESSION_ID` the
/// runner stamped, one occupancy name and the keys it came from. No
/// product list.
fn session_actor() -> Option<(String, String)> {
    let mut parts: Vec<(String, String)> = std::env::vars()
        .filter(|(k, v)| runner_session_var(k, v))
        .collect();
    if parts.is_empty() {
        return None;
    }
    parts.sort_by(|a, b| a.0.cmp(&b.0));
    if parts.len() == 1 {
        return Some(session_from_value(&parts[0].0, &parts[0].1));
    }
    let joined = parts
        .iter()
        .map(|(k, v)| format!("{k}={}", v.trim()))
        .collect::<Vec<_>>()
        .join(";");
    let id = work_id(&joined);
    let keys = parts
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join("+");
    Some((format!("sess-{id}"), keys))
}

/// A conversation id the runner stamped, not the login (`XDG_SESSION_ID`
/// is a small integer): a `*_SESSION_ID`, or a `*_THREAD_ID` from a runner
/// that names its conversations threads. Values shorter than eight
/// characters are ignored.
fn runner_session_var(key: &str, val: &str) -> bool {
    (key.ends_with("_SESSION_ID") || key.ends_with("_THREAD_ID"))
        && key != "XDG_SESSION_ID"
        && val.trim().len() >= 8
}

fn session_from_value(key: &str, raw: &str) -> (String, String) {
    (raw.trim().to_string(), key.to_string())
}

/// Who is sitting. The seat is the program that connected: the name a
/// runner remembers, votes and earns trust under, the same across its
/// conversations. The holder is that seat in one conversation: the name
/// its claims are held under, so two conversations of one runner hold two
/// tickets while a vote from either counts for the one voter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    pub seat: String,
    pub holder: String,
    /// Where the name came from, for `ljos seat` and the doctor.
    pub source: String,
}

impl Seat {
    fn whole(name: &str, source: &str) -> Self {
        Self {
            seat: name.to_string(),
            holder: name.to_string(),
            source: source.to_string(),
        }
    }

    fn tagged(seat: String, tag: &str, source: String) -> Self {
        Self {
            holder: format!("{seat}-{tag}"),
            seat,
            source,
        }
    }
}

/// What the MCP client said at initialize, kept for every tool call after.
static ANNOUNCED: std::sync::OnceLock<Seat> = std::sync::OnceLock::new();

/// A name as a seat: lower case, runs of letters and digits joined by one
/// hyphen. `Acme CLI`, `acme-cli` and `acme_cli/1.2` are one seat.
#[must_use]
pub fn seat_slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "runner".to_string()
    } else {
        out
    }
}

/// A short tag for one conversation from the process that runs it: the pid
/// in base 36, so `acme-cli-39u` reads as a name and not a number.
#[must_use]
pub fn conversation_tag(pid: u32) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut n = u64::from(pid);
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The login's runtime directory, where what belongs to a session and never
/// to the pack is kept.
fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ljos")
}

/// The record a server leaves for the shells the same runner opens.
fn seat_record_path(runner_pid: u32) -> PathBuf {
    runtime_dir().join(format!("seat-{runner_pid}"))
}

/// The process that started this one. For `ljos-mcp` that is the runner,
/// and the runner is also above every shell it opens.
#[must_use]
pub fn runner_pid() -> u32 {
    // SAFETY: getppid reads one field of the calling process and cannot fail.
    let ppid = unsafe { libc::getppid() };
    u32::try_from(ppid).unwrap_or(0)
}

/// One tool call answered by a fresh `ljos-mcp`: start `program` with
/// `marker` set, send it the client's initialize (`init`, or a plain one),
/// the initialized notification and `tools/call` with `params`, and return
/// the JSON-RPC answer to the call, `result` or `error`.
///
/// # Errors
///
/// The program not starting, or closing before it answers.
pub fn mcp_forward(
    program: &Path,
    marker: &str,
    init: Option<Value>,
    params: Value,
) -> Result<Value> {
    use std::io::{BufRead, Write};
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .env(marker, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("{}: spawn", program.display()))?;
    let init = init.unwrap_or_else(|| {
        serde_json::json!({"protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "runner", "version": "0"}})
    });
    let lines = [
        serde_json::json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": init}),
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": params}),
    ];
    {
        let stdin = child.stdin.as_mut().context("forward: stdin closed")?;
        for line in &lines {
            writeln!(stdin, "{line}")?;
        }
    }
    let stdout = child.stdout.take().context("forward: stdout closed")?;
    let mut answer = None;
    for line in std::io::BufReader::new(stdout).lines() {
        let Ok(v) = serde_json::from_str::<Value>(&line?) else {
            continue;
        };
        if v["id"] == serde_json::json!(1) {
            answer = Some(v);
            break;
        }
    }
    drop(child.stdin.take());
    let _ = child.wait();
    answer.with_context(|| format!("{}: closed without answering the call", program.display()))
}

/// The conversation ids a runner stamped into this environment, by key:
/// every `*_SESSION_ID` but the login's, sorted so two processes with the
/// same variables agree on the first.
fn stamped_sessions() -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = std::env::vars()
        .filter(|(k, v)| runner_session_var(k, v))
        .map(|(k, v)| (k, v.trim().to_string()))
        .collect();
    found.sort();
    found
}

/// A conversation tag from a stamped id: ten base-36 digits of FNV-1a over
/// the whole id. A prefix of the id would not do: a UUID v7 opens with its
/// timestamp, so two conversations started in one window share it.
#[must_use]
pub fn session_tag(id: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.trim().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    for _ in 0..10 {
        out.push(DIGITS[(h % 36) as usize]);
        h /= 36;
    }
    String::from_utf8(out).unwrap_or_default()
}

/// The record a server leaves under a conversation's stamped id, for the
/// shells that carry the same id and whatever else their line editor adds.
fn session_record_path(id: &str) -> PathBuf {
    runtime_dir().join(format!("session-{}", session_tag(id)))
}

/// A record is the seat, the holder, and the conversation ids its writer
/// carried. A shell's line editor stamps one id into every conversation
/// started from that terminal; the ids line is how a reader tells its own
/// conversation's record from another's filed under the same shared id.
fn write_record(path: &Path, seat: &Seat) {
    let ids: Vec<String> = stamped_sessions().into_iter().map(|(_, id)| id).collect();
    write_record_ids(path, seat, &ids);
}

fn write_record_ids(path: &Path, seat: &Seat, ids: &[String]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(
        path,
        format!("{}\n{}\nids\t{}\n", seat.seat, seat.holder, ids.join("\t")),
    );
}

fn read_record(path: &Path, source: String) -> Option<Seat> {
    let text = std::fs::read_to_string(path).ok()?;
    let mine: Vec<String> = stamped_sessions().into_iter().map(|(_, id)| id).collect();
    record_for(&text, &mine, source)
}

/// The seat in a record's text, unless its writer carried a conversation id
/// this process does not: that record is another conversation's, filed
/// under an id both happen to share. A record without an ids line predates
/// the check and is taken as it stands.
fn record_for(text: &str, mine: &[String], source: String) -> Option<Seat> {
    let mut lines = text.lines();
    let (seat, holder) = (lines.next()?, lines.next()?);
    if let Some(ids) = lines.next().and_then(|l| l.strip_prefix("ids")) {
        let foreign = ids
            .split('\t')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .any(|id| !mine.iter().any(|m| m == id));
        if foreign {
            return None;
        }
    }
    Some(Seat {
        seat: seat.to_string(),
        holder: holder.to_string(),
        source,
    })
}

/// Names an MCP library sends when the runner gives none. They name the
/// library, not the runner, and every runner built on it would share one
/// seat.
const LIBRARY_CLIENT_NAMES: &[&str] = &["mcp", "mcp-client", "client", "runner"];

/// The seat a connecting client names: its own name, unless that is a
/// library's default; then the program above this server, else `runner`.
fn seat_for_client(client: &str) -> String {
    let name = seat_slug(client);
    if let Some(runner) = runner_for_client(&harnesses_path(), &name) {
        return runner;
    }
    if !LIBRARY_CLIENT_NAMES.contains(&name.as_str()) {
        return name;
    }
    ancestry()
        .into_iter()
        .find(|(_, comm)| !WRAPPERS.contains(&comm.as_str()))
        .map(|(pid, comm)| seat_slug(&program_name(pid, &comm)))
        .unwrap_or(name)
}

/// The harness a client name belongs to, by its `clients` list in the
/// runners file.
fn runner_for_client(file: &Path, slug: &str) -> Option<String> {
    harnesses_from(file)
        .ok()?
        .harness
        .into_iter()
        .find_map(|h| {
            h.clients
                .iter()
                .any(|c| seat_slug(c) == slug)
                .then(|| seat_slug(&h.name))
        })
}

/// The seat of a record another seat left under one of this process's
/// conversation ids. A runner started from a shell of another runner
/// inherits that runner's ids; the record they find is the parent's.
fn inherited_record(name: &str) -> Option<Seat> {
    stamped_sessions().into_iter().find_map(|(_, id)| {
        read_record(&session_record_path(&id), String::new()).filter(|s| s.seat != name)
    })
}

tokio::task_local! {
    /// The seat of one MCP call whose runner named its thread on the call.
    static CALL_SEAT: Seat;
}

/// Run `f` as the thread a runner named on this call, when it named one.
/// A runner that spawns one server for many conversations names each in
/// the call's metadata rather than in the server's environment.
pub async fn as_thread<F: std::future::Future>(thread: Option<String>, f: F) -> F::Output {
    match thread.filter(|t| t.trim().len() >= 8) {
        Some(t) => CALL_SEAT.scope(seat_for_thread(&t), f).await,
        None => f.await,
    }
}

/// The seat for a thread a runner named on a call. The holder is the one a
/// shell of that thread already took, found by the thread's record; else
/// the thread id whole, recorded so the thread's shells find it.
#[must_use]
pub fn seat_for_thread(thread: &str) -> Seat {
    let thread = thread.trim();
    let seat = named_var("LJOS_SEAT")
        .or_else(|| ANNOUNCED.get().map(|s| s.seat.clone()))
        .unwrap_or_else(login_user);
    let path = session_record_path(thread);
    if let Some(holder) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| holder_naming(&t, thread))
    {
        return Seat {
            seat,
            holder,
            source: "the thread the runner named on this call, as its shells hold it".into(),
        };
    }
    let found = Seat {
        seat,
        holder: thread.to_string(),
        source: "the thread the runner named on this call".into(),
    };
    write_record_ids(&path, &found, &[thread.to_string()]);
    found
}

/// The holder in a record whose ids line names `id`.
fn holder_naming(text: &str, id: &str) -> Option<String> {
    let mut lines = text.lines();
    let (_, holder) = (lines.next()?, lines.next()?);
    let ids = lines.next()?.strip_prefix("ids")?;
    ids.split('\t')
        .any(|i| i.trim() == id)
        .then(|| holder.to_string())
}

/// The MCP server, once a client has said who it is: the seat is the
/// client's name. The holder is any `*_SESSION_ID` the runner stamped,
/// else that seat tagged with the runner's process. The record under the
/// runtime directory is how `ljos` in a shell the same runner opened
/// names the same seat and holder. A runner started from another runner's
/// shell carries that runner's ids; it holds under its own process and
/// leaves the parent's records alone.
pub fn announce_seat(client: &str, runner_pid: u32) -> Seat {
    let name = seat_for_client(client);
    if let Some(parent) = inherited_record(&name) {
        let seat = Seat::tagged(
            name,
            &conversation_tag(runner_pid),
            format!(
                "the client that connected, process {runner_pid}, inside {}",
                parent.seat
            ),
        );
        write_record(&seat_record_path(runner_pid), &seat);
        let _ = ANNOUNCED.set(seat.clone());
        return seat;
    }
    let seat = if let Some((holder, keys)) = session_actor() {
        Seat {
            seat: name,
            holder,
            source: format!("the client that connected, process {runner_pid}; session {keys}"),
        }
    } else {
        Seat::tagged(
            name,
            &conversation_tag(runner_pid),
            format!("the client that connected, process {runner_pid}"),
        )
    };
    // One record by the runner's process, one by each conversation id the
    // runner stamped: a shell whose line editor stamps an id of its own
    // still shares one with the server, and finds this seat by it.
    write_record(&seat_record_path(runner_pid), &seat);
    for (_, id) in stamped_sessions() {
        write_record(&session_record_path(&id), &seat);
    }
    let _ = ANNOUNCED.set(seat.clone());
    seat
}

/// Drop the records [`announce_seat`] wrote, when the server ends.
pub fn retire_seat(runner_pid: u32) {
    let mine = read_record(&seat_record_path(runner_pid), String::new());
    let _ = std::fs::remove_file(seat_record_path(runner_pid));
    for (_, id) in stamped_sessions() {
        let path = session_record_path(&id);
        // Another seat's record under an inherited id stays for its owner.
        let theirs = read_record(&path, String::new())
            .is_some_and(|r| mine.as_ref().is_some_and(|m| m.holder != r.holder));
        if !theirs {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The seat a server announced for one of the conversation ids this
/// process carries. A shell's line editor may add a session id of its
/// own; any one shared id is enough.
fn seat_from_session_records() -> Option<Seat> {
    stamped_sessions().into_iter().find_map(|(key, id)| {
        read_record(
            &session_record_path(&id),
            format!("this conversation's record, session {key}"),
        )
    })
}

/// A process's parent and its own short name, from procfs.
#[cfg(target_os = "linux")]
fn parent_and_comm(pid: u32) -> Option<(u32, String)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?.to_string();
    let ppid = stat
        .get(close + 2..)?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some((ppid, comm))
}

#[cfg(not(target_os = "linux"))]
fn parent_and_comm(_pid: u32) -> Option<(u32, String)> {
    None
}

/// The processes above this one, nearest first, as (pid, name); stops
/// below init.
fn ancestry() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let mut pid = std::process::id();
    for _ in 0..32 {
        let Some((ppid, _)) = parent_and_comm(pid) else {
            break;
        };
        if ppid <= 1 {
            break;
        }
        let Some((_, comm)) = parent_and_comm(ppid) else {
            break;
        };
        out.push((ppid, comm));
        pid = ppid;
    }
    out
}

/// Programs that run other programs and are nobody's seat.
const WRAPPERS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "csh", "nu", "env", "sudo", "doas",
    "timeout", "nohup", "xargs", "script", "uv", "direnv", "ljos", "ljos-mcp",
];

/// Where a process tree stops being a program and becomes the session
/// itself: above these, nobody ran the shell but the person.
const SESSION: &[&str] = &[
    "tmux", "screen", "zellij", "systemd", "init", "sshd", "login",
];

/// Path components that name a place, not a program.
const PLACES: &[&str] = &[
    "bin",
    "sbin",
    "versions",
    "current",
    "dist",
    "build",
    "target",
    "release",
    "debug",
    "node_modules",
    ".bin",
    "lib",
    "libexec",
    "app",
    "resources",
];

/// Interpreters run a program named by their first argument.
const INTERPRETERS: &[&str] = &[
    "node", "nodejs", "bun", "deno", "python", "python3", "ruby", "perl", "java",
];

fn version_like(s: &str) -> bool {
    let t = s.strip_prefix('v').unwrap_or(s);
    t.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// A program's name from how it was started: the last path component of
/// what ran that is neither a version (`2.1.266`) nor a place (`bin`,
/// `versions`); for an interpreter, the script it was handed. Falls back
/// to the kernel's short name.
#[cfg(target_os = "linux")]
fn program_name(pid: u32, comm: &str) -> String {
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let args: Vec<String> = cmdline
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let mut candidates: Vec<&str> = Vec::new();
    if let Some(first) = args.first() {
        let base = Path::new(first)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(first);
        if INTERPRETERS.contains(&base) {
            if let Some(script) = args.iter().skip(1).find(|a| !a.starts_with('-')) {
                candidates.push(script);
            }
        }
        candidates.push(first);
    }
    for path in candidates {
        let mut parts: Vec<&str> = Path::new(path)
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();
        while let Some(last) = parts.pop() {
            let name = last.rsplit_once('.').map_or(last, |(stem, ext)| {
                if ["js", "mjs", "cjs", "py", "rb", "pl", "jar", "exe"].contains(&ext) {
                    stem
                } else {
                    last
                }
            });
            if name.is_empty() || version_like(name) || PLACES.contains(&name) || name == "/" {
                continue;
            }
            if name.starts_with('.') || name.contains(std::path::MAIN_SEPARATOR) {
                continue;
            }
            return name.to_string();
        }
    }
    comm.to_string()
}

#[cfg(not(target_os = "linux"))]
fn program_name(_pid: u32, comm: &str) -> String {
    comm.to_string()
}

/// The seat from the process tree: the record a server left for the runner
/// above this shell, else the nearest ancestor that is neither a shell nor
/// a wrapper, named from how it was started and tagged with its pid. None
/// when the tree ends in the session itself, which is a person at a
/// terminal.
fn seat_from_tree() -> Option<Seat> {
    if let Some(seat) = seat_from_tree_records() {
        return Some(seat);
    }
    let chain = ancestry();
    for (pid, comm) in &chain {
        let name = comm.as_str();
        if WRAPPERS.contains(&name) {
            continue;
        }
        if SESSION.iter().any(|s| name.starts_with(s)) {
            return None;
        }
        let program = program_name(*pid, name);
        return Some(Seat::tagged(
            seat_slug(&program),
            &conversation_tag(*pid),
            format!("the process tree, {program} {pid}"),
        ));
    }
    None
}

/// The record a server left for the nearest runner above this shell. It
/// names the runner that opened the shell, which a conversation id in the
/// environment does not when one runner started another.
fn seat_from_tree_records() -> Option<Seat> {
    ancestry().into_iter().find_map(|(pid, _)| {
        read_record(
            &seat_record_path(pid),
            format!("the server the runner opened, process {pid}"),
        )
    })
}

fn named_var(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && !omitted_actor_name(v))
}

/// Who is sitting, with nothing set. The seat: `LJOS_SEAT` or the
/// tracker's `VISSUE_AGENT` when someone set one; else what the MCP client
/// said at initialize; else the process tree above this shell, which is
/// the runner that opened it or the server that runner opened; else the
/// login user, who is the seat when no program is. The holder is any
/// `*_SESSION_ID` the runner stamped, ahead of the process tag, so MCP
/// sitting and CLI sitting of one conversation are one occupancy name;
/// else the seat tagged with the conversation's process.
#[must_use]
pub fn whoami() -> Seat {
    if let Ok(seat) = CALL_SEAT.try_with(Clone::clone) {
        return seat;
    }
    let session = session_actor();
    // Both variables are a person naming the seat: the seat's own, and the
    // tracker's name for the same thing. Either beats what the tree says.
    let named = named_var("LJOS_SEAT")
        .map(|n| (n, "LJOS_SEAT"))
        .or_else(|| named_var("VISSUE_AGENT").map(|n| (n, "VISSUE_AGENT")));
    // The record filed under a conversation id this shell carries, unless
    // the nearest runner above left one for another seat: a runner started
    // from another runner's shell inherits the other's ids, and its own
    // record is the one above it.
    let record = seat_from_session_records().map(|by_id| {
        seat_from_tree_records()
            .filter(|above| above.seat != by_id.seat)
            .unwrap_or(by_id)
    });
    let program = ANNOUNCED
        .get()
        .cloned()
        .or_else(|| record.clone())
        .or_else(seat_from_tree);
    let agent = named_var("VISSUE_AGENT");
    let seat_name = named
        .as_ref()
        .map(|(n, _)| n.clone())
        .or_else(|| program.as_ref().map(|p| p.seat.clone()))
        .or_else(|| agent.clone())
        .unwrap_or_else(login_user);
    // The server's record first: it carries the holder the server took,
    // whatever else this shell's environment adds.
    if let Some(record) = record {
        return Seat {
            seat: seat_name,
            holder: record.holder,
            source: record.source,
        };
    }
    if let Some((holder, keys)) = session {
        let seat = Seat {
            seat: seat_name,
            holder,
            source: keys,
        };
        // The first resolution in a conversation leaves a record under
        // every id stamped so far; a later process carrying one of them and
        // more finds this holder by the shared id rather than hashing the
        // larger set into a new name. The tests stamp ids of their own
        // into one process and must not leave records for each other.
        #[cfg(not(test))]
        for (_, id) in stamped_sessions() {
            write_record(&session_record_path(&id), &seat);
        }
        return seat;
    }
    match (&named, &program) {
        (Some((name, key)), Some(p)) => Seat {
            seat: name.clone(),
            holder: p.holder.replacen(&p.seat, name, 1),
            source: format!("{key}, held by {}", p.source),
        },
        (Some((name, key)), None) => Seat::whole(name, key),
        (None, Some(p)) => p.clone(),
        (None, None) => {
            if let Some(name) = agent {
                Seat::whole(&name, "VISSUE_AGENT")
            } else {
                Seat::whole(&login_user(), "the login user")
            }
        }
    }
}

/// The person at the terminal, when no program is the seat.
fn login_user() -> String {
    std::env::var("USER")
        .ok()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "seat".to_string())
}

/// The name this seat remembers, votes and earns trust under.
#[must_use]
pub fn seat_name() -> String {
    whoami().seat
}

/// The name this conversation's claims are held under.
#[must_use]
pub fn holder_name() -> String {
    whoami().holder
}

/// Resolve an `--assignee` / MCP field for a claim. Empty, a pronoun
/// (`seat`, `you`, `agent`), or this process naming itself is omitted:
/// occupancy is the conversation's holder, not the product name on the
/// box. A named worker is taken as given.
#[must_use]
pub fn resolve_assignee(passed: Option<&str>) -> String {
    match passed.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) if !omitted_actor_name(n) && !own_seat(n) => n.to_string(),
        _ => holder_name(),
    }
}

/// Occupancy is always `{name}:{issue}`. One live claim per name is what
/// made two conversations unseat each other; the issue is already
/// exclusive. Already-scoped names (they contain `:`) are left alone.
#[must_use]
pub fn occupancy_assignee(passed: Option<&str>, issue: &str) -> String {
    occupancy_scope(&resolve_assignee(passed), issue)
}

fn occupancy_scope(assignee: &str, issue: &str) -> String {
    let issue = issue.trim();
    if issue.is_empty() || assignee.contains(':') {
        assignee.to_string()
    } else {
        format!("{assignee}:{issue}")
    }
}

/// The doctor's `seat` row: who votes, who holds, and where the names came
/// from.
#[must_use]
pub fn format_seat_row() -> String {
    let who = whoami();
    format!(
        "{}, holding as {} (from {})",
        who.seat, who.holder, who.source
    )
}

/// `ljos seat`: who is sitting, one field a line.
#[must_use]
pub fn format_seat(seat: &Seat) -> String {
    format!(
        "seat\t{}\nholder\t{}\nsource\t{}\n",
        seat.seat, seat.holder, seat.source
    )
}

/// Whether a runner with a `registered` command already has the server.
fn is_registered(h: &Harness, server: &Path) -> Option<bool> {
    if !h.registered.is_empty() {
        let argv = filled(&h.registered, server, &h.name);
        return Some(
            argv.first().is_some_and(|bin| on_path(bin)) && {
                let (bin, rest) = (&argv[0], &argv[1..]);
                run_captured(bin, rest).is_ok()
            },
        );
    }
    if let (Some(config), Some(marker)) = (&h.config, &h.marker) {
        return Some(std::fs::read_to_string(expand(config)).is_ok_and(|t| t.contains(marker)));
    }
    if let (Some(config), Some(pointer)) = (&h.config_json, &h.json_pointer) {
        return Some(
            std::fs::read_to_string(expand(config))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .is_some_and(|doc| doc.pointer(pointer).is_some()),
        );
    }
    None
}

/// Set `pointer` in the JSON document at `config` to `entry`, making the
/// objects on the way; a missing file starts as `{}`.
fn set_json_entry(config: &Path, pointer: &str, entry: &Value) -> Result<()> {
    let mut doc: Value = match std::fs::read_to_string(config) {
        Ok(t) if !t.trim().is_empty() => {
            serde_json::from_str(&t).with_context(|| format!("{}: not JSON", config.display()))?
        }
        _ => serde_json::json!({}),
    };
    let mut at = &mut doc;
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let (last, path) = parts
        .split_last()
        .context("onboard: an empty JSON pointer")?;
    for key in path {
        at = at
            .as_object_mut()
            .context("onboard: the pointer crosses a value that is not an object")?
            .entry((*key).to_string())
            .or_insert_with(|| serde_json::json!({}));
    }
    at.as_object_mut()
        .context("onboard: the pointer's parent is not an object")?
        .insert((*last).to_string(), entry.clone());
    if let Some(parent) = config.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(&doc)?;
    text.push('\n');
    std::fs::write(config, text)?;
    Ok(())
}

/// Grok watches `[mcp_servers.ljos.env]`. Changing `LJOS_MCP_GENERATION`
/// respawns the server; a session restart is not required.
fn bump_ljos_mcp_generation(config: &Path, version: &str, dry: bool) -> Result<Option<String>> {
    let text = match std::fs::read_to_string(config) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    let mut changed = false;
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rhs) = trimmed.strip_prefix("LJOS_MCP_GENERATION") {
            let rhs = rhs.trim_start().strip_prefix('=').unwrap_or("").trim();
            let val = rhs.trim_matches(|c| c == '"' || c == '\'');
            if val == version {
                out.push_str(line);
            } else {
                let indent_len = line.len() - trimmed.len();
                out.push_str(&line[..indent_len]);
                out.push_str("LJOS_MCP_GENERATION = \"");
                out.push_str(version);
                out.push('"');
                changed = true;
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !changed {
        return Ok(None);
    }
    if dry {
        return Ok(Some(version.to_string()));
    }
    std::fs::write(config, out).with_context(|| config.display().to_string())?;
    Ok(Some(version.to_string()))
}

fn register_step(h: &Harness, server: &Path, dry: bool) -> Step {
    let what = format!("{} mcp", h.name);
    match is_registered(h, server) {
        Some(true) => {
            let config = expand(h.config.as_deref().unwrap_or_default());
            match bump_ljos_mcp_generation(&config, env!("CARGO_PKG_VERSION"), dry) {
                Ok(Some(v)) => Step {
                    what,
                    detail: format!("ljos registered; MCP generation {v}"),
                    ok: true,
                },
                Ok(None) => Step {
                    what,
                    detail: "ljos registered".into(),
                    ok: true,
                },
                Err(e) => Step {
                    what,
                    detail: format!("ljos registered; generation {e}"),
                    ok: false,
                },
            }
        }
        None => Step {
            what,
            detail: "no register or config in harnesses.toml; paste `ljos onboard --harness json`"
                .into(),
            ok: false,
        },
        Some(false) if !h.register.is_empty() => {
            let argv = filled(&h.register, server, &h.name);
            if !on_path(&argv[0]) {
                return Step {
                    what,
                    detail: format!("{} not on PATH", argv[0]),
                    ok: false,
                };
            }
            if dry {
                return Step {
                    what,
                    detail: format!("would run {}", argv.join(" ")),
                    ok: true,
                };
            }
            match run_captured(&argv[0], &argv[1..]) {
                Ok(_) => Step {
                    what,
                    detail: format!("ran {}", argv.join(" ")),
                    ok: true,
                },
                Err(e) => Step {
                    what,
                    detail: e.to_string().lines().next().unwrap_or("").to_string(),
                    ok: false,
                },
            }
        }
        Some(false) if h.config_json.is_some() => {
            let config = expand(h.config_json.as_deref().unwrap_or_default());
            let pointer = h.json_pointer.clone().unwrap_or_default();
            let entry_text = h
                .json_entry
                .as_deref()
                .unwrap_or_default()
                .replace("{server}", &server.display().to_string())
                .replace("{name}", &h.name);
            let entry: Value = match serde_json::from_str(&entry_text) {
                Ok(v) => v,
                Err(e) => {
                    return Step {
                        what,
                        detail: format!("json_entry is not JSON: {e}"),
                        ok: false,
                    }
                }
            };
            if dry {
                return Step {
                    what,
                    detail: format!("would set {pointer} in {}", config.display()),
                    ok: true,
                };
            }
            match set_json_entry(&config, &pointer, &entry) {
                Ok(()) => Step {
                    what,
                    detail: format!("set {pointer} in {}", config.display()),
                    ok: true,
                },
                Err(e) => Step {
                    what,
                    detail: format!("{}: {e}", config.display()),
                    ok: false,
                },
            }
        }
        Some(false) => {
            let config = expand(h.config.as_deref().unwrap_or_default());
            let snippet = h
                .snippet
                .as_deref()
                .unwrap_or_default()
                .replace("{server}", &server.display().to_string())
                .replace("{name}", &h.name);
            if snippet.is_empty() {
                return Step {
                    what,
                    detail: format!("no snippet to append to {}", config.display()),
                    ok: false,
                };
            }
            if dry {
                return Step {
                    what,
                    detail: format!("would append the entry to {}", config.display()),
                    ok: true,
                };
            }
            let mut text = std::fs::read_to_string(&config).unwrap_or_default();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&snippet);
            let written = config
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&config, text));
            match written {
                Ok(()) => Step {
                    what,
                    detail: format!("appended the entry to {}", config.display()),
                    ok: true,
                },
                Err(e) => Step {
                    what,
                    detail: format!("{}: {e}", config.display()),
                    ok: false,
                },
            }
        }
    }
}

/// Register the server and install the skill for one runner named in the
/// runners file. `json` registers nothing and returns the entry to paste.
/// `dry` reports without writing.
///
/// # Errors
///
/// No such runner in the file, no home directory, or `ljos-mcp` not on `PATH`.
pub fn onboard(harness: &str, dry: bool) -> Result<Vec<Step>> {
    onboard_from(&harnesses_path(), harness, dry)
}

/// Frozen Grok hook file. Copied to `~/.grok/hooks/ljos.json`.
const GROK_HOOKS_JSON: &str = include_str!("../assets/grok/ljos.json");

/// The `ljos` a runner's hook runs: the one beside `ljos-mcp`, by absolute
/// path, since a runner started outside a login shell has no `~/.local/bin`
/// on its PATH.
fn ljos_path() -> Result<PathBuf> {
    let beside = server_path()?.with_file_name("ljos");
    if beside.is_file() {
        return Ok(beside);
    }
    which::which("ljos").context("ljos not on PATH")
}

/// The grok hooks file with `{ljos}` filled in.
fn grok_hooks_json(ljos: &Path) -> String {
    GROK_HOOKS_JSON.replace("{ljos}", &ljos.display().to_string())
}

fn write_grok_hooks(dry: bool) -> Result<Step> {
    let dest = home()?.join(".grok/hooks/ljos.json");
    if dry {
        return Ok(Step {
            what: "hook".into(),
            detail: format!("would write {}", dest.display()),
            ok: true,
        });
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&dest, grok_hooks_json(&ljos_path()?))?;
    Ok(Step {
        what: "hook".into(),
        detail: format!("wrote {}", dest.display()),
        ok: true,
    })
}

pub fn onboard_from(file: &Path, harness: &str, dry: bool) -> Result<Vec<Step>> {
    if harness == "json" {
        return Ok(vec![Step {
            what: "json".into(),
            detail: serde_json::to_string_pretty(&server_entry()?)?,
            ok: true,
        }]);
    }
    if harness == "grok" {
        let mut steps = vec![write_grok_hooks(dry)?];
        if let Ok(all) = harnesses_from(file) {
            if let Some(h) = all.harness.iter().find(|h| h.name == "grok") {
                let server = server_path()?;
                steps.push(register_step(h, &server, dry));
                if let Some(dir) = &h.skills {
                    steps.push(write_skill(&expand(dir), dry));
                }
            }
        }
        return Ok(steps);
    }
    let all = harnesses_from(file)?;
    let Some(h) = all.harness.iter().find(|h| h.name == harness) else {
        let names: Vec<&str> = all.harness.iter().map(|h| h.name.as_str()).collect();
        bail!(
            "onboard: no runner {harness:?} in {}; it names {}. `ljos onboard --example` \
             prints the file's shape, and `--harness json` prints the entry to paste anywhere.",
            file.display(),
            if names.is_empty() {
                "none".to_string()
            } else {
                names.join(", ")
            }
        );
    };
    let server = server_path()?;
    let dependencies = [pack_step(dry), host_key_step(dry)];
    let mut steps = vec![register_step(h, &server, dry)];
    if let Some(file) = &h.hooks {
        steps.push(hook_step(&expand(file), &hook_events_of(h), dry));
    }
    if let Some(dest) = &h.plugin {
        steps.push(plugin_step(h, &expand(dest), dry));
    }
    match &h.skills {
        Some(dir) => steps.push(write_skill(&expand(dir), dry)),
        None => steps.push(Step {
            what: "skill".into(),
            detail: "no skills directory in harnesses.toml; `ljos protocol` prints the text".into(),
            ok: false,
        }),
    }
    steps.extend(dependencies);
    Ok(steps)
}

/// The events the memory hook fires on when a runner's table names none:
/// the prompt, which carries the task in the person's words. A tool call
/// carries the command about to run and is a cue too; a runner asks for it
/// with `hook_events`. The default came out of a panel of this seat's
/// personas: a turn issues many shell commands and one prompt.
pub const HOOK_EVENTS: &[&str] = &["UserPromptSubmit", "SessionEnd"];

/// The events the hook knows a matcher for; any other event takes `*`.
pub const HOOK_MATCHERS: &[(&str, &str)] = &[
    ("PreToolUse", "Bash"),
    ("PostToolUse", "*"),
    ("UserPromptSubmit", "*"),
    ("SessionEnd", "*"),
    ("SubagentStop", "*"),
];

/// One runner sends snake_case `hookEventName`; another sends
/// PascalCase `hook_event_name`. One name in the seat.
fn normalize_hook_event(raw: &str) -> &str {
    match raw {
        "pre_llm_call" => "UserPromptSubmit",
        "pre_tool_call" => "PreToolUse",
        "post_tool_call" => "PostToolUse",
        // One runner fires on_session_end after every turn; its session
        // ends on finalize or reset.
        "on_session_finalize" | "on_session_reset" => "SessionEnd",
        "on_session_end" => "TurnEnd",
        "pre_tool_use" | "PreToolUse" => "PreToolUse",
        "post_tool_use" | "PostToolUse" => "PostToolUse",
        "user_prompt_submit" | "UserPromptSubmit" => "UserPromptSubmit",
        "session_end" | "SessionEnd" => "SessionEnd",
        "session_start" | "SessionStart" => "SessionStart",
        "subagent_stop" | "SubagentStop" | "SubagentEnd" | "subagentStop" => "SubagentStop",
        other => other,
    }
}

fn hook_matcher(event: &str) -> &'static str {
    HOOK_MATCHERS
        .iter()
        .find(|(e, _)| *e == event)
        .map_or("*", |(_, m)| m)
}

/// The events a runner's table asks for, or the default.
fn hook_events_of(h: &Harness) -> Vec<String> {
    if h.name == "grok" {
        return [
            "UserPromptSubmit",
            "PostToolUse",
            "PreToolUse",
            "SessionEnd",
            "SubagentStop",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
    }
    if h.hook_events.is_empty() {
        HOOK_EVENTS.iter().map(|e| (*e).to_string()).collect()
    } else {
        h.hook_events.clone()
    }
}

fn is_seat_hook(h: &Value) -> bool {
    h["command"]
        .as_str()
        .is_some_and(|c| c.contains("ljos") && c.ends_with(" hook"))
}

/// The command the runner's hook runs.
fn hook_command() -> String {
    which::which("ljos").map_or_else(
        |_| "ljos hook".to_string(),
        |p| format!("{} hook", p.display()),
    )
}

/// Merge the seat's memory hook into a runner's hooks file, once per event.
/// The file is JSON with a `hooks` object of event name to matcher groups;
/// a group whose command is the seat's is left alone, so the step is
/// idempotent.
fn hook_step(file: &Path, events: &[String], dry: bool) -> Step {
    let what = "hook".to_string();
    let mut root: Value = match std::fs::read_to_string(file) {
        Ok(text) if !text.trim().is_empty() => match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                return Step {
                    what,
                    detail: format!("{}: not JSON: {e}", file.display()),
                    ok: false,
                }
            }
        },
        _ => serde_json::json!({}),
    };
    let command = hook_command();
    let Some(obj) = root.as_object_mut() else {
        return Step {
            what,
            detail: format!("{}: not a JSON object", file.display()),
            ok: false,
        };
    };
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Step {
            what,
            detail: format!("{}: hooks is not an object", file.display()),
            ok: false,
        };
    };
    // Reconcile: the seat's hook is on the events asked for and on no
    // other, and every group that is not the seat's is left alone.
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for event in events {
        let groups = hooks
            .entry(event.clone())
            .or_insert_with(|| serde_json::json!([]));
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let present = groups.iter().any(|g| {
            g["hooks"]
                .as_array()
                .into_iter()
                .flatten()
                .any(is_seat_hook)
        });
        if present {
            continue;
        }
        groups.push(serde_json::json!({
            "matcher": hook_matcher(event),
            "hooks": [{"type": "command", "command": command, "timeout": 20}]
        }));
        added.push(event.clone());
    }
    for (event, groups) in hooks.iter_mut() {
        if events.contains(event) {
            continue;
        }
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let before = groups.len();
        groups.retain(|g| {
            !g["hooks"]
                .as_array()
                .into_iter()
                .flatten()
                .any(is_seat_hook)
        });
        if groups.len() != before {
            removed.push(event.clone());
        }
    }
    if added.is_empty() && removed.is_empty() {
        return Step {
            what,
            detail: format!(
                "{} carries the memory hook on {}",
                file.display(),
                events.join(", ")
            ),
            ok: true,
        };
    }
    let mut change = Vec::new();
    if !added.is_empty() {
        change.push(format!("add it on {}", added.join(", ")));
    }
    if !removed.is_empty() {
        change.push(format!("drop it from {}", removed.join(", ")));
    }
    let change = change.join(" and ");
    if dry {
        return Step {
            what,
            detail: format!("would {change} in {}", file.display()),
            ok: true,
        };
    }
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| serde_json::to_string_pretty(&root).map_err(std::io::Error::other))
        .and_then(|text| std::fs::write(file, text + "\n"));
    match written {
        Ok(()) => Step {
            what,
            detail: format!("memory hook: {change} in {}", file.display()),
            ok: true,
        },
        Err(e) => Step {
            what,
            detail: format!("{}: {e}", file.display()),
            ok: false,
        },
    }
}

/// Whether a runner's hooks file carries the memory hook on every event.
fn hook_installed(file: &Path, events: &[String]) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    events.iter().all(|event| {
        root["hooks"][event.as_str()]
            .as_array()
            .into_iter()
            .flatten()
            .any(|g| {
                g["hooks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(is_seat_hook)
            })
    })
}

/// What the runner's hook hands the seat: the event, and the text worth
/// asking the pack about. From a tool call, the command about to run; from
/// a prompt, the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCall {
    pub event: String,
    pub cue: String,
    /// The runner's session, when it says: each memory is injected once
    /// per session, so the same lesson does not arrive on every command.
    pub session: Option<String>,
    /// The hook contract the call arrived in; it decides how a
    /// verdict is written back.
    pub shape: HookShape,
}

/// The hook contract a call arrived in, told apart by its stdin. The
/// runners share one name for the answer, `permissionDecision`, but not
/// what they do with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookShape {
    /// snake_case stdin; `permissionDecision` takes `deny` or `ask`.
    #[default]
    Asks,
    /// snake_case stdin carrying `turn_id`; `deny` only, and an `ask` is
    /// rejected as unsupported and the tool runs.
    DenyOnly,
    /// camelCase stdin (`hookEventName`, `toolInput`); a top-level
    /// `decision` blocks, and there is no `ask`.
    CamelCase,
    /// lower-case event names (`pre_llm_call`, `pre_tool_call`) with the
    /// prompt under `extra.user_message`; a top-level `context` is
    /// injected, `decision: block` blocks, and there is no `ask`.
    Context,
}

impl HookShape {
    /// Whether the runner can stop and ask the person on a verdict.
    #[must_use]
    pub fn asks(self) -> bool {
        self == Self::Asks
    }
}

/// Read a hook call from the runner's JSON, or from plain text (an argv
/// under argv law). Fields: `hook_event_name`, `tool_name`, `tool_input`
/// (its `command`, else every string value joined), `prompt`; grok's
/// camelCase `hookEventName`, `sessionId` and `toolInput` read the same.
#[must_use]
pub fn hook_call(input: &str) -> HookCall {
    let trimmed = input.trim();
    let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
        return HookCall {
            event: "argv".into(),
            cue: trimmed.to_string(),
            session: None,
            shape: HookShape::Asks,
        };
    };
    let raw_event = v["hook_event_name"].as_str().unwrap_or("");
    let shape = if v.get("hookEventName").is_some() || v.get("toolInput").is_some() {
        HookShape::CamelCase
    } else if raw_event.starts_with("pre_")
        || raw_event.starts_with("post_")
        || raw_event.starts_with("on_")
    {
        HookShape::Context
    } else if v.get("turn_id").is_some() {
        HookShape::DenyOnly
    } else {
        HookShape::Asks
    };
    let input = if v["tool_input"].is_null() {
        &v["toolInput"]
    } else {
        &v["tool_input"]
    };
    let session = v["session_id"]
        .as_str()
        .or_else(|| v["sessionId"].as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let raw = v["hook_event_name"]
        .as_str()
        .or_else(|| v["hookEventName"].as_str())
        .unwrap_or("PreToolUse");
    let event = normalize_hook_event(raw).to_string();
    let cue = if let Some(p) = v["prompt"].as_str() {
        p.to_string()
    } else if let Some(p) = v["extra"]["user_message"].as_str() {
        p.to_string()
    } else if let Some(c) = input["command"].as_str() {
        c.to_string()
    } else if let Some(map) = input.as_object() {
        map.values()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        String::new()
    };
    HookCall {
        event,
        cue,
        session,
        shape,
    }
}

/// Where the ids already injected in a session are kept: the runtime
/// directory, so they go with the login and never into the pack.
fn seen_path(session: &str) -> Option<PathBuf> {
    let safe: String = session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if safe.is_empty() {
        return None;
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ljos");
    Some(dir.join(format!("hook-seen-{safe}")))
}

pub fn seen_ids(session: Option<&str>) -> std::collections::BTreeSet<String> {
    session
        .and_then(seen_path)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// The memories injected during a session, in the order they arrived, and
/// the file they were kept in. The nudge marker is not a memory.
fn injected_ids(session: &str) -> (Vec<String>, Option<PathBuf>) {
    let path = seen_path(session);
    let ids: Vec<String> = path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| {
            t.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && *l != "due-nudge")
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    (ids, path)
}

/// When a session ends, the memories injected during it fire together:
/// they served one sitting, so their links gain weight and the next
/// sitting like it walks a heavier path (Hebb, through the pack's `fire`).
/// The seen file goes with the session. Returns how many fired; nothing to
/// fire, or no pack, is zero and not an error, since a hook must not stop
/// a runner from ending.
pub fn session_end(session: Option<&str>) -> usize {
    let Some(session) = session else {
        return 0;
    };
    let (ids, path) = injected_ids(session);
    let fired = if ids.len() >= 2 {
        let top: Vec<String> = ids.into_iter().take(8).collect();
        pack()
            .ok()
            .and_then(|c| c.fire(&c.workspace(), &top).ok())
            .map_or(0, |_| top.len())
    } else {
        0
    };
    if let Some(p) = path {
        let _ = std::fs::remove_file(p);
    }
    fired
}

/// Where a Grok prompt's pack context waits for `PostToolUse`.
/// Grok discards `UserPromptSubmit` stdout; it delivers
/// `PostToolUse` `additionalContext` after the first tool.
fn hook_hold_path(session: Option<&str>) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TMPDIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let name = session
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .take(32)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".into());
    Some(dir.join(format!("ljos-hook-hold-{name}")))
}

/// Remember the prompt's pack text so the next `PostToolUse` can emit it.
pub fn hold_hook_context(session: Option<&str>, context: &str) {
    let Some(path) = hook_hold_path(session) else {
        return;
    };
    if context.is_empty() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let _ = std::fs::write(path, context);
}

/// Take the held pack text once. Empty if nothing was held.
#[must_use]
pub fn take_hook_context(session: Option<&str>) -> String {
    let Some(path) = hook_hold_path(session) else {
        return String::new();
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    text
}

pub fn mark_seen(session: Option<&str>, ids: &[String]) {
    let Some(path) = session.and_then(seen_path) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    for id in ids {
        text.push_str(id);
        text.push('\n');
    }
    let _ = std::fs::write(path, text);
}

/// The floor a hit must reach, as a share of the strongest hit's score, to
/// be injected. A command line matches many claims weakly; only the ones
/// that match it as well as the best does are worth the agent's context.
pub const HOOK_SCORE_FLOOR: f64 = 0.6;

/// The context the hook injects: the island the cue activates, standing
/// preferences first because they bear on what to do, then lessons. Empty
/// when the pack holds nothing on it or does not answer; a hook that fails
/// must not stop the runner, so this never errors.
#[must_use]
pub fn hook_context(call: &HookCall, limit: usize) -> String {
    let cue = call.cue.trim();
    if cue.len() < 3 {
        return String::new();
    }
    // The nudges answer what the prompt says, not what the pack holds, so
    // a prompt the pack knows nothing about still gets them.
    let mut nudge = due_nudge(call);
    for extra in [correction_nudge(call), decision_nudge(call)]
        .into_iter()
        .flatten()
    {
        if !nudge.is_empty() {
            nudge.push('\n');
        }
        nudge.push_str(&extra);
    }
    let Ok(hits) = packset_search(cue) else {
        return nudge;
    };
    let top = hits.iter().map(|h| h.score).fold(0.0_f64, f64::max);
    if top <= 0.0 {
        return nudge;
    }
    let seen = seen_ids(call.session.as_deref());
    let mut rows: Vec<&Hit> = hits
        .iter()
        .filter(|h| !UNREVIEWED_KINDS.contains(&h.kind.as_str()))
        .filter(|h| h.score >= top * HOOK_SCORE_FLOOR)
        .filter(|h| agreed(h))
        .filter(|h| h.id.as_ref().is_none_or(|id| !seen.contains(id)))
        .collect();
    rows.sort_by(|a, b| {
        let pa = a.kind == "preference";
        let pb = b.kind == "preference";
        pb.cmp(&pa).then(
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    let mut rows: Vec<&Hit> = rows.into_iter().take(limit).collect();
    // Preferences stay in front by score; the lessons behind them run
    // oldest to newest, so what was learnt last is read last and nearest
    // the action, and a later lesson that revises an earlier one reads as
    // a revision.
    let now = now_utc();
    let split = rows.iter().filter(|h| h.kind == "preference").count();
    rows[split..].sort_by_key(|h| days_of_stamp(h.ts.as_deref()).unwrap_or(i64::MAX));
    let lines: Vec<String> = rows.iter().map(|h| hit_line(h, &now)).collect();
    if lines.is_empty() {
        return nudge;
    }
    mark_seen(
        call.session.as_deref(),
        &rows.iter().filter_map(|h| h.id.clone()).collect::<Vec<_>>(),
    );
    let mut out = format!(
        "What this seat already knows that bears on this (from the pack, each with its age, lessons oldest first; `ljos search` for more):\n{}",
        lines.join("\n")
    );
    if !nudge.is_empty() {
        out.push('\n');
        out.push_str(&nudge);
    }
    out
}

/// Whether the pack's scorers agreed on a hit: named by at least two of
/// the ballots that ran. When one ballot ran, or the hit carries no
/// count, it stands. A command line matches many claims weakly on one
/// scorer; what reaches the agent unasked should be what two scorers
/// found.
fn agreed(h: &Hit) -> bool {
    match (h.ballots, h.of) {
        (Some(named), Some(of)) if of >= 2 => named >= 2,
        _ => true,
    }
}

/// What a hook call says about a subagent: its type when the call fired
/// inside one (`subagentType`, or `agent_type`), and whether a stop gate
/// already held it this turn (`stopHookActive`), and the agent's id when
/// the runner shares one session between a parent and its subagents.
#[must_use]
pub fn hook_subagent(input: &str) -> (Option<String>, bool, String) {
    let Ok(v) = serde_json::from_str::<Value>(input.trim()) else {
        return (None, false, String::new());
    };
    let kind = v["subagentType"]
        .as_str()
        .or_else(|| v["subagent_type"].as_str())
        .or_else(|| v["agent_type"].as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let active = v["stopHookActive"]
        .as_bool()
        .or_else(|| v["stop_hook_active"].as_bool())
        .unwrap_or(false);
    let agent = v["agent_id"]
        .as_str()
        .or_else(|| v["agentId"].as_str())
        .unwrap_or("")
        .to_string();
    (kind, active, agent)
}

/// The issue this conversation's holder claimed last and still works: a
/// subagent's hook runs under its parent's holder, so this is the work
/// the subagent is a slice of.
#[must_use]
pub fn held_issue() -> Option<String> {
    // The record the runner's own server left names the holder its claims
    // were made under. A hook's environment can carry session variables
    // the server's did not, which hash to another holder that holds
    // nothing, so the record is asked first.
    let mut holders: Vec<String> = seat_from_tree_records()
        .map(|s| s.holder)
        .into_iter()
        .collect();
    let own = holder_name();
    if !holders.contains(&own) {
        holders.push(own);
    }
    holders.iter().find_map(|holder| {
        let out = run_captured("vissue", &["claims", "--by", holder, "--json"]).ok()?;
        let rows: Value = serde_json::from_str(&out.stdout).ok()?;
        rows.as_array()?
            .iter()
            .filter(|c| c["state"].as_str() == Some("STARTED"))
            .next_back()?["id"]
            .as_str()
            .map(str::to_string)
    })
}

/// What a subagent is told on its first tool result: the issue its parent
/// holds and how its result joins it. A subagent that is not told the
/// issue cannot cast a ballot on it, and a sitting of its own would
/// contend with its parent's.
#[must_use]
pub fn subagent_brief(kind: &str, issue: &str, decision: bool) -> String {
    let judge = if decision {
        format!("{issue} is a decision: end with your ballot, `ljos vote {issue} --for OPTION --as ROLE`.")
    } else {
        format!(
            "A judgement between options is a ballot: `ljos vote {issue} --for OPTION --as ROLE`."
        )
    };
    format!(
        "You are a subagent ({kind}) working under {issue}, which your parent holds. Do not open a sitting \
         on it. {judge} A lesson that will hold next time is `ljos remember \"...\" --as ROLE`; a \
         finding is `vissue note {issue} \"...\"`. ROLE is a persona from `ljos personas` when one fits \
         your task, else `{kind}`."
    )
}

/// The stop gate for a subagent: once, when its parent holds an issue,
/// the reason the subagent is kept working one more round. A gate that
/// already held it this turn, or a parent holding nothing, lets it stop.
#[must_use]
pub fn subagent_stop_reason(
    kind: &str,
    issue: Option<&str>,
    decision: bool,
    active: bool,
) -> Option<String> {
    if active {
        return None;
    }
    let issue = issue?;
    Some(if decision {
        format!(
            "{issue} is a decision your parent holds. Before you stop, cast your ballot: \
             `ljos vote {issue} --for OPTION --as ROLE` (ROLE: your persona, else `{kind}`)."
        )
    } else {
        format!(
            "You worked under {issue}. Before you stop: if your result settles a choice, \
             `ljos vote {issue} --for OPTION --as ROLE`; if it taught something that holds next time, \
             `ljos remember \"...\" --as ROLE`. Otherwise stop."
        )
    })
}

/// Phrases a person uses when the agent has forgotten something it was
/// told. A prompt that opens this way is a preference or a lesson the
/// pack does not hold yet, and the moment to write it is now, before the
/// work that follows.
pub const CORRECTION_CUES: &[&str] = &[
    "do you not remember",
    "don't you remember",
    "dont you remember",
    "you should have",
    "why did you not",
    "why didn't you",
    "why havent you",
    "why haven't you",
    "you forgot",
    "i told you",
    "i've told you",
    "as i said",
    "again you",
    "still not",
    "not even able",
    "you never",
    "you keep",
];

/// On a prompt that reads as a correction, the one line that turns it
/// into memory: the agent writes the preference or lesson with `ljos
/// prefer` or `ljos remember` before it goes on. Once a session for the
/// same cue, so a run of corrections does not repeat it.
fn correction_nudge(call: &HookCall) -> Option<String> {
    if call.event != "UserPromptSubmit" {
        return None;
    }
    let lower = call.cue.to_lowercase();
    let hit = CORRECTION_CUES.iter().find(|c| lower.contains(*c))?;
    let key = format!("correction:{hit}");
    if seen_ids(call.session.as_deref()).contains(&key) {
        return None;
    }
    mark_seen(call.session.as_deref(), &[key]);
    Some(
        "This prompt reads as a correction. Before the work: write what it corrects as one \
         `ljos prefer \"...\"` (a standing choice) or `ljos remember \"...\"` (a lesson), \
         so the pack holds it and the hook can raise it next time."
            .to_string(),
    )
}

/// Phrases that put a choice to the agent. A choice with more than one
/// defensible answer is a ballot, and a ballot needs an issue to sit on.
pub const DECISION_CUES: &[&str] = &[
    "should we",
    "should i ",
    "or should",
    "which is better",
    "which one",
    "which approach",
    "which option",
    "pros and cons",
    "trade-off",
    "tradeoff",
    " versus ",
    " vs ",
    " vs. ",
    "what do you recommend",
    "do you think we",
    "option 1",
    "option 2",
    "option a",
    "option b",
];

/// On a prompt that puts a choice, the lines that take it to a panel
/// instead of one agent's opinion. Once a session, since one decision
/// is usually argued over several prompts.
fn decision_nudge(call: &HookCall) -> Option<String> {
    if call.event != "UserPromptSubmit" {
        return None;
    }
    let lower = format!(" {} ", call.cue.to_lowercase());
    DECISION_CUES.iter().find(|c| lower.contains(*c))?;
    let key = "decision-nudge".to_string();
    if seen_ids(call.session.as_deref()).contains(&key) {
        return None;
    }
    mark_seen(call.session.as_deref(), &[key]);
    Some(
        "This prompt puts a choice. Before choosing: put it on an issue whose body has an \
         `Options: A, B` line, then `ljos sitting ISSUE` writes one brief per persona the \
         title names; start one subagent per brief, each casting `ljos vote ISSUE --for \
         OPTION --as NAME`, and settle with `ljos consensus ISSUE`."
            .to_string(),
    )
}

/// On a prompt, once per session: how many claims are due for review. The
/// review loop runs only when somebody grades, and nobody grades what they
/// were not told about.
fn due_nudge(call: &HookCall) -> String {
    if call.event != "UserPromptSubmit" {
        return String::new();
    }
    let key = "due-nudge".to_string();
    if seen_ids(call.session.as_deref()).contains(&key) {
        return String::new();
    }
    let Ok(client) = pack() else {
        return String::new();
    };
    let Ok(atoms) = atoms_lean(&client, &client.workspace()) else {
        return String::new();
    };
    let due = due_of(&atoms, &now_utc()).len();
    // Counted once a session either way; a quiet seat is not re-counted on
    // every prompt. Do not call consolidate here: that walk is a sitting,
    // not a hook, and it is what made PreToolUse time out at 20s.
    mark_seen(call.session.as_deref(), &[key]);
    if due == 0 {
        return String::new();
    }
    format!(
        "{due} claim{} due for review in this seat: `ljos due`, read each, then `ljos graded ID` (or `--lapsed`).",
        if due == 1 { " is" } else { "s are" }
    )
}

/// The hook's answer in the runner's JSON: `additionalContext` under the
/// event that fired. Empty context is no output, which the runner reads as
/// no opinion.
#[must_use]
pub fn hook_output(call: &HookCall, context: &str) -> String {
    hook_output_ruled(call, context, None)
}

/// [`hook_output`] carrying a rule's verdict on a tool call: `deny` or
/// `ask` as the runner's permission decision, with the rule's reason. On a
/// prompt or an argv line the verdict is a line of text.
#[must_use]
pub fn hook_output_ruled(call: &HookCall, context: &str, verdict: Option<&Rule>) -> String {
    if context.is_empty() && verdict.is_none() {
        return String::new();
    }
    if call.event == "argv" {
        let mut out = String::new();
        if let Some(r) = verdict {
            out.push_str(&format!(
                "{}: {} (rule `{}`)\n",
                r.verdict, r.reason, r.pattern
            ));
        }
        if !context.is_empty() {
            out.push_str(context);
            out.push('\n');
        }
        return out;
    }
    if call.shape == HookShape::Context && verdict.is_none() {
        return if context.is_empty() {
            String::new()
        } else {
            serde_json::json!({ "context": context }).to_string() + "\n"
        };
    }
    let mut specific = serde_json::json!({ "hookEventName": call.event });
    if !context.is_empty() {
        specific["additionalContext"] = Value::String(context.to_string());
    }
    let mut top = serde_json::Map::new();
    if let Some(r) = verdict {
        if call.event == "PreToolUse" {
            // A runner that cannot ask runs the tool on an `ask`; the
            // seat stops it and tells the agent to ask the person.
            let (decision, reason) = if r.verdict == "ask" && !call.shape.asks() {
                (
                    "deny",
                    format!(
                        "ask the person before running this: {} (seat rule `{}`)",
                        r.reason, r.pattern
                    ),
                )
            } else {
                (
                    r.verdict.as_str(),
                    format!("{} (seat rule `{}`)", r.reason, r.pattern),
                )
            };
            if call.shape == HookShape::Context {
                // `block` is the one verb there; context rides along.
                let mut out = serde_json::json!({ "decision": "block", "reason": reason });
                if !context.is_empty() {
                    out["context"] = Value::String(context.to_string());
                }
                return out.to_string() + "\n";
            }
            specific["permissionDecision"] = Value::String(decision.to_string());
            specific["permissionDecisionReason"] = Value::String(reason.clone());
            if call.shape == HookShape::CamelCase {
                top.insert("decision".into(), Value::String(decision.to_string()));
                top.insert("reason".into(), Value::String(reason));
            }
        }
    }
    top.insert("hookSpecificOutput".into(), specific);
    Value::Object(top).to_string() + "\n"
}

pub fn format_steps(steps: &[Step]) -> String {
    steps
        .iter()
        .map(|s| {
            format!(
                "{}\t{}\t{}\n",
                if s.ok { "ok" } else { "no" },
                s.what,
                s.detail
            )
        })
        .collect()
}

/// The runner rows for `doctor`, one pair per runner the file names.
fn harness_rows() -> Vec<Habitat> {
    let path = harnesses_path();
    let all = match harnesses_from(&path) {
        Ok(all) => all,
        Err(e) => {
            return vec![Habitat {
                name: "runners",
                state: format!("{e:#}"),
                ok: false,
            }]
        }
    };
    if all.harness.is_empty() {
        return vec![Habitat {
            name: "runners",
            state: format!(
                "none named in {}; `ljos onboard --example` prints the shape",
                path.display()
            ),
            ok: false,
        }];
    }
    let server = server_path().unwrap_or_else(|_| PathBuf::from("ljos-mcp"));
    let mut rows = Vec::new();
    for h in &all.harness {
        let registered = is_registered(h, &server) == Some(true);
        let probed = (registered && !h.probe.is_empty()).then(|| probe_lists_ljos(&h.probe));
        rows.push(Habitat {
            name: "runner mcp",
            state: match (registered, &probed) {
                (false, _) => format!(
                    "{}: not registered; ljos onboard --harness {}",
                    h.name, h.name
                ),
                (true, Some(Err(why))) => format!(
                    "{}: registered, but `{}` does not list ljos_sitting: {why}",
                    h.name,
                    h.probe.join(" ")
                ),
                (true, Some(Ok(()))) => format!("{}: ljos registered and loads", h.name),
                (true, None) => format!("{}: ljos registered", h.name),
            },
            ok: registered && !matches!(probed, Some(Err(_))),
        });
        let skill = h
            .skills
            .as_deref()
            .map(|d| expand(d).join("ljos").join("SKILL.md"));
        let current = skill
            .as_ref()
            .is_some_and(|p| std::fs::read_to_string(p).is_ok_and(|t| t == skill_text()));
        if let Some(file) = &h.hooks {
            let path = expand(file);
            let installed = hook_installed(&path, &hook_events_of(h));
            rows.push(Habitat {
                name: "runner hook",
                state: if installed {
                    format!("{}: memory hook on {}", h.name, path.display())
                } else {
                    format!(
                        "{}: no memory hook; ljos onboard --harness {}",
                        h.name, h.name
                    )
                },
                ok: installed,
            });
        }
        if let Some(dest) = &h.plugin {
            let path = expand(dest);
            let want = ljos_path().ok().and_then(|l| plugin_text(h, &l));
            let current = want
                .as_ref()
                .is_some_and(|w| std::fs::read_to_string(&path).is_ok_and(|t| &t == w));
            rows.push(Habitat {
                name: "runner hook",
                state: if current {
                    format!("{}: plugin {}", h.name, path.display())
                } else if path.is_file() {
                    format!(
                        "{}: plugin {} is stale; ljos onboard --harness {}",
                        h.name,
                        path.display(),
                        h.name
                    )
                } else {
                    format!("{}: no plugin; ljos onboard --harness {}", h.name, h.name)
                },
                ok: current,
            });
        }
        rows.push(Habitat {
            name: "runner skill",
            state: match (&skill, current) {
                (Some(p), true) => format!("{}: {}", h.name, p.display()),
                (Some(p), false) if p.is_file() => {
                    format!(
                        "{}: {} is stale; ljos onboard --harness {}",
                        h.name,
                        p.display(),
                        h.name
                    )
                }
                (Some(_), false) => {
                    format!("{}: absent; ljos onboard --harness {}", h.name, h.name)
                }
                (None, _) => format!("{}: no skills directory named", h.name),
            },
            ok: current,
        });
    }
    rows
}

/// Run a runner's probe with a thirty-second limit; it passes when it
/// exits 0 and its output names `ljos_sitting`.
fn probe_lists_ljos(argv: &[String]) -> std::result::Result<(), String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let (bin, args) = argv.split_first().ok_or("empty probe")?;
    let mut child = Command::new(expand(bin))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{bin}: {e}"))?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > std::time::Duration::from_secs(30) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("no answer in 30 s".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut out = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut out);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut out);
    }
    if !status.success() {
        return Err(format!("exit {}", status.code().unwrap_or(-1)));
    }
    if out.contains("ljos_sitting") {
        Ok(())
    } else {
        Err("its output names no ljos tool".into())
    }
}

/// Have a pack writer up before anything else is wired: a runner onboarded
/// to a seat with no writer would meet every memory verb failing. `packset
/// ensure` starts one when none answers and is idempotent when one does.
fn pack_step(dry: bool) -> Step {
    let what = "pack".to_string();
    if let Ok(client) = pack() {
        if client.health().is_ok() {
            return Step {
                what,
                detail: format!("writer up at {}", client.base()),
                ok: true,
            };
        }
    } else {
        return Step {
            what,
            detail: "PACKSET_URL=off; no pack on purpose".into(),
            ok: true,
        };
    }
    if !on_path("packset") {
        return Step {
            what,
            detail: "no writer answers and packset is not on PATH".into(),
            ok: false,
        };
    }
    if dry {
        return Step {
            what,
            detail: "would run packset ensure".into(),
            ok: true,
        };
    }
    match run_captured("packset", &["ensure"]) {
        Ok(said) => Step {
            what,
            detail: format!(
                "started a writer: {}",
                said.stdout.lines().next().unwrap_or("").trim()
            ),
            ok: true,
        },
        Err(e) => Step {
            what,
            detail: e.to_string().lines().next().unwrap_or("").to_string(),
            ok: false,
        },
    }
}

/// Make the seat's host key at `~/.config/deedar/host.key` when there is
/// none, so handovers go out signed from the first one. An existing key, or
/// one named by `DEEDAR_HOST_SIGNING_KEY`, is left alone.
fn host_key_step(dry: bool) -> Step {
    if let Some(path) = host_key_path() {
        return Step {
            what: "host key".into(),
            detail: format!("{} exists", path.display()),
            ok: true,
        };
    }
    if std::env::var_os("DEEDAR_HOST_SIGNING_KEY").is_some_and(|r| r == "off") {
        return Step {
            what: "host key".into(),
            detail: "DEEDAR_HOST_SIGNING_KEY=off; handovers go out unsigned on purpose".into(),
            ok: true,
        };
    }
    let Some(path) = default_host_key_path() else {
        return Step {
            what: "host key".into(),
            detail: "no home directory to keep a key in".into(),
            ok: false,
        };
    };
    if dry {
        return Step {
            what: "host key".into(),
            detail: format!("would write a 32-byte seed to {}", path.display()),
            ok: true,
        };
    }
    let made = (|| -> std::io::Result<()> {
        use std::io::Read;
        let mut seed = [0u8; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut seed)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, seed)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    })();
    match made {
        Ok(()) => Step {
            what: "host key".into(),
            detail: format!("wrote a 32-byte seed to {}", path.display()),
            ok: true,
        },
        Err(e) => Step {
            what: "host key".into(),
            detail: format!("{}: {e}", path.display()),
            ok: false,
        },
    }
}

/// `$XDG_CONFIG_HOME/deedar/host.key`, whether or not it exists.
fn default_host_key_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().ok().map(|h| h.join(".config")))?;
    Some(config.join("deedar").join("host.key"))
}

/// The host key `deedar` will sign with: `DEEDAR_HOST_SIGNING_KEY`, else
/// `~/.config/deedar/host.key` when it exists. `off` is no key on purpose.
fn host_key_path() -> Option<PathBuf> {
    if let Some(raw) = std::env::var_os("DEEDAR_HOST_SIGNING_KEY").filter(|r| !r.is_empty()) {
        return (raw != "off").then(|| PathBuf::from(raw));
    }
    let path = default_host_key_path()?;
    path.is_file().then_some(path)
}

/// `raw` with a leading `~` or `~/` put against `home`; `None` when there is
/// nothing to expand.
pub fn expand_leading_tilde(raw: &str, home: &str) -> Option<String> {
    let home = home.trim_end_matches('/');
    if raw == "~" {
        return Some(home.to_string());
    }
    raw.strip_prefix("~/").map(|rest| format!("{home}/{rest}"))
}

/// Expand a leading `~` in `ISSUE_ROOT` and `VISSUE_ROOT` once, at start.
/// environment.d and MCP `env` blocks pass `~/...` through unexpanded; a
/// tracker crate that predates the fix then resolves it against the working
/// directory, and every child `vissue` inherits the same relative root.
pub fn normalize_tracker_env() {
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
        return;
    };
    let home = home.to_string_lossy().to_string();
    for var in ["ISSUE_ROOT", "VISSUE_ROOT"] {
        if let Ok(raw) = std::env::var(var) {
            if let Some(expanded) = expand_leading_tilde(&raw, &home) {
                std::env::set_var(var, expanded);
            }
        }
    }
}

/// Printed on stderr. `ljos-policyd` is the TCB when it exists.
pub const POLICY_TCB: &str =
    "argv law. ljos-policyd is the TCB when present. Reloading a pack is not a check.";

/// The workspace the seat's memory lives in when nothing names one. The
/// pack's command line keys a workspace to the repository it stands in;
/// a seat is one memory across every repository it works in, so the seat
/// pins one. `PACKSET_WORKSPACE` overrides it.
pub const SEAT_WORKSPACE: &str = "seat";

/// The pack client. With nothing set it speaks to `127.0.0.1:8761` about
/// the `seat` workspace; `PACKSET_URL` points elsewhere, `PACKSET_WORKSPACE`
/// names another workspace, and `PACKSET_URL=off` is the one way to have no
/// pack.
/// Load `~/.config/ljos/env` (KEY=VALUE) when the process has not set
/// those keys. The shell and the MCP seat then share one pack.
fn load_seat_env() {
    let Ok(home) = home() else {
        return;
    };
    let path = home.join(".config/ljos/env");
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        if k.is_empty() || std::env::var_os(k).is_some() {
            continue;
        }
        std::env::set_var(k, v.trim());
    }
}

/// A transport failure, as distinct from a writer that answered and refused.
fn writer_unreachable(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<packset_client::Error>()
            .is_some_and(|inner| matches!(inner, packset_client::Error::Http(_)))
    })
}

/// Start the default writer when a memory verb could not connect.
/// `PACKSET_URL=off` is left alone. A URL pointed somewhere else is not
/// replaced with the default writer.
fn ensure_writer() -> Result<()> {
    if std::env::var("PACKSET_URL").ok().as_deref() == Some("off") {
        return Ok(());
    }
    if std::env::var("PACKSET_URL")
        .ok()
        .is_some_and(|url| !url.is_empty())
    {
        bail!(
            "the pack writer at PACKSET_URL is not answering. This seat is not pointed at the default writer, so it was not started"
        );
    }
    if !on_path("packset") {
        bail!("no pack writer is answering, and packset is not on PATH. cargo binstall packset");
    }
    run_captured("packset", &["ensure"]).context("packset ensure")?;
    Ok(())
}

fn with_writer<T>(op: impl Fn() -> Result<T>) -> Result<T> {
    match op() {
        Ok(value) => Ok(value),
        Err(err) if writer_unreachable(&err) => {
            ensure_writer()?;
            op()
        }
        Err(err) => Err(err),
    }
}

/// The pack's live atoms without their dense vectors. Every reader here
/// wants texts, kinds, review clocks, trust or rules; the vectors are nine
/// tenths of the listing, and parsing them grew one ljos-mcp from 10 to
/// 66 MB and kept it. A writer older than `embedding=omit` sends them
/// anyway, and the answer is the same.
///
/// # Errors
///
/// The pack not answering, or an answer that is not atoms.
pub fn atoms_lean(client: &PacksetClient, workspace: &str) -> Result<Vec<Value>> {
    let url = format!("{}/v1/atoms", client.base());
    let mut body: Value = ureq::get(&url)
        .query("workspace", workspace)
        .query("embedding", "omit")
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| anyhow::anyhow!("{url}: {e}"))?
        .into_json()?;
    let atoms = body
        .get_mut("atoms")
        .map(Value::take)
        .unwrap_or(Value::Array(Vec::new()));
    Ok(serde_json::from_value(atoms)?)
}

pub fn pack() -> Result<PacksetClient> {
    load_seat_env();
    let workspace = std::env::var("PACKSET_WORKSPACE")
        .ok()
        .filter(|w| !w.is_empty())
        .unwrap_or_else(|| SEAT_WORKSPACE.to_string());
    Ok(PacksetClient::from_env()
        .context("PACKSET_URL=off: this seat has no pack on purpose")?
        .with_workspace(workspace))
}

/// The pack's last write, RFC 3339, for a HUD watch. `None` when the
/// status has no stamp yet.
///
/// # Errors
///
/// The pack not answering.
pub fn pack_last_write_ts() -> Result<Option<String>> {
    let client = pack()?;
    let status = client
        .status(Some(&client.workspace()))
        .context("pack: GET /v1/status failed")?;
    Ok(status
        .get("last_write_ts")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string))
}

pub fn join(parts: &[String]) -> String {
    parts.join(" ")
}

/// Remember → lesson, Prefer → preference. Trust rows go through [`trust_atom`].
pub fn atom_kind(label: &str) -> Result<&'static str> {
    match label {
        "Remember" => Ok("lesson"),
        "Prefer" => Ok("preference"),
        other => bail!("unknown write kind {other}"),
    }
}

/// The entity every write carries: which seat wrote it. Many seats share
/// one pack, and a reader can then see whose lesson it is reading.
pub const SEAT_ENTITY: &str = "seat:";

/// Explicit claim body. The text is stored as given; never harvested. The
/// entities open with the seat that wrote it.
pub fn atom_body(kind: &str, text: &str, workspace: &str) -> Value {
    serde_json::json!({
        "schema": "inside.atom/v1",
        "kind": kind,
        "level": "explicit",
        "text": text,
        "workspace": workspace,
        "entities": [format!("{SEAT_ENTITY}{}", seat_name())],
    })
}

/// Add entities to a body without losing the seat's.
pub fn add_entities(atom: &mut Value, more: impl IntoIterator<Item = String>) {
    let list = atom["entities"]
        .as_array_mut()
        .map(std::mem::take)
        .unwrap_or_default();
    let mut list = list;
    for e in more {
        let v = Value::String(e);
        if !list.contains(&v) {
            list.push(v);
        }
    }
    atom["entities"] = Value::Array(list);
}

/// POST one explicit claim. Callers pass Remember/Prefer only.
pub fn post_claim(
    client: &PacksetClient,
    label: &str,
    text: &str,
    workspace: &str,
) -> Result<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("{label}: empty text is not a claim");
    }
    let kind = atom_kind(label)?;
    let atom = atom_body(kind, trimmed, workspace);
    with_writer(|| {
        client
            .post_atom(&atom)
            .with_context(|| format!("{label}: POST /v1/atoms failed"))
    })
}

pub fn packset_write(label: &str, text: &str) -> Result<Value> {
    packset_write_as(label, text, None)
}

/// [`packset_write`] for a lesson learned on an issue: it carries an
/// `issue:ID` entity naming where it was learned, and a `scope:NAME`
/// entity when one is given, so the claim travels with that scope's log
/// rather than the machine's default.
///
/// # Errors
///
/// An empty text, an unknown label, or the pack refusing the claim.
pub fn packset_write_scoped(
    label: &str,
    text: &str,
    issue: &str,
    scope: Option<&str>,
) -> Result<Value> {
    let client = pack()?;
    let workspace = client.workspace();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("{label}: empty text is not a claim");
    }
    let kind = atom_kind(label)?;
    let mut atom = atom_body(kind, trimmed, &workspace);
    let mut tags = vec![format!("issue:{}", issue.trim())];
    if let Some(scope) = scope.map(str::trim).filter(|s| !s.is_empty()) {
        tags.push(format!("scope:{scope}"));
    }
    add_entities(&mut atom, tags);
    with_writer(|| {
        client
            .post_atom(&atom)
            .with_context(|| format!("{label}: POST /v1/atoms failed"))
    })
}

/// The entity a persona's own claims carry, so a brief can find them.
#[must_use]
pub fn persona_entity(name: &str) -> String {
    format!("persona:{}", name.trim().to_lowercase())
}

/// The set a persona's own conclusions live in: `persona-<name>`, in the
/// pack's set alphabet. A set is its own tree for the duplicate and
/// replacement rules, so a persona's lesson never closes the seat's or
/// another persona's, and the seat still reads them all.
#[must_use]
pub fn persona_set(name: &str) -> String {
    let mut out = String::from("persona-");
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(32).collect()
}

/// [`packset_write`] as a persona: the claim carries the persona's entity,
/// so what a persona learned comes back to it first in its next brief and
/// stays in the seat's one pack. A persona accumulates its own lessons the
/// way a reviewer does; the seat still reads them all.
pub fn packset_write_as(label: &str, text: &str, persona: Option<&str>) -> Result<Value> {
    let client = pack()?;
    let workspace = client.workspace();
    let Some(name) = persona.map(str::trim).filter(|n| !n.is_empty()) else {
        return post_claim(&client, label, text, &workspace);
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("{label}: empty text is not a claim");
    }
    let kind = atom_kind(label)?;
    let mut atom = atom_body(kind, trimmed, &workspace);
    add_entities(&mut atom, [persona_entity(name)]);
    // Its own tree: the persona's conclusions replace and duplicate among
    // themselves, not against the seat's or another persona's.
    atom["set"] = Value::String(persona_set(name));
    with_writer(|| {
        client
            .post_atom(&atom)
            .with_context(|| format!("{label}: POST /v1/atoms failed"))
    })
}

/// Retire one atom from the workspace the cwd resolves to, optionally naming
/// the deed that withdrew it.
///
/// The daemon tombstones rather than erases: the atom stops being recalled and
/// the pack still records that it was held and withdrawn. That is the right
/// shape for standing knowledge, where "we no longer believe this" is itself
/// worth keeping.
///
/// `why` is a deed accession and the pack refuses free text in its place. It
/// runs the same join as a remembered claim's `entities`, in the same
/// direction: the pack cites the deed store, never the other way round. A
/// retraction the work justified is therefore checkable with `deedar evidence`
/// like any other citation, and one nothing justified simply carries no `why`.
///
/// # Errors
///
/// An unset `PACKSET_URL`, an id the workspace does not hold, a `why` that is
/// not an accession, or the request's.
pub fn packset_forget(id: &str, why: Option<&str>) -> Result<Value> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        bail!("forget: an atom id is required");
    }
    let why = why.map(str::trim).filter(|w| !w.is_empty());
    let client = pack()?;
    let workspace = client.workspace();
    client
        .delete_atom(&workspace, trimmed, why)
        .with_context(|| format!("forget: POST /v1/atoms/delete failed for {trimmed}"))
}

/// One row of the influence graph: `from` listens to `to` with `weight`.
/// `about` scopes the row to the domains it speaks to: a row with none
/// applies everywhere, a row with some applies when one of them meets the
/// issue at hand (its title, or the entities of the island it activates).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trust {
    pub from: String,
    pub to: String,
    pub weight: f64,
    pub about: Vec<String>,
}

/// A voter with a view of its own: a persona. `anchor` in `[0, 1]` is how
/// far it moves off its ballot in a settle; 0 never moves, 1 is a plain
/// DeGroot voter. `entities` are the domains it speaks to.
#[derive(Debug, Clone, PartialEq)]
pub struct Persona {
    pub name: String,
    pub anchor: f64,
    pub view: String,
    pub entities: Vec<String>,
}

/// The `persona` atom for the pack: kind `persona`, the view as text.
///
/// # Errors
///
/// An empty name, an anchor outside `[0, 1]`, or an empty view.
pub fn persona_atom(p: &Persona, workspace: &str) -> Result<Value> {
    let name = p.name.trim();
    if name.is_empty() {
        bail!("persona: a name is required");
    }
    if !(0.0..=1.0).contains(&p.anchor) {
        bail!("persona: anchor {} is not in [0, 1]", p.anchor);
    }
    let view = p.view.trim();
    if view.is_empty() {
        bail!("persona: say in a sentence or two how {name} reads the work");
    }
    let mut atom = atom_body("persona", view, workspace);
    atom["name"] = Value::String(name.into());
    atom["anchor"] = serde_json::json!(p.anchor);
    if !p.entities.is_empty() {
        add_entities(&mut atom, p.entities.iter().map(|e| e.to_lowercase()));
    }
    Ok(atom)
}

/// POST one persona. A persona of the same name already in the pack is
/// superseded, so a rewrite moves the roster without leaving the old view
/// live. Every persona is owed one unscoped inbound trust row; `--about`
/// on a later trust row only adds weight, it does not replace that floor.
pub fn write_persona(p: &Persona) -> Result<Value> {
    let client = pack()?;
    let workspace = client.workspace();
    let mut atom = persona_atom(p, &workspace)?;
    let previous: Vec<Value> = client
        .atoms_of_kind(&workspace, "persona")
        .unwrap_or_default()
        .into_iter()
        .filter(|a| a.get("name").and_then(Value::as_str) == Some(p.name.trim()))
        .filter_map(|a| {
            a.get("id")
                .and_then(Value::as_str)
                .map(|id| Value::String(id.to_string()))
        })
        .collect();
    if !previous.is_empty() {
        atom["supersedes"] = Value::Array(previous);
    }
    let posted = client
        .post_atom(&atom)
        .context("persona: POST /v1/atoms failed")?;
    ensure_unscoped_inbound(p)?;
    Ok(posted)
}

/// The unscoped inbound row a persona is owed: the seat weighs it at 1,
/// everywhere. None when the seat and the persona are the same name
/// (a row cannot weigh itself).
#[must_use]
pub fn inbound_floor(p: &Persona, seat: &str) -> Option<Trust> {
    let to = p.name.trim();
    let from = seat.trim();
    if to.is_empty() || from.is_empty() || from == to {
        return None;
    }
    Some(Trust {
        from: from.to_string(),
        to: to.to_string(),
        weight: 1.0,
        about: Vec::new(),
    })
}

/// Whether `name` already has the seat's unscoped inbound row in `rows`.
/// A third-party unscoped row does not seat this persona.
#[must_use]
pub fn has_unscoped_inbound(rows: &[Trust], name: &str, seat: &str) -> bool {
    let name = name.trim();
    let seat = seat.trim();
    rows.iter()
        .any(|r| r.from == seat && r.to == name && r.about.is_empty() && r.weight > 0.0)
}

fn ensure_unscoped_inbound(p: &Persona) -> Result<()> {
    let name = p.name.trim();
    let seat = seat_name();
    if has_unscoped_inbound(&trust_from_pack().unwrap_or_default(), name, &seat) {
        return Ok(());
    }
    let Some(row) = inbound_floor(p, &seat) else {
        return Ok(());
    };
    write_trust(&row, &[]).map(|_| ())
}

/// The live personas: the latest `persona` atom per name.
pub fn personas_of(atoms: &[Value]) -> Vec<Persona> {
    let mut latest: std::collections::BTreeMap<String, (String, Persona)> =
        std::collections::BTreeMap::new();
    for atom in atoms {
        if atom.get("kind").and_then(Value::as_str) != Some("persona") {
            continue;
        }
        let (Some(name), Some(anchor)) = (
            atom.get("name").and_then(Value::as_str),
            atom.get("anchor").and_then(Value::as_f64),
        ) else {
            continue;
        };
        let ts = atom
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let p = Persona {
            name: name.to_string(),
            anchor,
            view: atom
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            entities: domains_of(atom.get("entities")),
        };
        match latest.get(name) {
            Some((seen, _)) if *seen > ts => {}
            _ => {
                latest.insert(name.to_string(), (ts, p));
            }
        }
    }
    latest.into_values().map(|(_, p)| p).collect()
}

/// The personas in the seat's pack.
pub fn personas_from_pack() -> Result<Vec<Persona>> {
    let client = pack()?;
    // One kind, not the pack: a roster of a dozen does not carry every
    // lesson's embedding across the socket.
    let atoms = client
        .atoms_of_kind(&client.workspace(), "persona")
        .context("persona: GET /v1/atoms?kind=persona failed")?;
    Ok(personas_of(&atoms))
}

/// A recipe a sitting copies before personas enter. `models` are optional
/// spawn hints; every panel still ends in `ljos vote --as` then
/// `ljos consensus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playbook {
    pub name: String,
    pub body: String,
    pub models: Vec<String>,
}

/// The closed set. Write, list, bind, and copy refuse any other name.
pub const PLAYBOOK_NAMES: &[&str] = &["sit", "arena", "land", "company-panel", "overnight"];

/// The five shipped recipes. Kind `playbook`, weighed not recalled.
pub const SHIPPED_PLAYBOOK_NAMES: &[&str] = PLAYBOOK_NAMES;

/// Five named principles, invocable mid-sitting, mapped onto existing law.
pub const PRINCIPLES: &str = "\
== principles
split-fence: independent implementers, independent trees. A's fence stays: no second plugin, no poteto-mode, no Benny, musl CLI iced-free, `ljos vote --as` and DeGroot stay.
prove-on-real-surface: measure on the host the users run. A cheaper substitute is not the result.
open-sibling-first: a second implementer opens a sibling leftover, not a rewrite of the first tree.
arena-then-compose: designs write scratch; the host writes a rubric on a compose child; personas vote the compose `--as`.
one-step-delegate: a subagent is one playbook step. No resume across phases. A new task is a new sitting.
";

/// The scoring sheet a compose is voted on. Personas vote the compose, not
/// accept-at-most-one on the designs.
pub const RUBRIC: &str = "\
== rubric
1. Ledger intact. `ljos vote --as` and DeGroot stay. No schema_yes, no BARMA, no host for-loop of accepts.
2. Playbook before panel. Sitting names one recipe and copies it before personas enter.
3. Rubric in brief. `ljos brief` carries the playbook step, these principles, and this sheet.
4. One-step delegate. Subagent = one playbook step. No resume across phases.
5. Unscoped inbound trust. Every panel persona has one unscoped inbound row; `--about` only adds weight.
6. No second plugin. Do not copy 47 skills, poteto-mode, Benny, or Cursor model files.
7. Small surface. Prefer pack atoms and brief fields over a new crate. Musl CLI stays iced-free.
8. Named principles. Five families, invocable mid-sitting, mapped onto existing law (split-fence, prove-on-real-surface, open-sibling-first, arena-then-compose, one-step-delegate).
";

const SIT_BODY: &str = "\
A sitting on one issue. Name this recipe at open (`ljos sitting ISSUE --playbook sit` or `ljos playbook ISSUE sit`). The sitting prints this body before recall and holds the name until finish or release.

1. Open with `ljos sitting ISSUE --playbook sit`. Read doctor, cards, due, island, this recipe, recall, timeline, claim.
2. Grade due claims (`ljos graded ID`).
3. Do the work on this claim only. Artefacts are deeds, then `ljos deed ISSUE --add ACCESSION`. Lessons are `ljos remember` in two sentences.
4. One playbook step is the whole sitting. A subagent takes this recipe and this issue; it does not resume a later phase.
5. Close with `ljos finish ISSUE --lesson \"...\"`. Completing the node does not close the ticket. `ljos finish ISSUE --close` does, when the work is accepted.
";

const ARENA_BODY: &str = "\
Designs compete; the host writes a rubric; personas vote a compose, not the designs.

1. Bind this recipe: `ljos sitting ISSUE --playbook arena` or `ljos playbook ISSUE arena`.
2. Each design writes scratch (summary and body). Do not vote the design children as accept-at-most-one.
3. The host writes a compose child and a rubric with named axes. Personas vote the compose `--as`.
4. Spawn hints are optional model-family names on this atom. Each subagent still ends with `ljos vote ISSUE --for accept|reject --as NAME`. No graft. PASS on an axis is not GREEN.
5. `ljos consensus ISSUE` settles under trust rows and DeGroot. `ljos vote --as` stays.
";

const LAND_BODY: &str = "\
Land a chosen design on the real surface.

1. Bind `land`. Sitting copies this body before recall.
2. Prove on the real surface: the host the users run, the crate they install. A cheaper substitute is not the result.
3. Keep A's fence: no 47 skills, no poteto-mode, no Benny, musl iced-free, `ljos vote --as` and DeGroot stay.
4. One step per subagent. Open a sibling first when a second implementer is in flight.
5. Close with finish. Do not ship a count as consensus.
";

const COMPANY_PANEL_BODY: &str = "\
A panel of personas on one bound recipe.

1. Bind `company-panel` before any persona enters. `ljos panel` refuses if none is bound.
2. Every persona has one unscoped inbound trust row; `--about` only adds weight.
3. `ljos brief NAME ISSUE` reprints this recipe in full, the five named principles, and the arena rubric.
4. One subagent per persona, optional model-family spawn hints. Each casts `ljos vote ISSUE --for OPTION --as NAME`. Then `ljos consensus ISSUE`.
5. Do not resume across phases. A new task is a new sitting.
";

const OVERNIGHT_BODY: &str = "\
Drive work while unattended, still one sitting.

1. Bind `overnight`. Name a checkable finish condition on the issue.
2. One playbook step per subagent. No session-pickup, no resume across phases.
3. Isolated worktree. Prove on the real surface before claiming done.
4. Decision log is tracker notes and deeds, not a second ledger.
5. `ljos finish` when the condition holds; otherwise `ljos release` and a new sitting.
";

/// The five shipped playbooks, bodies in full, model roles as spawn hints.
#[must_use]
pub fn shipped_playbooks() -> Vec<Playbook> {
    vec![
        Playbook {
            name: "sit".into(),
            body: SIT_BODY.trim().into(),
            models: Vec::new(),
        },
        Playbook {
            name: "arena".into(),
            body: ARENA_BODY.trim().into(),
            models: vec!["judgment".into(), "instruction".into(), "fast".into()],
        },
        Playbook {
            name: "land".into(),
            body: LAND_BODY.trim().into(),
            models: Vec::new(),
        },
        Playbook {
            name: "company-panel".into(),
            body: COMPANY_PANEL_BODY.trim().into(),
            models: vec!["judgment".into(), "instruction".into()],
        },
        Playbook {
            name: "overnight".into(),
            body: OVERNIGHT_BODY.trim().into(),
            models: Vec::new(),
        },
    ]
}

/// Refuse a name that is not in [`PLAYBOOK_NAMES`].
///
/// # Errors
///
/// An unknown name.
pub fn parse_playbook_name(name: &str) -> Result<&'static str> {
    let n = name.trim();
    if n.is_empty() {
        bail!(
            "playbook: a name is required ({})",
            PLAYBOOK_NAMES.join(", ")
        );
    }
    PLAYBOOK_NAMES
        .iter()
        .copied()
        .find(|k| *k == n)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "playbook: unknown name {n:?}; the closed set is {}",
                PLAYBOOK_NAMES.join(", ")
            )
        })
}

/// The `playbook` atom: kind `playbook`, the recipe as text.
///
/// # Errors
///
/// An unknown name or an empty body.
pub fn playbook_atom(p: &Playbook, workspace: &str) -> Result<Value> {
    let name = parse_playbook_name(&p.name)?;
    let body = p.body.trim();
    if body.is_empty() {
        bail!("playbook: {name} needs a recipe body");
    }
    let mut atom = atom_body("playbook", body, workspace);
    atom["name"] = Value::String(name.into());
    if !p.models.is_empty() {
        atom["models"] = Value::Array(
            p.models
                .iter()
                .map(|m| m.trim())
                .filter(|m| !m.is_empty())
                .map(|m| Value::String(m.to_string()))
                .collect(),
        );
    }
    Ok(atom)
}

/// POST one playbook. A playbook of the same name already in the pack is
/// superseded, so a rewrite moves the recipe without leaving the old body
/// live.
pub fn write_playbook(p: &Playbook) -> Result<Value> {
    let client = pack()?;
    let workspace = client.workspace();
    let mut atom = playbook_atom(p, &workspace)?;
    let previous: Vec<Value> = client
        .atoms_of_kind(&workspace, "playbook")
        .unwrap_or_default()
        .into_iter()
        .filter(|a| a.get("name").and_then(Value::as_str) == Some(p.name.trim()))
        .filter_map(|a| {
            a.get("id")
                .and_then(Value::as_str)
                .map(|id| Value::String(id.to_string()))
        })
        .collect();
    if !previous.is_empty() {
        atom["supersedes"] = Value::Array(previous);
    }
    client
        .post_atom(&atom)
        .context("playbook: POST /v1/atoms failed")
}

/// The live playbooks: the latest `playbook` atom per name.
pub fn playbooks_of(atoms: &[Value]) -> Vec<Playbook> {
    let mut latest: std::collections::BTreeMap<String, (String, Playbook)> =
        std::collections::BTreeMap::new();
    for atom in atoms {
        if atom.get("kind").and_then(Value::as_str) != Some("playbook") {
            continue;
        }
        let Some(name) = atom.get("name").and_then(Value::as_str) else {
            continue;
        };
        if parse_playbook_name(name).is_err() {
            continue;
        }
        let ts = atom
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let p = Playbook {
            name: name.to_string(),
            body: atom
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            models: atom
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        };
        match latest.get(name) {
            Some((seen, _)) if *seen > ts => {}
            _ => {
                latest.insert(name.to_string(), (ts, p));
            }
        }
    }
    latest.into_values().map(|(_, p)| p).collect()
}

fn ensure_shipped_playbooks() {
    let have = pack()
        .ok()
        .and_then(|c| c.atoms_of_kind(&c.workspace(), "playbook").ok())
        .map(|atoms| playbooks_of(&atoms))
        .unwrap_or_default();
    for p in shipped_playbooks() {
        if have.iter().any(|h| h.name == p.name) {
            continue;
        }
        let _ = write_playbook(&p);
    }
}

/// The roster: pack atoms, with the five shipped filled in when missing.
pub fn playbooks_from_pack() -> Result<Vec<Playbook>> {
    ensure_shipped_playbooks();
    let client = pack()?;
    let atoms = client
        .atoms_of_kind(&client.workspace(), "playbook")
        .context("playbook: GET /v1/atoms?kind=playbook failed")?;
    let mut got = playbooks_of(&atoms);
    for p in shipped_playbooks() {
        if !got.iter().any(|g| g.name == p.name) {
            got.push(p);
        }
    }
    got.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(got)
}

/// Pack latest for `name`, else the shipped seed. Unknown names are refused
/// even when the pack holds them.
///
/// # Errors
///
/// An unknown name; the error lists the closed set.
pub fn playbook_among(name: &str, pack: &[Playbook]) -> Result<Playbook> {
    let name = parse_playbook_name(name)?;
    if let Some(p) = pack.iter().find(|p| p.name == name) {
        return Ok(p.clone());
    }
    shipped_playbooks()
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "playbook: unknown name {name:?}; the closed set is {}",
                PLAYBOOK_NAMES.join(", ")
            )
        })
}

/// Look up one playbook by name: pack latest first, shipped seed only when
/// the pack has no live atom of that name.
///
/// # Errors
///
/// Unknown name; the error lists the closed set.
pub fn playbook_named(name: &str) -> Result<Playbook> {
    let pack = playbooks_from_pack().unwrap_or_default();
    playbook_among(name, &pack)
}

/// The recipe body a sitting copies, including optional spawn hints.
#[must_use]
pub fn format_playbook_copy(p: &Playbook) -> String {
    let mut out = format!("{}\n{}\n", p.name, p.body.trim());
    if !p.models.is_empty() {
        out.push_str("spawn hints (optional): ");
        out.push_str(&p.models.join(", "));
        out.push_str("; each subagent still ends with `ljos vote --as` then `ljos consensus`.\n");
    }
    out
}

/// The roster, one playbook per line: name, spawn hints, first sentence.
#[must_use]
pub fn format_playbooks(playbooks: &[Playbook]) -> String {
    if playbooks.is_empty() {
        return "no playbooks; the shipped recipes are sit, arena, land, company-panel, overnight\n"
            .to_string();
    }
    let width = playbooks.iter().map(|p| p.name.len()).max().unwrap_or(0);
    playbooks
        .iter()
        .map(|p| {
            let first = p
                .body
                .split_once('.')
                .map(|(s, _)| s.trim())
                .unwrap_or(p.body.trim());
            format!(
                "{:width$}  {}  {}\n",
                p.name,
                if p.models.is_empty() {
                    "no spawn hints".to_string()
                } else {
                    format!("hints {}", p.models.join(", "))
                },
                first
            )
        })
        .collect()
}

/// A tracker logbook note that binds a playbook name to an issue. Latest
/// such note wins; empty rest is the sitting-scoped drop finish/release write.
pub const PLAYBOOK_NOTE_PREFIX: &str = "playbook:";

fn playbook_key(issue: &str) -> String {
    issue
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn playbook_bind_path(issue: &str) -> PathBuf {
    runtime_dir().join(format!("playbook-{}", playbook_key(issue)))
}

fn cached_playbook(issue: &str) -> Option<String> {
    let text = std::fs::read_to_string(playbook_bind_path(issue)).ok()?;
    let name = text.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn write_playbook_cache(issue: &str, name: &str) -> Result<()> {
    let path = playbook_bind_path(issue);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, format!("{name}\n"))
        .with_context(|| format!("playbook: could not bind {name} on {issue}"))
}

/// The playbook name bound on an issue JSON: the latest logbook note that
/// opens with [`PLAYBOOK_NOTE_PREFIX`]. Empty rest means this sitting dropped
/// it; do not walk back to an earlier bind.
#[must_use]
pub fn playbook_name_from_issue(v: &Value) -> Option<String> {
    let mut dated: Vec<(String, Option<String>)> = Vec::new();
    for e in v["logbook"].as_array().into_iter().flatten() {
        let Some(note) = e["note"].as_str() else {
            continue;
        };
        let Some(rest) = note.trim().strip_prefix(PLAYBOOK_NOTE_PREFIX) else {
            continue;
        };
        let name = rest.trim();
        let live = if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        };
        let ts = e["timestamp"].as_str().unwrap_or("").to_string();
        dated.push((ts, live));
    }
    if dated.iter().any(|(ts, _)| !ts.is_empty()) {
        dated
            .into_iter()
            .max_by_key(|(ts, _)| ts.clone())
            .and_then(|(_, n)| n)
    } else {
        dated.into_iter().next().and_then(|(_, n)| n)
    }
}

/// The playbook name bound on a tracker issue, if any.
///
/// # Errors
///
/// The tracker not answering.
pub fn playbook_named_on(issue: &str) -> Result<Option<String>> {
    let said = run_captured("vissue", &["show", issue, "--json"])?;
    let v: Value = serde_json::from_str(&said.stdout).context("vissue show --json")?;
    Ok(playbook_name_from_issue(&v))
}

/// The playbook name this sitting holds, if one was bound. Tracker note is
/// the bind that survives the process; the runtime cache is only when the
/// tracker does not answer.
#[must_use]
pub fn bound_playbook(issue: &str) -> Option<String> {
    match playbook_named_on(issue) {
        Ok(name) => name,
        Err(_) => cached_playbook(issue),
    }
}

/// Drop the sticky name. Finish and release call this; a new task is a
/// new sitting. Writes an empty `playbook:` note so the next sitting does
/// not reprint the previous recipe, and unlinks the runtime cache.
pub fn drop_playbook(issue: &str) {
    if bound_playbook(issue).is_some() {
        let _ = run_captured("vissue", &["note", issue, PLAYBOOK_NOTE_PREFIX]);
    }
    let _ = std::fs::remove_file(playbook_bind_path(issue));
}

/// Hold `name` on `issue` until finish or release. A different name while
/// one is held is refused: mid-sitting turns re-read the same note.
///
/// # Errors
///
/// Empty issue or name, or a different recipe already bound.
pub fn bind_playbook(issue: &str, name: &str) -> Result<()> {
    let issue = issue.trim();
    let name = name.trim();
    if issue.is_empty() {
        bail!("playbook: an issue is required");
    }
    if name.is_empty() {
        bail!("playbook: a name is required");
    }
    let name = parse_playbook_name(name)?;
    if let Some(have) = bound_playbook(issue) {
        if have != name {
            bail!(
                "playbook: {issue} is bound to {have} until finish or release; \
                 a new task is a new sitting"
            );
        }
        let _ = write_playbook_cache(issue, name);
        return Ok(());
    }
    let note = format!("{PLAYBOOK_NOTE_PREFIX} {name}");
    match run_captured("vissue", &["note", issue, &note]) {
        Ok(_) => {
            let _ = write_playbook_cache(issue, name);
            Ok(())
        }
        Err(_) => write_playbook_cache(issue, name),
    }
}

/// Bind `name` to `issue` and return the full recipe body. This is the
/// copy into the working set; sitting prints it before recall.
pub fn copy_playbook(issue: &str, name: &str) -> Result<String> {
    let p = playbook_named(name)?;
    bind_playbook(issue, &p.name)?;
    Ok(format_playbook_copy(&p))
}

/// A closed-set name the issue title names, else `sit`. Longer names win
/// (`company-panel` before a stray `sit` token); `sitting` is not `sit`.
#[must_use]
pub fn playbook_from_title(title: &str) -> &'static str {
    let tokens: Vec<String> = title
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let mut names: Vec<&'static str> = PLAYBOOK_NAMES.to_vec();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for name in names {
        if tokens.iter().any(|t| t == name) {
            return name;
        }
    }
    "sit"
}

/// Which playbook a sitting copies: an explicit name, else the name already
/// bound on the issue (sticky until finish/release), else a closed-set
/// token in the title, else `sit`.
///
/// # Errors
///
/// An unknown explicit name.
pub fn resolve_sitting_playbook(issue: &str, title: &str, asked: Option<&str>) -> Result<String> {
    if let Some(name) = asked.map(str::trim).filter(|n| !n.is_empty()) {
        return Ok(playbook_named(name)?.name);
    }
    if let Some(name) = bound_playbook(issue) {
        return Ok(name);
    }
    Ok(playbook_from_title(title).to_string())
}

/// The `== playbook` section of a sitting: bind when a name is given,
/// else reprint the sticky body, else say none is bound.
pub fn playbook_opening(issue: &str, name: Option<&str>) -> Result<String> {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => copy_playbook(issue, n),
        None => match bound_playbook(issue) {
            Some(have) => {
                let p = playbook_named(&have)?;
                Ok(format_playbook_copy(&p))
            }
            None => Ok("none bound; `ljos sitting ISSUE --playbook NAME` or \
                 `ljos playbook ISSUE NAME` names one. A panel is refused until then.\n"
                .to_string()),
        },
    }
}

/// The three blocks a brief carries: playbook step (full body), named
/// principles, arena rubric.
#[must_use]
pub fn brief_playbook_blocks(issue: &str) -> String {
    let copy = match bound_playbook(issue) {
        Some(name) => playbook_named(&name)
            .map(|p| format_playbook_copy(&p))
            .unwrap_or_else(|e| format!("{e}\n")),
        None => {
            "none bound; `ljos playbook ISSUE NAME` names one before personas enter.\n".to_string()
        }
    };
    format!("== playbook\n{copy}\n{PRINCIPLES}\n{RUBRIC}")
}

/// The brief a subagent playing a persona starts from: the persona's view
/// and domains, what the seat knows on those domains (preferences first),
/// and the issue's working set. One text, so a panel member reads the
/// same seat the rest do and still reads it its own way.
///
/// # Errors
///
/// No such persona in the pack, or the tracker or pack not answering.
pub fn brief(name: &str, issue: &str) -> Result<String> {
    let personas = personas_from_pack()?;
    let Some(p) = personas.iter().find(|p| p.name == name) else {
        let names: Vec<&str> = personas.iter().map(|p| p.name.as_str()).collect();
        bail!(
            "brief: no persona {name:?} in the pack; the pack holds {}",
            if names.is_empty() {
                "none".to_string()
            } else {
                names.join(", ")
            }
        );
    };
    let mut out = format!(
        "You are {}. {}\nYou hold your ballot at anchor {:.2}{}.\n\n{}",
        p.name,
        p.view,
        p.anchor,
        if p.entities.is_empty() {
            String::new()
        } else {
            format!("; you speak to {}", p.entities.join(", "))
        },
        brief_playbook_blocks(issue)
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut lines = Vec::new();
    let now = now_utc();
    // What this persona remembered itself comes first: its own lessons,
    // written with `remember --as`, carry its entity.
    let client = pack()?;
    let own_tag = persona_entity(&p.name);
    // Its own set first; lessons written before sets carry the entity alone.
    let mut pool = client
        .atoms_in_set(&client.workspace(), &persona_set(&p.name))
        .unwrap_or_default();
    if let Ok(all) = client.atoms_of_kind(&client.workspace(), "lesson") {
        pool.extend(
            all.into_iter()
                .filter(|a| words_of(a.get("entities")).contains(&own_tag))
                .filter(|a| a.get("set").is_none()),
        );
    }
    {
        let atoms = pool;
        let mut own: Vec<&Value> = atoms.iter().filter(|a| reviewable(a)).collect();
        own.sort_by(|a, b| b["ts"].as_str().cmp(&a["ts"].as_str()));
        if !own.is_empty() {
            out.push_str("\nWhat you remembered yourself:\n");
            for a in own.iter().take(8) {
                if let Some(id) = a["id"].as_str() {
                    seen.insert(id.to_string());
                }
                out.push_str(&format!(
                    "- [{}{}] {}\n",
                    a["kind"].as_str().unwrap_or("claim"),
                    age_tag(a["ts"].as_str(), &now),
                    a["text"].as_str().unwrap_or("").trim()
                ));
            }
        }
    }
    let cues: Vec<String> = if p.entities.is_empty() {
        vec![issue_title(issue)?]
    } else {
        p.entities.clone()
    };
    for cue in &cues {
        let Ok(hits) = packset_search(cue) else {
            continue;
        };
        for h in hits.into_iter().take(5) {
            if UNREVIEWED_KINDS.contains(&h.kind.as_str()) {
                continue;
            }
            if let Some(id) = &h.id {
                if !seen.insert(id.clone()) {
                    continue;
                }
            }
            lines.push((h.kind == "preference", hit_line(&h, &now)));
        }
    }
    lines.sort_by_key(|row| std::cmp::Reverse(row.0));
    if !lines.is_empty() {
        out.push_str("\nWhat this seat knows on your domains:\n");
        for (_, l) in lines.iter().take(8) {
            out.push_str(l);
            out.push('\n');
        }
    }
    out.push_str("\nThe work:\n");
    out.push_str(&run_captured("vissue", &["recall", issue])?.stdout);
    out.push_str(&format!(
        "\nWalk the island as yourself before the ballot: `ljos island` on the work with `--as {}`. \
         The number on a row is spread along your links, not a rank of what is true. \
         Pass `--fire` only after you have used that island. Fire rewrites your weights, not the seat's, and the next walk of the same cue follows them. \
         End with one ballot: `ljos vote {{issue}} --for OPTION --confidence P --used deed-... --as {}`. \
         P is the probability you give that the choice is the outcome. \
         --used none records that the ballot drew on no deed. \
         The line it prints is a count. `ljos consensus {{issue}}` is the settle. \
         A lesson of your own goes in with `ljos remember --as {} \"...\"`.\n",
        p.name, p.name, p.name
    ));
    Ok(out)
}

/// A panel for a runner with no MCP: one brief per persona written to
/// `out`, named `<persona>.md`, and the lines that run it. A runner starts
/// one subagent per file, each ends with the ballot its brief names, and
/// `ljos consensus ISSUE` settles.
///
/// # Errors
///
/// No personas in the pack, or a brief that cannot be written.
/// The personas that speak to an issue: those whose domains meet the
/// words of its title or the entities of the island it activates. A pack
/// shared by many projects holds reviewers for all of them, and a panel on
/// a docs ticket does not want the CUDA reviewer. None matching, all sit.
#[must_use]
/// The roster, one persona per line: name, anchor, the domains it speaks
/// to, its view. Empty pack: one line saying how to write the first one.
pub fn format_personas(personas: &[Persona]) -> String {
    if personas.is_empty() {
        return "no personas; `ljos persona NAME --anchor A --view \"...\" --about DOMAIN` writes one\n"
            .to_string();
    }
    let width = personas.iter().map(|p| p.name.len()).max().unwrap_or(0);
    personas
        .iter()
        .map(|p| {
            format!(
                "{:width$}  anchor {:.2}  {}  {}\n",
                p.name,
                p.anchor,
                if p.entities.is_empty() {
                    "about anything".to_string()
                } else {
                    format!("about {}", p.entities.join(", "))
                },
                p.view
            )
        })
        .collect()
}

pub fn personas_speaking_to(personas: &[Persona], words: &[String]) -> Vec<Persona> {
    let words: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    let speaking: Vec<Persona> = personas
        .iter()
        .filter(|p| {
            p.entities
                .iter()
                .any(|d| words.iter().any(|w| w == &d.to_lowercase()))
        })
        .cloned()
        .collect();
    if !speaking.is_empty() {
        return speaking;
    }
    // No domain matched. Personas with no domains speak to every issue.
    // Specialists stay seated out: seating the whole pack is a count.
    let general: Vec<Persona> = personas
        .iter()
        .filter(|p| p.entities.is_empty())
        .cloned()
        .collect();
    if !general.is_empty() {
        return general;
    }
    // A pack of specialists only: seat the few whose own view uses the
    // issue's words most, so a decision still has voters with a view on it.
    let mut ranked: Vec<(usize, &Persona)> = personas
        .iter()
        .map(|p| {
            let view = p.view.to_lowercase();
            let hits = words
                .iter()
                .filter(|w| w.chars().count() > 3 && view.contains(w.as_str()))
                .count();
            (hits, p)
        })
        .filter(|(hits, _)| *hits > 0)
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    ranked
        .into_iter()
        .take(PANEL_BY_VIEW)
        .map(|(_, p)| p.clone())
        .collect()
}

/// How many specialists a panel seats by their views when no domain and no
/// generalist speaks to the issue.
pub const PANEL_BY_VIEW: usize = 5;

/// The words an issue speaks in: its title's topic words, its tags, and
/// the entities of the island its title activates when that island is not
/// weak.
pub fn issue_words(issue: &str) -> Vec<String> {
    let title = issue_title(issue).unwrap_or_default();
    let mut words = topic_words(&title);
    // The tags the issue's author chose name its domains outright.
    if let Ok(v) = tracker_show_json(issue) {
        words.extend(tags_of(&v));
    }
    // A weak island is the pack's best-connected cluster, not what the title
    // is about: its entities seated five course reviewers on a question
    // about syncing memory. Only an island two scorers agreed on speaks.
    if packset_island(&title, false).is_ok_and(|i| !i["weak"].as_bool().unwrap_or(false)) {
        words.extend(island_entities(issue).unwrap_or_default());
    }
    words
}

/// An issue's tags from its tracker record, lower-cased.
fn tags_of(v: &Value) -> Vec<String> {
    v["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_lowercase)
        .collect()
}

pub fn panel(issue: &str, out: &Path) -> Result<String> {
    if bound_playbook(issue).is_none() {
        bail!(
            "panel: no playbook bound on {issue}; `ljos playbook {issue} NAME` or \
             `ljos sitting {issue} --playbook NAME` names one before personas enter"
        );
    }
    let all = personas_from_pack()?;
    if all.is_empty() {
        bail!("panel: the pack holds no personas; `ljos persona NAME --anchor A --view ...` writes one");
    }
    let words = issue_words(issue);
    let personas = personas_speaking_to(&all, &words);
    if personas.is_empty() {
        bail!(
            "panel: none of the {} personas speaks to {issue}: none holds its words ({}) as a \
             domain or in its view. Tag the issue with a domain a persona holds, or write the \
             briefs by hand with `ljos brief NAME {issue}`",
            all.len(),
            words.join(", ")
        );
    }
    std::fs::create_dir_all(out)?;
    let mut lines = vec![format!(
        "{} of {} personas speak to {issue}; briefs in {}; start one subagent per file, each ends with its ballot, then:",
        personas.len(),
        all.len(),
        out.display()
    )];
    for p in &personas {
        let path = out.join(format!("{}.md", p.name));
        std::fs::write(&path, brief(&p.name, issue)?)?;
        lines.push(format!("  {}", path.display()));
    }
    lines.push(format!("ljos consensus {issue}"));
    Ok(lines.join("\n") + "\n")
}

/// One voter's forecast on one issue: what share the others give each
/// option, or the option it expects to win.
#[derive(Debug, Clone, PartialEq)]
pub struct Prediction {
    pub issue: String,
    pub agent: String,
    pub expect: Value,
}

/// POST one forecast. `expect` is an option name or `{option: share}`.
pub fn write_prediction(issue: &str, agent: &str, expect: &str) -> Result<Value> {
    let (issue, agent, expect) = (issue.trim(), agent.trim(), expect.trim());
    if issue.is_empty() || agent.is_empty() || expect.is_empty() {
        bail!("predict: an issue, an identity and an expectation are required");
    }
    let expect_value: Value = match serde_json::from_str::<Value>(expect) {
        Ok(v @ Value::Object(_)) => v,
        _ => Value::String(expect.to_string()),
    };
    let client = pack()?;
    let workspace = client.workspace();
    let mut atom = atom_body(
        "prediction",
        &format!("{agent} expects {expect} on {issue}."),
        &workspace,
    );
    atom["issue"] = Value::String(issue.into());
    atom["agent"] = Value::String(agent.into());
    atom["expect"] = expect_value;
    client
        .post_atom(&atom)
        .context("predict: POST /v1/atoms failed")
}

/// The latest forecast per agent on an issue.
pub fn predictions_of(atoms: &[Value], issue: &str) -> Vec<Prediction> {
    let mut latest: std::collections::BTreeMap<String, (String, Prediction)> =
        std::collections::BTreeMap::new();
    for atom in atoms {
        if atom.get("kind").and_then(Value::as_str) != Some("prediction")
            || atom.get("issue").and_then(Value::as_str) != Some(issue)
        {
            continue;
        }
        let (Some(agent), Some(expect)) = (
            atom.get("agent").and_then(Value::as_str),
            atom.get("expect"),
        ) else {
            continue;
        };
        let ts = atom
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let p = Prediction {
            issue: issue.to_string(),
            agent: agent.to_string(),
            expect: expect.clone(),
        };
        match latest.get(agent) {
            Some((seen, _)) if *seen > ts => {}
            _ => {
                latest.insert(agent.to_string(), (ts, p));
            }
        }
    }
    latest.into_values().map(|(_, p)| p).collect()
}

/// Forecasts as `ljos-consensus surprising --predictions` takes them.
pub fn predictions_json(predictions: &[Prediction]) -> String {
    Value::Array(
        predictions
            .iter()
            .map(|p| serde_json::json!({"agent": p.agent, "expect": p.expect}))
            .collect(),
    )
    .to_string()
}

/// Argv law kept in the pack: a glob over the command line, a verdict, and
/// the reason a reader sees when it fires. `deny` stops the action at the
/// runner and under `ljos policy`; `ask` hands it to the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: String,
    pub verdict: String,
    pub reason: String,
}

/// POST one rule.
pub fn write_rule(rule: &Rule) -> Result<Value> {
    let pattern = rule.pattern.trim();
    if pattern.is_empty() {
        bail!("rule: a pattern over the command line is required");
    }
    if !matches!(rule.verdict.as_str(), "deny" | "ask") {
        bail!("rule: the verdict is deny or ask, not {:?}", rule.verdict);
    }
    let reason = rule.reason.trim();
    if reason.is_empty() {
        bail!("rule: say in a sentence why, so the reader who is stopped knows");
    }
    let client = pack()?;
    let workspace = client.workspace();
    let mut atom = atom_body("rule", reason, &workspace);
    atom["pattern"] = Value::String(pattern.into());
    atom["verdict"] = Value::String(rule.verdict.clone());
    client
        .post_atom(&atom)
        .context("rule: POST /v1/atoms failed")
}

/// The live rules in a set of atoms.
pub fn rules_of(atoms: &[Value]) -> Vec<Rule> {
    atoms
        .iter()
        .filter(|a| a.get("kind").and_then(Value::as_str) == Some("rule"))
        .filter_map(|a| {
            Some(Rule {
                pattern: a.get("pattern")?.as_str()?.to_string(),
                verdict: a.get("verdict")?.as_str()?.to_string(),
                reason: a
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

/// The rules in the seat's pack.
pub fn rules_from_pack() -> Result<Vec<Rule>> {
    let client = pack()?;
    let atoms = atoms_lean(&client, &client.workspace()).context("rules: GET /v1/atoms failed")?;
    Ok(rules_of(&atoms))
}

/// A glob over a command line: `*` matches any run of characters, `?` one.
/// The match is on the whole line, so `rm -rf *` is `rm -rf ` and anything
/// after, and `*sudo*` is sudo anywhere.
#[must_use]
pub fn glob_matches(pattern: &str, line: &str) -> bool {
    fn go(p: &[char], l: &[char]) -> bool {
        match (p.first(), l.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], l) || (!l.is_empty() && go(p, &l[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &l[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &l[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let l: Vec<char> = line.trim().chars().collect();
    go(&p, &l)
}

/// The verdict the rules give a command line: the first `deny` wins, then
/// the first `ask`, else none. Returns the rule that fired.
#[must_use]
pub fn verdict_for<'a>(rules: &'a [Rule], line: &str) -> Option<&'a Rule> {
    rules
        .iter()
        .find(|r| r.verdict == "deny" && glob_matches(&r.pattern, line))
        .or_else(|| {
            rules
                .iter()
                .find(|r| r.verdict == "ask" && glob_matches(&r.pattern, line))
        })
}

/// Anchors as the settles take them: `{"name": anchor, ...}`.
pub fn anchors_json(personas: &[Persona]) -> String {
    let map: serde_json::Map<String, Value> = personas
        .iter()
        .map(|p| (p.name.clone(), serde_json::json!(p.anchor)))
        .collect();
    Value::Object(map).to_string()
}

/// The entities that name a domain: every entity but the seat that wrote
/// the atom, which says who, not what.
fn domains_of(v: Option<&Value>) -> Vec<String> {
    words_of(v)
        .into_iter()
        .filter(|e| !e.starts_with(SEAT_ENTITY))
        .collect()
}

fn words_of(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_lowercase)
        .collect()
}

/// The domains an issue's island speaks to: the entities of the memories
/// its title activates, most frequent first, eight at most. What `learn`
/// scopes its rows to.
///
/// # Errors
///
/// The tracker or the pack not answering.
pub fn island_entities(issue: &str) -> Result<Vec<String>> {
    let title = issue_title(issue)?;
    let island = packset_island(&title, false)?;
    let ids: Vec<&str> = island["island"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["id"].as_str())
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let client = pack()?;
    let atoms = atoms_lean(&client, &client.workspace()).context("island: GET /v1/atoms failed")?;
    let mut count: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for atom in &atoms {
        if atom
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| ids.contains(&id))
        {
            for e in words_of(atom.get("entities")) {
                *count.entry(e).or_insert(0) += 1;
            }
        }
    }
    let mut ranked: Vec<(String, usize)> = count.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(ranked.into_iter().take(8).map(|(e, _)| e).collect())
}

/// The words an issue is about, for scoping trust rows: its title, lower
/// case, three letters or longer.
pub fn topic_words(title: &str) -> Vec<String> {
    let mut words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_lowercase)
        .collect();
    words.sort_unstable();
    words.dedup();
    words
}

/// The rows that apply to an issue about `topic`: every unscoped row, and
/// every scoped row one of whose domains is among the topic's words.
pub fn rows_about(rows: &[Trust], topic: &[String]) -> Vec<Trust> {
    // A scoped row that applies stands in for the unscoped row of the same
    // pair, so the settle sees one weight per pair and never a sum of two.
    let mut chosen: std::collections::BTreeMap<(String, String), Trust> =
        std::collections::BTreeMap::new();
    for r in rows {
        let applies = r.about.is_empty() || r.about.iter().any(|a| topic.contains(a));
        if !applies {
            continue;
        }
        let key = (r.from.clone(), r.to.clone());
        match chosen.get(&key) {
            Some(have) if !have.about.is_empty() && r.about.is_empty() => {}
            _ => {
                chosen.insert(key, r.clone());
            }
        }
    }
    chosen.into_values().collect()
}

/// The personas after an outcome: one whose ballot the outcome refuted
/// moves its anchor toward one by `1 - beta` of the gap, so a persona that
/// keeps being wrong listens more; a vindicated one keeps its anchor. The
/// personas that voted are the only ones touched. Acemoglu, Como, Fagnani
/// and Ozdaglar (doi:10.1287/moor.1120.0570) show what a stubborn wrong
/// voter does to a pool; this is the seat's remedy.
#[must_use]
pub fn learn_anchors(
    personas: &[Persona],
    ballots: &[(String, String)],
    outcome: &str,
    beta: f64,
) -> Vec<Persona> {
    let outcome = outcome.trim();
    personas
        .iter()
        .filter(|p| {
            ballots
                .iter()
                .any(|(agent, choice)| *agent == p.name && choice != outcome)
        })
        .map(|p| Persona {
            anchor: (p.anchor + (1.0 - p.anchor) * (1.0 - beta)).min(1.0),
            ..p.clone()
        })
        .collect()
}

/// [`learn_about`] and [`learn_anchors`] together, written to the pack:
/// the rows, then the personas the outcome moved. Returns what was written.
///
/// # Errors
///
/// The pack refusing a row or a persona.
/// A ballot as a forecast: the choice, and the probability the voter stated
/// for that choice. Absent confidence is not a claim of certainty.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub agent: String,
    pub choice: String,
    pub confidence: Option<f64>,
}

/// Quadratic score of a stated probability against the outcome.
///
/// `p` is the probability the voter assigned to its own choice being the
/// outcome. The outcome indicator is 1 when the choice matches and 0
/// otherwise. The score is `(p - o)^2` (Brier 1950; Gneiting and Raftery
/// 2007, doi:10.1198/016214506000001437). Lower is better. It is not a
/// trust weight.
#[must_use]
pub fn brier(choice: &str, outcome: &str, p: f64) -> f64 {
    let o = if choice == outcome { 1.0 } else { 0.0 };
    let d = p - o;
    d * d
}

/// Logarithmic score of the probability assigned to the event that occurred.
///
/// Good 1952, doi:10.1111/j.2517-6161.1952.tb00104.x. The score is
/// `-ln` of the probability the forecast put on what happened. It is
/// unbounded when that probability is 0, which a stated certainty on the
/// wrong choice is. `None` in that case, rather than a stand-in number.
#[must_use]
pub fn log_score(choice: &str, outcome: &str, p: f64) -> Option<f64> {
    let assigned = if choice == outcome { p } else { 1.0 - p };
    if assigned <= 0.0 {
        None
    } else {
        Some(-assigned.ln())
    }
}

/// Mean logarithmic score over the forecasts that stated a probability,
/// how many of those scores were finite, and how many were unbounded.
#[must_use]
pub fn mean_log(rows: &[Forecast], outcome: &str) -> (Option<f64>, usize, usize) {
    let mut sum = 0.0;
    let mut finite = 0usize;
    let mut unbounded = 0usize;
    for row in rows {
        let Some(p) = row.confidence else { continue };
        match log_score(&row.choice, outcome, p) {
            Some(score) => {
                sum += score;
                finite += 1;
            }
            None => unbounded += 1,
        }
    }
    let mean = (finite > 0).then_some(sum / finite as f64);
    (mean, finite, unbounded)
}

/// One voter's forecast record. The bins are the probabilities actually
/// stated, in thousandths, each with how many times it was stated and how
/// many of those events occurred. Murphy's categories are those values,
/// not a grid this seat invented.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Calibration {
    pub n: u32,
    pub sum_p: f64,
    pub sum_o: f64,
    pub sum_brier: f64,
    pub sum_log: f64,
    pub log_n: u32,
    pub bins: std::collections::BTreeMap<u16, (u32, u32)>,
}

/// Murphy's partition of the Brier score (1973,
/// doi:10.1175/1520-0450(1973)012<0595:ANVPOT>2.0.CO;2).
/// `brier = reliability - resolution + uncertainty`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Partition {
    pub reliability: f64,
    pub resolution: f64,
    pub uncertainty: f64,
}

/// Add one stated probability to a voter's record.
#[must_use]
pub fn observe(cal: &Calibration, choice: &str, outcome: &str, p: f64) -> Calibration {
    let mut next = cal.clone();
    let occurred = choice == outcome;
    let o = if occurred { 1.0 } else { 0.0 };
    next.n += 1;
    next.sum_p += p;
    next.sum_o += o;
    next.sum_brier += brier(choice, outcome, p);
    if let Some(score) = log_score(choice, outcome, p) {
        next.sum_log += score;
        next.log_n += 1;
    }
    let key = (p.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let slot = next.bins.entry(key).or_insert((0, 0));
    slot.0 += 1;
    if occurred {
        slot.1 += 1;
    }
    next
}

/// Reliability, resolution, and uncertainty. `None` until the voter has
/// two forecasts: one forecast makes the partition the score itself.
#[must_use]
pub fn murphy(cal: &Calibration) -> Option<Partition> {
    if cal.n < 2 || cal.bins.is_empty() {
        return None;
    }
    let n = f64::from(cal.n);
    let base = cal.sum_o / n;
    let mut reliability = 0.0;
    let mut resolution = 0.0;
    for (thou, (count, occurred)) in &cal.bins {
        let nk = f64::from(*count);
        if nk == 0.0 {
            continue;
        }
        let forecast = f64::from(*thou) / 1000.0;
        let rate = f64::from(*occurred) / nk;
        reliability += nk * (forecast - rate) * (forecast - rate);
        resolution += nk * (rate - base) * (rate - base);
    }
    Some(Partition {
        reliability: reliability / n,
        resolution: resolution / n,
        uncertainty: base * (1.0 - base),
    })
}

/// Mean Brier score over the forecasts that stated a probability, and how
/// many those were. `None` when nobody stated one.
#[must_use]
pub fn mean_brier(rows: &[Forecast], outcome: &str) -> Option<(f64, usize)> {
    let scores: Vec<f64> = rows
        .iter()
        .filter_map(|r| r.confidence.map(|p| brier(&r.choice, outcome, p)))
        .collect();
    if scores.is_empty() {
        None
    } else {
        Some((
            scores.iter().sum::<f64>() / scores.len() as f64,
            scores.len(),
        ))
    }
}

/// `(agent, choice, confidence)` from a tracker's `vote --json`.
pub fn forecasts_from_json(raw: &str) -> Result<Vec<Forecast>> {
    let rows: Vec<Value> = serde_json::from_str(raw).context("ballots: not a JSON array")?;
    rows.iter()
        .map(|row| {
            let agent = row.get("agent").and_then(Value::as_str);
            let choice = row.get("choice").and_then(Value::as_str);
            let confidence = match row.get("confidence") {
                None | Some(Value::Null) => None,
                Some(value) => {
                    let probability = value
                        .as_f64()
                        .or_else(|| value.as_str()?.parse::<f64>().ok())
                        .context("ballots: confidence must be a probability in (0, 1]")?;
                    if !probability.is_finite() || probability <= 0.0 || probability > 1.0 {
                        bail!("ballots: confidence must be a probability in (0, 1]");
                    }
                    Some(probability)
                }
            };
            match (agent, choice) {
                (Some(a), Some(c)) => Ok(Forecast {
                    agent: a.to_string(),
                    choice: c.to_string(),
                    confidence,
                }),
                _ => bail!("ballots: a row without agent and choice"),
            }
        })
        .collect()
}

/// What a learn did. The rows are the next settle's weights. This call is not a settle.
/// The scores, when any ballot stated a probability, are not trust weights.
/// `calibration` is each voter's record after this outcome is folded in.
#[must_use]
pub fn learn_reading(
    rows: usize,
    moved: usize,
    forecasts: &[Forecast],
    outcome: &str,
    calibration: &std::collections::BTreeMap<String, Calibration>,
) -> String {
    let mut out = format!(
        "Learned. {rows} trust rows rewritten. A voter the outcome refuted shrinks; a vindicated one keeps its weight. {moved} persona anchors moved. This is not a new settle; the next ljos consensus uses these rows."
    );
    match mean_brier(forecasts, outcome) {
        Some((mean, n)) => {
            let silent = forecasts.len().saturating_sub(n);
            out.push_str(&format!(
                " Brier {mean:.3} over {n} stated probabilities (doi:10.1198/016214506000001437). {silent} ballots stated none and were not scored. The score is not a trust weight."
            ));
        }
        None => out.push_str(
            " No stated probability, so there is no Brier score. A hard vote is not a claim of certainty.",
        ),
    }
    let (mean_log, finite, unbounded) = mean_log(forecasts, outcome);
    if let Some(mean) = mean_log {
        out.push_str(&format!(
            " Logarithmic score {mean:.3} over {finite} (doi:10.1111/j.2517-6161.1952.tb00104.x)."
        ));
    }
    if unbounded > 0 {
        out.push_str(&format!(
            " {unbounded} assigned probability 0 to the event that occurred, so those logarithmic scores are unbounded."
        ));
    }
    let mut named: Vec<(&str, &Calibration)> = forecasts
        .iter()
        .filter(|f| f.confidence.is_some())
        .filter_map(|f| calibration.get(&f.agent).map(|cal| (f.agent.as_str(), cal)))
        .collect();
    named.sort_by(|a, b| {
        let gap = |c: &Calibration| {
            if c.n == 0 {
                0.0
            } else {
                (c.sum_p / f64::from(c.n) - c.sum_o / f64::from(c.n)).abs()
            }
        };
        gap(b.1)
            .partial_cmp(&gap(a.1))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(b.0))
    });
    named.dedup_by_key(|row| row.0);
    for (name, cal) in named.into_iter().take(8) {
        if cal.n == 0 {
            continue;
        }
        let n = f64::from(cal.n);
        let mean_p = cal.sum_p / n;
        let rate = cal.sum_o / n;
        out.push_str(&format!(
            " {name}: {} forecasts, mean probability {mean_p:.3}, event rate {rate:.3} (doi:10.1080/01621459.1982.10477856)",
            cal.n
        ));
        if let Some(part) = murphy(cal) {
            out.push_str(&format!(
                "; reliability {:.3}, resolution {:.3}, uncertainty {:.3} (doi:10.1175/1520-0450(1973)012<0595:ANVPOT>2.0.CO;2)",
                part.reliability, part.resolution, part.uncertainty
            ));
        }
        out.push('.');
    }
    out
}

/// Trust rows, personas, and each voter's forecast calibration.
pub type LearnedState = (
    Vec<Trust>,
    Vec<Persona>,
    std::collections::BTreeMap<String, Calibration>,
);

pub fn learn_and_write(
    ballots: &[(String, String)],
    outcome: &str,
    beta: f64,
    about: &[String],
    forecasts: &[Forecast],
) -> Result<LearnedState> {
    let client = pack()?;
    let atoms = atoms_lean(&client, &client.workspace()).context("learn: GET /v1/atoms failed")?;
    let (rows, records) = learn_record(ballots, outcome, &records_from_atoms(&atoms), about)?;
    let mut calibration = calibration_from_atoms(&atoms);
    for forecast in forecasts {
        let Some(p) = forecast.confidence else {
            continue;
        };
        let slot = calibration.entry(forecast.agent.clone()).or_default();
        *slot = observe(slot, &forecast.choice, outcome, p);
    }
    let moved = learn_anchors(&personas_from_pack()?, ballots, outcome, beta);
    // Every row lands before anything is printed, so a closed pipe cannot
    // leave the graph half written.
    for row in &rows {
        write_trust_record(
            row,
            &[],
            records.get(&row.to).copied(),
            calibration.get(&row.to),
        )?;
    }
    for p in &moved {
        write_persona(p)?;
    }
    Ok((rows, moved, calibration))
}

/// A voter's record: how often the outcome agreed with its ballot, and
/// how often not, carried on every trust row into that voter.
pub type Standing = (f64, f64);

/// The latest record per voter among the trust atoms that carry one.
#[must_use]
pub fn records_from_atoms(atoms: &[Value]) -> std::collections::BTreeMap<String, Standing> {
    let mut latest: std::collections::BTreeMap<String, (String, Standing)> =
        std::collections::BTreeMap::new();
    for atom in atoms {
        if atom.get("kind").and_then(Value::as_str) != Some("trust") {
            continue;
        }
        let (Some(to), Some(hits), Some(misses)) = (
            atom.get("to").and_then(Value::as_str),
            atom.get("hits").and_then(Value::as_f64),
            atom.get("misses").and_then(Value::as_f64),
        ) else {
            continue;
        };
        let ts = atom
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match latest.get(to) {
            Some((seen, _)) if *seen > ts => {}
            _ => {
                latest.insert(to.to_string(), (ts, (hits, misses)));
            }
        }
    }
    latest.into_iter().map(|(k, (_, r))| (k, r)).collect()
}

/// Learn from an outcome by the record: each voter's hits and misses so
/// far, this outcome added, give its accuracy with one of each smoothed
/// in, and the rows are the log odds of that scaled to the best voter at
/// one ([`calibration_weights`]). Measured against multiplicative
/// shrinking (Hedge) on voters of known accuracy, the record reaches the
/// batch calibration and the shrink does not: a voter is weighed by what
/// it got right, not by how many times it has been punished. Rows are
/// complete over the voters and scoped to `about`.
///
/// # Errors
///
/// No outcome, or fewer than two voters.
pub fn learn_record(
    ballots: &[(String, String)],
    outcome: &str,
    records: &std::collections::BTreeMap<String, Standing>,
    about: &[String],
) -> Result<(Vec<Trust>, std::collections::BTreeMap<String, Standing>)> {
    let outcome = outcome.trim();
    if outcome.is_empty() {
        bail!("learn: an outcome is required");
    }
    let mut agents: Vec<&str> = ballots.iter().map(|(a, _)| a.as_str()).collect();
    agents.sort_unstable();
    agents.dedup();
    if agents.len() < 2 {
        bail!("learn: fewer than two voters, nothing to weigh");
    }
    let mut next = records.clone();
    for (agent, choice) in ballots {
        let r = next.entry(agent.clone()).or_insert((0.0, 0.0));
        if choice == outcome {
            r.0 += 1.0;
        } else {
            r.1 += 1.0;
        }
    }
    let accuracy: Vec<(String, f64)> = agents
        .iter()
        .map(|a| {
            let (h, m) = next.get(*a).copied().unwrap_or((0.0, 0.0));
            ((*a).to_string(), (h + 1.0) / (h + m + 2.0))
        })
        .collect();
    let weights = calibration_weights(&accuracy);
    let mut out = Vec::new();
    for from in &agents {
        for (to, weight) in &weights {
            if *from == to {
                continue;
            }
            out.push(Trust {
                from: (*from).to_string(),
                to: to.clone(),
                weight: *weight,
                about: about.to_vec(),
            });
        }
    }
    Ok((out, next))
}

/// [`write_trust`] carrying the voter's record on the row.
pub fn write_trust_record(
    row: &Trust,
    why: &[String],
    record: Option<Standing>,
    calibration: Option<&Calibration>,
) -> Result<Value> {
    let client = pack()?;
    let workspace = client.workspace();
    let mut atom = trust_atom(row, why, &workspace)?;
    if let Some((hits, misses)) = record {
        atom["hits"] = serde_json::json!(hits);
        atom["misses"] = serde_json::json!(misses);
    }
    if let Some(cal) = calibration.filter(|c| c.n > 0) {
        atom["forecast_n"] = serde_json::json!(cal.n);
        atom["forecast_sum_p"] = serde_json::json!(cal.sum_p);
        atom["forecast_sum_o"] = serde_json::json!(cal.sum_o);
        atom["forecast_sum_brier"] = serde_json::json!(cal.sum_brier);
        atom["forecast_sum_log"] = serde_json::json!(cal.sum_log);
        atom["forecast_log_n"] = serde_json::json!(cal.log_n);
        let mut bins = serde_json::Map::new();
        for (key, (count, occurred)) in &cal.bins {
            bins.insert(key.to_string(), serde_json::json!([count, occurred]));
        }
        atom["forecast_bins"] = Value::Object(bins);
    }
    client
        .post_atom(&atom)
        .context("trust: POST /v1/atoms failed")
}

/// The latest forecast record per voter, from the trust rows that carry one.
#[must_use]
pub fn calibration_from_atoms(atoms: &[Value]) -> std::collections::BTreeMap<String, Calibration> {
    let mut latest: std::collections::BTreeMap<String, (String, Calibration)> =
        std::collections::BTreeMap::new();
    for atom in atoms {
        if atom.get("kind").and_then(Value::as_str) != Some("trust") {
            continue;
        }
        let Some(to) = atom.get("to").and_then(Value::as_str) else {
            continue;
        };
        let Some(n) = atom.get("forecast_n").and_then(Value::as_u64) else {
            continue;
        };
        let ts = atom
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let cal = Calibration {
            n: n as u32,
            sum_p: atom
                .get("forecast_sum_p")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            sum_o: atom
                .get("forecast_sum_o")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            sum_brier: atom
                .get("forecast_sum_brier")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            sum_log: atom
                .get("forecast_sum_log")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            log_n: atom
                .get("forecast_log_n")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
            bins: bins_of(atom.get("forecast_bins")),
        };
        match latest.get(to) {
            Some((seen, _)) if *seen > ts => {}
            _ => {
                latest.insert(to.to_string(), (ts, cal));
            }
        }
    }
    latest.into_iter().map(|(k, (_, cal))| (k, cal)).collect()
}

fn bins_of(value: Option<&Value>) -> std::collections::BTreeMap<u16, (u32, u32)> {
    let mut out = std::collections::BTreeMap::new();
    let Some(obj) = value.and_then(Value::as_object) else {
        return out;
    };
    for (key, row) in obj {
        let Ok(thou) = key.parse::<u16>() else {
            continue;
        };
        let Some(pair) = row.as_array() else { continue };
        let count = pair.first().and_then(Value::as_u64).unwrap_or(0) as u32;
        let occurred = pair.get(1).and_then(Value::as_u64).unwrap_or(0) as u32;
        out.insert(thou, (count, occurred));
    }
    out
}

/// The factor a refuted voter's rows shrink by (Hedge, doi:10.1006/jcss.1997.1504).
pub const LEARN_BETA: f64 = 0.5;

/// The least a row can fall to, so a voter who is right again is heard again.
pub const TRUST_FLOOR: f64 = 0.01;

/// A `trust` atom for one row. `why` are deed accessions it cites.
pub fn trust_atom(row: &Trust, why: &[String], workspace: &str) -> Result<Value> {
    let (from, to) = (row.from.trim(), row.to.trim());
    if from.is_empty() || to.is_empty() {
        bail!("trust: from and to are required");
    }
    if from == to {
        bail!("trust: {from} cannot weigh itself; self weight is the settle's");
    }
    if !(row.weight > 0.0 && row.weight <= 1.0) {
        bail!("trust: weight {} is not in (0, 1]", row.weight);
    }
    let mut atom = atom_body(
        "trust",
        &format!("{from} weighs {to} at {:.3}.", row.weight),
        workspace,
    );
    atom["from"] = Value::String(from.into());
    atom["to"] = Value::String(to.into());
    atom["weight"] = serde_json::json!(row.weight);
    // A trust row's entities are the deeds it stands on. The pack refuses
    // an entity that is not an accession. Who wrote the row is `from`.
    for w in why {
        if !w.starts_with("deed-") && !w.starts_with("sha256:") {
            bail!("trust: {w} is not a deed accession");
        }
    }
    atom["entities"] = Value::Array(why.iter().map(|w| Value::String(w.clone())).collect());
    if !row.about.is_empty() {
        atom["about"] = Value::Array(
            row.about
                .iter()
                .map(|w| Value::String(w.to_lowercase()))
                .collect(),
        );
    }
    Ok(atom)
}

/// The live rows in a set of atoms: the latest `trust` atom per `(from, to)`.
pub fn trust_rows(atoms: &[Value]) -> Vec<Trust> {
    // The latest row per (from, to, scope): an unscoped row and a scoped one
    // for the same pair are different rows, and a later row of the same
    // scope supersedes.
    let mut latest: std::collections::BTreeMap<(String, String, Vec<String>), (String, f64)> =
        std::collections::BTreeMap::new();
    for atom in atoms {
        if atom.get("kind").and_then(Value::as_str) != Some("trust") {
            continue;
        }
        let (Some(from), Some(to), Some(weight)) = (
            atom.get("from").and_then(Value::as_str),
            atom.get("to").and_then(Value::as_str),
            atom.get("weight").and_then(Value::as_f64),
        ) else {
            continue;
        };
        let ts = atom
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut about = words_of(atom.get("about"));
        about.sort_unstable();
        let key = (from.to_string(), to.to_string(), about);
        match latest.get(&key) {
            Some((seen, _)) if *seen > ts => {}
            _ => {
                latest.insert(key, (ts, weight));
            }
        }
    }
    latest
        .into_iter()
        .map(|((from, to, about), (_, weight))| Trust {
            from,
            to,
            weight,
            about,
        })
        .collect()
}

/// Rows as the consensus takes them: `[[from, to, weight], ...]`.
pub fn trust_json(rows: &[Trust]) -> String {
    let tuples: Vec<Value> = rows
        .iter()
        .map(|r| serde_json::json!([r.from, r.to, r.weight]))
        .collect();
    Value::Array(tuples).to_string()
}

/// `(agent, choice)` pairs from a tracker's `vote --json`.
pub fn ballots_from_json(raw: &str) -> Result<Vec<(String, String)>> {
    let rows: Vec<Value> = serde_json::from_str(raw).context("ballots: not a JSON array")?;
    rows.iter()
        .map(|row| {
            let agent = row.get("agent").and_then(Value::as_str);
            let choice = row.get("choice").and_then(Value::as_str);
            match (agent, choice) {
                (Some(a), Some(c)) => Ok((a.to_string(), c.to_string())),
                _ => bail!("ballots: a row without agent and choice"),
            }
        })
        .collect()
}

/// The rows every voter holds on every other after `outcome` is known: a
/// voter whose ballot was refuted shrinks by `beta`, floored at
/// [`TRUST_FLOOR`]; a missing row starts at one. Complete, so the settle
/// sees the whole graph.
pub fn learn(
    ballots: &[(String, String)],
    outcome: &str,
    rows: &[Trust],
    beta: f64,
) -> Result<Vec<Trust>> {
    learn_about(ballots, outcome, rows, beta, &[])
}

/// [`learn`] writing rows scoped to `about`: the domains the issue's island
/// speaks to, so that being wrong about one topic does not cost a voter its
/// standing on every other. An empty `about` is the unscoped rule.
pub fn learn_about(
    ballots: &[(String, String)],
    outcome: &str,
    rows: &[Trust],
    beta: f64,
    about: &[String],
) -> Result<Vec<Trust>> {
    learn_shared(ballots, outcome, rows, beta, about, 0.0)
}

/// [`learn_about`] with a fixed share of recovery: after the Hedge step
/// every row moves toward one by `share` of the gap, so a voter refuted
/// long ago is not held down forever and the best voter can change
/// (Herbster and Warmuth, doi:10.1023/A:1007424614876). Zero is plain
/// Hedge; the seat's default.
pub fn learn_shared(
    ballots: &[(String, String)],
    outcome: &str,
    rows: &[Trust],
    beta: f64,
    about: &[String],
    share: f64,
) -> Result<Vec<Trust>> {
    if !(beta > 0.0 && beta < 1.0) {
        bail!("learn: beta {beta} is not in (0, 1)");
    }
    if !(0.0..1.0).contains(&share) {
        bail!("learn: share {share} is not in [0, 1)");
    }
    let outcome = outcome.trim();
    if outcome.is_empty() {
        bail!("learn: an outcome is required");
    }
    let mut agents: Vec<&str> = ballots.iter().map(|(a, _)| a.as_str()).collect();
    agents.sort_unstable();
    agents.dedup();
    if agents.len() < 2 {
        bail!("learn: fewer than two voters, nothing to weigh");
    }
    let refuted = |agent: &str| {
        ballots
            .iter()
            .any(|(a, choice)| a == agent && choice != outcome)
    };
    let mut out = Vec::new();
    for from in &agents {
        for to in &agents {
            if from == to {
                continue;
            }
            // The row being moved is the one of this scope; a scoped learn
            // starts from the unscoped row when it has none of its own.
            let current = rows
                .iter()
                .find(|r| r.from == *from && r.to == *to && r.about == about)
                .or_else(|| {
                    rows.iter()
                        .find(|r| r.from == *from && r.to == *to && r.about.is_empty())
                })
                .map_or(1.0, |r| r.weight);
            let stepped = if refuted(to) {
                (current * beta).max(TRUST_FLOOR)
            } else {
                current
            };
            let next = stepped + (1.0 - stepped) * share;
            out.push(Trust {
                from: (*from).to_string(),
                to: (*to).to_string(),
                weight: next,
                about: about.to_vec(),
            });
        }
    }
    Ok(out)
}

/// The live trust rows in the seat's pack.
pub fn trust_from_pack() -> Result<Vec<Trust>> {
    let client = pack()?;
    let workspace = client.workspace();
    let atoms = atoms_lean(&client, &workspace).context("trust: GET /v1/atoms failed")?;
    Ok(trust_rows(&atoms))
}

/// POST one trust row.
pub fn write_trust(row: &Trust, why: &[String]) -> Result<Value> {
    let client = pack()?;
    let workspace = client.workspace();
    client
        .post_atom(&trust_atom(row, why, &workspace)?)
        .context("trust: POST /v1/atoms failed")
}

/// One habitat and whether it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Habitat {
    pub name: &'static str,
    pub state: String,
    pub ok: bool,
}

/// One line after a pack write: id, kind, due, text. Not the embedding.
#[must_use]
pub fn format_write_ack(body: &serde_json::Value) -> String {
    format!(
        "{}\t{}\tdue {}\t{}",
        body["id"].as_str().unwrap_or("?"),
        body["kind"].as_str().unwrap_or("?"),
        body["due_at"].as_str().unwrap_or("-"),
        body["text"].as_str().unwrap_or("").replace('\n', " "),
    )
}

/// The habitats the seat needs. Encoder and policyd move with the rest.
pub const REQUIRED: &[&str] = &[
    "ljos",
    "ljos-mcp",
    "ljos-policyd",
    "vissue",
    "deedar",
    "claimdag",
    "packset",
    "packsetd",
    "packset-embed",
    "pack",
    "encoder",
];

/// Binary on PATH and the crates.io name it should track.
const SEAT_BINS: &[(&str, &str)] = &[
    ("ljos", "ljos"),
    // The published `ljos` crate ships this binary. The crates.io name
    // `ljos-mcp` stopped at 0.14.0 and is not the binary's version line.
    ("ljos-mcp", "ljos"),
    ("ljos-policyd", "ljos-policyd"),
    ("ljos-consensus", "ljos-consensus"),
    ("vissue", "vissue-cli"),
    ("deedar", "deedar-cli"),
    ("claimdag", "claimdag-cli"),
    ("packset", "packset"),
    ("packsetd", "packset"),
    ("packset-embed", "packset-embed"),
    ("packset-mcp", "packset"),
    ("ljos-hud", "ljos-hud"),
];

/// First `N.N.N` in a `--version` line.
#[must_use]
pub fn parse_semver(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 4 < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut dots = 0;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                if bytes[i] == b'.' {
                    dots += 1;
                }
                i += 1;
            }
            if dots >= 2 {
                return Some(&text[start..i]);
            }
        }
        i += 1;
    }
    None
}

fn bin_version(bin: &str) -> Option<String> {
    use std::process::{Command, Stdio};
    let path = which::which(bin).ok()?;
    // MCP servers that do not implement --version sit on stdio.
    // Cap the wait so doctor cannot hang the seat.
    let mut cmd = if bin.ends_with("-mcp") {
        let mut c = Command::new("timeout");
        c.args(["0.4", path.to_str()?, "--version"]);
        c
    } else {
        let mut c = Command::new(&path);
        c.arg("--version");
        c
    };
    let said = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&said.stdout);
    let stderr = String::from_utf8_lossy(&said.stderr);
    parse_semver(&stdout)
        .or_else(|| parse_semver(&stderr))
        .map(str::to_string)
}

/// A day, in seconds: how long a crates.io answer is kept on disk.
const CRATE_VERSION_TTL_S: u64 = 86_400;

/// Where a crates.io answer is kept between processes, so a herd of seats
/// opening sittings asks the registry once a day for each binary rather
/// than once a sitting each.
fn crate_version_cache(name: &str) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().ok().map(|h| h.join(".cache")))?
        .join("ljos");
    Some(dir.join(format!("crate-{name}")))
}

/// A registry answer and where it came from: the day cache on disk, or
/// the registry itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateVersion {
    pub version: String,
    pub cached: bool,
}

/// The newest version crates.io lists for `name`, from the day cache when
/// it holds one. `refresh` skips the cache: a binary on `PATH` ahead of
/// the cached answer proves the cache stale.
fn crate_max_version(name: &str, refresh: bool) -> Option<CrateVersion> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, Option<CrateVersion>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if !refresh {
        if let Ok(guard) = cache.lock() {
            if let Some(hit) = guard.get(name) {
                return hit.clone();
            }
        }
    }
    let on_disk = crate_version_cache(name);
    if let Some(path) = on_disk.as_ref().filter(|_| !refresh) {
        let fresh = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() < CRATE_VERSION_TTL_S);
        if fresh {
            if let Ok(text) = std::fs::read_to_string(path) {
                let v = text.trim();
                let got = (!v.is_empty()).then(|| CrateVersion {
                    version: v.to_string(),
                    cached: true,
                });
                if let Ok(mut guard) = cache.lock() {
                    guard.insert(name.to_string(), got.clone());
                }
                return got;
            }
        }
    }
    let url = format!("https://crates.io/api/v1/crates/{name}");
    let said = std::process::Command::new("curl")
        .args(["-sS", "-A", "ljos-doctor", "--max-time", "3", &url])
        .output()
        .ok();
    let got = said.and_then(|said| {
        if !said.status.success() {
            return None;
        }
        let v: serde_json::Value = serde_json::from_slice(&said.stdout).ok()?;
        v["crate"]["max_version"].as_str().map(|v| CrateVersion {
            version: v.to_string(),
            cached: false,
        })
    });
    if let (Some(path), Some(v)) = (&on_disk, &got) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, format!("{}\n", v.version));
    }
    if let Ok(mut guard) = cache.lock() {
        guard.insert(name.to_string(), got.clone());
    }
    got
}

fn cmp_semver(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let parse = |s: &str| -> Option<[u64; 3]> {
        let mut it = s.split('.');
        Some([
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
        ])
    };
    Some(parse(a)?.cmp(&parse(b)?))
}

/// Which habitats answer: binaries on `PATH`, the pack over `PACKSET_URL`, the
/// deed store, the tracker, the claim graph.
pub fn doctor() -> Vec<Habitat> {
    // The runner rows ask the runners' own command lines, which start slowly;
    // they run beside the seat's rows rather than after them.
    let (mut out, runners) = std::thread::scope(|s| {
        let runners = s.spawn(harness_rows);
        let seat = doctor_seat();
        (seat, runners.join().unwrap_or_default())
    });
    out.extend(runners);
    out
}

/// A binary on PATH answers even when crates.io is ahead. Sitting refuses
/// a missing required habitat, not a stale one. Behind and ahead are both
/// said; a registry answer read from the day cache says so.
fn bin_health(path: &str, have: Option<&str>, latest: Option<&CrateVersion>) -> (String, bool) {
    use std::cmp::Ordering;
    let ver = have.unwrap_or("?");
    let Some(cr) = latest else {
        return (format!("{path}  {ver}"), true);
    };
    let source = if cr.cached {
        "crates.io (cached)"
    } else {
        "crates.io"
    };
    let word = match have.and_then(|v| cmp_semver(v, &cr.version)) {
        Some(Ordering::Less) => "behind ",
        Some(Ordering::Greater) => "ahead of ",
        _ => "",
    };
    (
        format!("{path}  {ver}  {word}{source} {}", cr.version),
        true,
    )
}

/// The registry answer for a seat binary. A cached answer the binary on
/// `PATH` is already ahead of is stale by construction, so the registry
/// is asked again before the row is written.
fn crate_version_for(crate_name: &str, have: Option<&str>) -> Option<CrateVersion> {
    let first = crate_max_version(crate_name, false)?;
    let ahead = first.cached
        && have.is_some_and(|v| cmp_semver(v, &first.version) == Some(std::cmp::Ordering::Greater));
    if ahead {
        crate_max_version(crate_name, true).or(Some(first))
    } else {
        Some(first)
    }
}

/// Evidence citations and forecast confidence are part of the ballot protocol.
/// A version line alone does not establish that the tracker accepts them.
fn check_vissue_ballot_protocol(path: &Path) -> Result<()> {
    use std::process::{Command, Stdio};
    let said = Command::new("timeout")
        .arg("2")
        .arg(path)
        .args(["vote", "--help"])
        .stdin(Stdio::null())
        .output()
        .context("could not check vissue vote --help")?;
    if !said.status.success() {
        bail!("vissue vote --help failed ({})", said.status);
    }
    let help = String::from_utf8_lossy(&said.stdout);
    let missing: Vec<_> = ["--used", "--confidence"]
        .into_iter()
        .filter(|flag| !help.split_whitespace().any(|word| word == *flag))
        .collect();
    if !missing.is_empty() {
        bail!(
            "incompatible ballot protocol: missing {}; install vissue-cli >= 0.16.2",
            missing.join(", ")
        );
    }
    Ok(())
}

/// The seat's own rows: binaries, pack, host key, deed store, tracker,
/// claim graph. What a sitting checks; the runner rows are onboarding.
pub fn doctor_seat() -> Vec<Habitat> {
    let mut out = Vec::new();
    for (bin, crate_name) in SEAT_BINS {
        let found = which::which(bin).ok();
        let have = found.as_ref().and_then(|_| bin_version(bin));
        let latest = crate_version_for(crate_name, have.as_deref());
        let ballot_protocol = found
            .as_deref()
            .filter(|_| *bin == "vissue")
            .map(check_vissue_ballot_protocol);
        let (mut state, mut ok) = match (found, have.as_deref(), latest.as_ref()) {
            (None, _, Some(cr)) => (
                format!(
                    "not on PATH; cargo binstall {crate_name} (crates.io {})",
                    cr.version
                ),
                false,
            ),
            (None, _, None) => ("not on PATH".into(), false),
            (Some(path), have, Some(cr)) => bin_health(&path.display().to_string(), have, Some(cr)),
            (Some(path), have, None) => {
                let ver = have.unwrap_or("?");
                (format!("{}  {ver}", path.display()), true)
            }
        };
        if let Some(protocol) = ballot_protocol {
            match protocol {
                Ok(()) => state.push_str("; evidence ballots supported"),
                Err(error) => {
                    state.push_str(&format!("; {error:#}"));
                    ok = false;
                }
            }
        }
        out.push(Habitat {
            name: bin,
            state,
            ok,
        });
    }
    // The host the seat runs on: a kernel that OOM-kills keeps killing the
    // encoder, the runners and the desktop, and every other row stays green.
    out.push(host_row());
    // Who is sitting: the name this runner votes under, the name this
    // conversation claims under, and where they came from.
    out.push(Habitat {
        name: "seat",
        state: format_seat_row(),
        ok: true,
    });
    load_seat_env();
    // The dense ballot: without it the pack ranks by words alone, and an
    // island's seeds are weaker than the agent may assume.
    out.push(
        match PacksetClient::from_env().and_then(|c| c.status(None)) {
            Ok(status) => {
                let available = status["embedder"]["available"].as_bool().unwrap_or(false);
                let answering = status["embedder"]["answering"].as_bool();
                Habitat {
                    name: "encoder",
                    state: if available {
                        "dense ballot on".to_string()
                    } else if answering == Some(false) {
                        "packset-embed did not answer its last call (killed or crashed); \
                         ranking is lexical until packsetd restarts it on the next search"
                            .to_string()
                    } else {
                        "down; cargo binstall packset-embed and put it beside packsetd".to_string()
                    },
                    ok: available,
                }
            }
            Err(e) => Habitat {
                name: "encoder",
                state: format!("pack does not answer: {e}"),
                ok: false,
            },
        },
    );
    out.push(match pack() {
        Ok(client) => match client.health() {
            Ok(_) => Habitat {
                name: "pack",
                state: format!("{} workspace {}", client.base(), client.workspace()),
                ok: true,
            },
            Err(e) => Habitat {
                name: "pack",
                state: format!("{} does not answer: {e}", client.base()),
                ok: false,
            },
        },
        Err(_) => Habitat {
            name: "pack",
            state: "PACKSET_URL=off: no pack on purpose".into(),
            ok: false,
        },
    });
    // What the pack holds and what it let go: the seat that lets a pack
    // grow or forget under it reads it here rather than in `packset status`.
    if let Ok(client) = pack() {
        if let Ok(status) = client.status(Some(&client.workspace())) {
            let live = status["live"].as_u64().unwrap_or(0);
            let cap = status["live_cap"].as_u64().unwrap_or(0);
            let forgotten: Vec<String> = status["forgotten_by_reason"]
                .as_object()
                .map(|m| {
                    m.iter()
                        .map(|(why, n)| format!("{} by {why}", n.as_u64().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default();
            let mut state = if cap > 0 {
                format!("{live} live of {cap}")
            } else {
                format!("{live} live, no cap")
            };
            if !forgotten.is_empty() {
                state.push_str(&format!("; forgotten {}", forgotten.join(", ")));
            }
            out.push(Habitat {
                name: "memory",
                state,
                ok: cap == 0 || live <= cap,
            });
        }
    }
    out.push(match host_key_path() {
        Some(path) => {
            let seed = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) == 32;
            // A key the deed store does not list signs deeds that evidence
            // refuses. deedar says so; one without the verb is not asked.
            let unlisted = if seed {
                run_captured("deedar", &["host"])
                    .err()
                    .map(|e| e.to_string())
                    .filter(|e| e.contains("is not a signer"))
            } else {
                None
            };
            Habitat {
                name: "host key",
                state: match (&unlisted, seed) {
                    (Some(why), _) => format!(
                        "{} (32-byte seed); {}",
                        path.display(),
                        why.lines().next().unwrap_or("").trim()
                    ),
                    (None, true) => format!("{} (32-byte seed)", path.display()),
                    (None, false) => format!("{} is not a 32-byte seed", path.display()),
                },
                ok: seed && unlisted.is_none(),
            }
        }
        None => Habitat {
            name: "host key",
            state: "none at ~/.config/deedar/host.key and DEEDAR_HOST_SIGNING_KEY unset; \
                    handovers go out unsigned"
                .into(),
            ok: false,
        },
    });
    for (name, bin, args) in [
        ("deed store", "deedar", &["log", "head"][..]),
        ("tracker", "vissue", &["identity"][..]),
        ("claim graph", "claimdag", &["list"][..]),
    ] {
        out.push(match run_captured(bin, args) {
            Ok(said) if name == "tracker" => {
                let (state, ok) = tracker_state(&said.stdout, &root_source());
                Habitat { name, state, ok }
            }
            Ok(said) => Habitat {
                name,
                state: said.stdout.lines().next().unwrap_or("").to_string(),
                ok: true,
            },
            Err(e) if name == "claim graph" && claim_graph_absent(&e.to_string()).is_some() => {
                let dir = claim_graph_absent(&e.to_string()).unwrap_or_default();
                Habitat {
                    name,
                    state: format!("none yet; the first claim creates it at {dir}"),
                    ok: true,
                }
            }
            Err(e) => Habitat {
                name,
                state: e.to_string().lines().next().unwrap_or("").to_string(),
                ok: false,
            },
        });
    }
    out
}

/// The directory claimdag would create, when its refusal says the seat has
/// no work graph yet because nothing was ever claimed. A fresh host is not a
/// fault: the sitting's first claim creates the graph.
pub fn claim_graph_absent(said: &str) -> Option<String> {
    let rest = said.split("no work graph at ").nth(1)?;
    let (dir, why) = rest.split_once(": ")?;
    why.starts_with("the directory does not exist")
        .then(|| dir.trim().to_string())
}

/// Where the tracker root came from, in the order vissue decides it.
fn root_source() -> String {
    for var in ["ISSUE_ROOT", "VISSUE_ROOT"] {
        if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            return format!("{var}={}", v.to_string_lossy());
        }
    }
    "seat config or working directory".into()
}

/// The tracker row from `vissue identity`: version, the root and prefix it
/// resolved, and where the root came from. A root that is relative, missing,
/// or holds no prefix directory fails the row: tickets filed there are
/// invisible to every other seat. When the root is a git checkout with an
/// upstream, the row also names how many commits origin lacks.
pub fn tracker_state(identity: &str, source: &str) -> (String, bool) {
    let version = identity.lines().next().unwrap_or("").trim();
    let field = |key: &str| {
        identity
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let (Some(root), Some(prefix)) = (field("root="), field("prefix=")) else {
        return (format!("{version}; no root in vissue identity"), false);
    };
    let path = std::path::Path::new(root);
    let problem = if !path.is_absolute() {
        Some("relative root: tickets land under the working directory")
    } else if !path.is_dir() {
        Some("root is not a directory")
    } else if !path.join(prefix).is_dir() {
        Some("no prefix directory under the root")
    } else {
        None
    };
    let base = format!("{version} root={root} prefix={prefix} from {source}");
    match problem {
        Some(why) => (format!("{base}; {why}"), false),
        None => match tracker_git_drift(path) {
            Some((extra, git_ok)) => (format!("{base}; {extra}"), git_ok),
            None => (base, true),
        },
    }
}

fn git_in(dir: &Path, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()
}

fn git_ok_stdout(dir: &Path, args: &[&str]) -> Option<String> {
    let o = git_in(dir, args)?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Upstream of the tracker checkout: the configured `@{upstream}`, else
/// `origin/HEAD`. Absent when the root is not a git checkout, or has no
/// remote the doctor can count against.
pub(crate) fn tracker_upstream(root: &Path) -> Option<String> {
    let inside = git_ok_stdout(root, &["rev-parse", "--is-inside-work-tree"])?;
    if inside.trim() != "true" {
        return None;
    }
    if let Some(up) = git_ok_stdout(
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    ) {
        let up = up.trim().to_string();
        if !up.is_empty() {
            return Some(up);
        }
    }
    git_ok_stdout(root, &["rev-parse", "--verify", "origin/HEAD"]).map(|_| "origin/HEAD".into())
}

/// Whether a leftover `tracker-push-<pid>.log` still has that pid running.
fn pid_alive(pid: u32) -> bool {
    // SAFETY: kill with signal 0 only probes existence; it does not deliver.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Newest leftover tracker-push log whose process has exited, and whether
/// any log's process is still running. persist_tracker removes the log on
/// a foreground success and leaves it on a refusal or a background push.
fn tracker_push_logs() -> (bool, Option<(std::time::SystemTime, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(runtime_dir()) else {
        return (false, None);
    };
    let mut running = false;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for ent in entries.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        let Some(rest) = name
            .strip_prefix("tracker-push-")
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        let Ok(pid) = rest.parse::<u32>() else {
            continue;
        };
        if pid_alive(pid) {
            running = true;
            continue;
        }
        let mtime = ent
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let path = ent.path();
        if newest.as_ref().is_none_or(|(t, _)| mtime >= *t) {
            newest = Some((mtime, path));
        }
    }
    (running, newest)
}

fn last_push_refusal() -> Option<String> {
    let path = tracker_push_logs().1?.1;
    let said = std::fs::read(path).ok()?;
    let line = first_line(&said);
    (!line.is_empty()).then_some(line)
}

/// Commits the tracker checkout holds that origin does not. The count is
/// always named. A live background push, or commits younger than the push
/// wait, stay healthy: the sitting already waited that long. Older drift
/// fails the row, and a leftover refused-push log names the reason.
pub fn tracker_git_drift(root: &Path) -> Option<(String, bool)> {
    let up = tracker_upstream(root)?;
    let (mut state, mut ok) = unpushed_drift(root, &up)?;
    if let Some(split) = tracker_remote_split(root, &up) {
        state = format!("{state}; {split}");
        ok = false;
    }
    if let Some(missing) = tracker_merge_driver_missing(root) {
        state = format!("{state}; {missing}");
        ok = false;
    }
    Some((state, ok))
}

/// A tracker whose .gitattributes merges issues.org with vissue, in a clone
/// that has no such driver configured. git then merges the file as text
/// without a word, which is the failure the driver exists to prevent: the
/// attribute travels with the repository, the driver's command does not.
fn tracker_merge_driver_missing(root: &Path) -> Option<String> {
    let top = git_ok_stdout(root, &["rev-parse", "--show-toplevel"])?;
    let attrs = std::fs::read_to_string(Path::new(top.trim()).join(".gitattributes")).ok()?;
    let named = attrs
        .lines()
        .any(|l| l.split_whitespace().any(|w| w == "merge=vissue"));
    if !named {
        return None;
    }
    let driver = git_ok_stdout(root, &["config", "--get", "merge.vissue.driver"]);
    driver.filter(|d| !d.trim().is_empty()).is_none().then(|| {
        ".gitattributes merges issues.org with vissue and this clone has no merge.vissue.driver; \
         `vissue merge-driver --install` in the tracker registers it"
            .to_string()
    })
}

/// The remotes of the tracker whose head of the upstream's branch differs
/// from the upstream's, as of the last fetch. Two seats that push to two
/// remotes of one tracker each read only their own writes, and every other
/// row stays green while they do.
fn tracker_remote_split(root: &Path, up: &str) -> Option<String> {
    let (_, branch) = up.split_once('/')?;
    let refs = git_ok_stdout(
        root,
        &[
            "for-each-ref",
            "--format=%(refname:short) %(objectname)",
            "refs/remotes",
        ],
    )?;
    let heads: Vec<(&str, &str)> = refs
        .lines()
        .filter_map(|l| l.trim().split_once(' '))
        .filter(|(r, _)| r.split_once('/').is_some_and(|(_, b)| b == branch))
        .collect();
    let tip = heads.iter().find(|(r, _)| *r == up)?.1;
    let off: Vec<&str> = heads
        .iter()
        .filter(|(_, o)| *o != tip)
        .map(|(r, _)| *r)
        .collect();
    (!off.is_empty()).then(|| {
        format!(
            "{} differs from {up}; pull and push every remote until they agree",
            off.join(", ")
        )
    })
}

/// The remotes other than the upstream's that carry its branch, as
/// (remote, branch). Names that would need quoting are left out.
pub(crate) fn tracker_mirrors(root: &Path, up: &str) -> Option<Vec<(String, String)>> {
    let (upstream, branch) = up.split_once('/')?;
    let plain = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
    };
    let refs = git_ok_stdout(
        root,
        &["for-each-ref", "--format=%(refname:short)", "refs/remotes"],
    )?;
    Some(
        refs.lines()
            .filter_map(|r| r.trim().split_once('/'))
            .filter(|(r, b)| *r != upstream && *b == branch && plain(r) && plain(b))
            .map(|(r, b)| (r.to_string(), b.to_string()))
            .collect(),
    )
}

fn unpushed_drift(root: &Path, up: &str) -> Option<(String, bool)> {
    let range = format!("{up}..HEAD");
    let count: u64 = git_ok_stdout(root, &["rev-list", "--count", &range])?
        .trim()
        .parse()
        .ok()?;
    if count == 0 {
        return Some(("0 unpushed".into(), true));
    }
    let (running, _) = tracker_push_logs();
    let oldest = git_ok_stdout(root, &["log", "--format=%ct", "--reverse", &range])
        .and_then(|s| {
            s.lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
        })
        .and_then(|s| s.parse::<u64>().ok());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let stuck = oldest.is_some_and(|t| now.saturating_sub(t) >= push_wait().as_secs());
    let unpushed = if count == 1 {
        "1 unpushed".to_string()
    } else {
        format!("{count} unpushed")
    };
    if running {
        return Some((format!("{unpushed}; push still running"), true));
    }
    if let Some(why) = last_push_refusal() {
        return Some((format!("{unpushed}; last push refused: {why}"), false));
    }
    Some((unpushed, !stuck))
}

/// The kernel, its OOM kills since boot, and the ljos-mcp servers this
/// login runs with their resident memory. Fails on any OOM kill: one kill
/// took the encoder, the next the compositor.
fn host_row() -> Habitat {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown kernel".into());
    let kills = oom_kills();
    let (servers, rss_kb) = ljos_mcp_servers();
    let mcp = format!("{servers} ljos-mcp, {} MB resident", rss_kb / 1024);
    match kills {
        Some(0) => Habitat {
            name: "host",
            state: format!("{kernel}; no OOM kills since boot; {mcp}"),
            ok: true,
        },
        Some(n) => Habitat {
            name: "host",
            state: format!(
                "{kernel}; {n} OOM kills since boot (/proc/vmstat oom_kill); {mcp}; \
                 the kernel is killing processes, read `journalctl -k -b` before the load"
            ),
            ok: false,
        },
        None => Habitat {
            name: "host",
            state: format!("{kernel}; {mcp}"),
            ok: true,
        },
    }
}

/// OOM kills since boot, from `/proc/vmstat`; none where it is not.
fn oom_kills() -> Option<u64> {
    parse_oom_kills(&std::fs::read_to_string("/proc/vmstat").ok()?)
}

fn parse_oom_kills(vmstat: &str) -> Option<u64> {
    vmstat
        .lines()
        .find_map(|l| l.strip_prefix("oom_kill "))
        .and_then(|n| n.trim().parse().ok())
}

/// The ljos-mcp processes of this user and their summed resident size in
/// kB, from procfs.
fn ljos_mcp_servers() -> (usize, u64) {
    let uid = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| status_field(&s, "Uid:"));
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return (0, 0);
    };
    let mut count = 0;
    let mut rss = 0;
    for entry in dir.flatten() {
        let path = entry.path();
        if std::fs::read_to_string(path.join("comm")).map_or(true, |c| c.trim() != "ljos-mcp") {
            continue;
        }
        let Ok(status) = std::fs::read_to_string(path.join("status")) else {
            continue;
        };
        if status_field(&status, "Uid:") != uid {
            continue;
        }
        count += 1;
        rss += status_field(&status, "VmRSS:")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
    }
    (count, rss)
}

/// The first number on a `/proc/*/status` line.
fn status_field(status: &str, key: &str) -> Option<String> {
    status
        .lines()
        .find_map(|l| l.strip_prefix(key))
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_string)
}

/// Whether every required habitat answers.
pub fn healthy(rows: &[Habitat]) -> bool {
    rows.iter()
        .all(|h| h.ok || !REQUIRED.contains(&h.name) && h.name != "pack")
}

pub fn format_doctor(rows: &[Habitat]) -> String {
    rows.iter()
        .map(|h| {
            format!(
                "{}	{}	{}
",
                if h.ok { "ok" } else { "no" },
                h.name,
                h.state
            )
        })
        .collect()
}

/// The accessions a satchel's description says it needs.
pub fn needs_of(satchel_json: &str) -> Result<Vec<String>> {
    let v: Value = serde_json::from_str(satchel_json).context("satchel.json")?;
    Ok(v.get("needs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}

/// Deeds to enclose: the satchel's `needs` plus what the pack cites, once each.
pub fn enclose(needs: Vec<String>, cited: &str) -> Vec<String> {
    let mut all: Vec<String> = needs
        .into_iter()
        .chain(cited.lines().map(str::trim).map(str::to_string))
        .filter(|s| !s.is_empty())
        .collect();
    all.sort();
    all.dedup();
    all
}

/// Pack a slice of the seat into `out`: the tracker's satchel, the pack's
/// atoms, the deeds both cite, sealed, and signed when a host key is set.
pub fn handover(out: &Path, projects: &[String], issues: &[String]) -> Result<Vec<String>> {
    if projects.is_empty() && issues.is_empty() {
        bail!("handover: name a project or an issue");
    }
    let mut lines = Vec::new();
    let mut args = vec![
        "satchel".to_string(),
        "--out".into(),
        out.display().to_string(),
    ];
    for p in projects {
        args.push("--project".into());
        args.push(p.clone());
    }
    for i in issues {
        args.push("--issue".into());
        args.push(i.clone());
    }
    lines.push(run_captured("vissue", &args)?.stdout.trim_end().to_string());

    let mut cited = String::new();
    match PacksetClient::from_env() {
        Ok(client) => {
            let atoms_dir = out.join("data").join("atoms");
            match run_captured(
                "packset",
                &[
                    "export",
                    "--into",
                    &atoms_dir.display().to_string(),
                    &client.workspace(),
                ],
            ) {
                Ok(said) => {
                    cited = said.stdout;
                    lines.push(said.stderr.trim_end().to_string());
                }
                Err(e) => lines.push(format!("atoms not enclosed: {e}")),
            }
        }
        Err(_) => lines.push("no pack: PACKSET_URL=off, atoms not enclosed".into()),
    }

    let description = std::fs::read_to_string(out.join("data").join("satchel.json"))
        .context("handover: the satchel has no description")?;
    let deeds = enclose(needs_of(&description)?, &cited);
    if deeds.is_empty() {
        lines.push("no deeds cited".into());
    } else {
        let deeds_dir = out.join("data").join("deeds");
        let said = run_fed(
            "deedar",
            &["export", "--into", &deeds_dir.display().to_string(), "-"],
            &format!(
                "{}
",
                deeds.join(
                    "
"
                )
            ),
        )?;
        lines.push(said.stdout.trim_end().to_string());
    }

    lines.push(
        run_captured("vissue", &["satchel", "--seal", &out.display().to_string()])?
            .stdout
            .trim_end()
            .to_string(),
    );
    // The key deedar signs with is the one doctor reports: the variable, or
    // the seat's own at ~/.config/deedar/host.key. `off` signs nothing.
    if host_key_path().is_some() {
        let manifest = out.join("manifest-sha256.txt");
        let said = run_captured(
            "deedar",
            &["vouch", "sign", &manifest.display().to_string()],
        )?;
        lines.push(said.stdout.trim_end().to_string());
    } else {
        lines.push(
            "unsigned: no host key at ~/.config/deedar/host.key and DEEDAR_HOST_SIGNING_KEY unset; \
             `ljos onboard` writes one"
                .into(),
        );
    }
    Ok(lines)
}

/// Check a satchel that arrived: manifest, deed receipts, signature, and what
/// the atoms hold; with `import`, POST the atoms into this seat's pack.
pub fn receive(dir: &Path, since: Option<&Path>, import: bool) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    lines.push(
        run_captured(
            "vissue",
            &["satchel", "--verify", &dir.display().to_string()],
        )?
        .stdout
        .trim_end()
        .to_string(),
    );
    if dir.join("data").join("deeds").is_dir() {
        let mut args = vec!["check".to_string(), dir.display().to_string()];
        if let Some(bridge) = since {
            args.push("--since".into());
            args.push(bridge.display().to_string());
        }
        lines.push(run_captured("deedar", &args)?.stdout.trim_end().to_string());
    } else {
        lines.push("no deeds enclosed".into());
    }
    let manifest = dir.join("manifest-sha256.txt");
    // Who sent it, for the atoms' provenance: the signing key when the bag
    // is signed, else the fact of a handover. An imported claim then says
    // where it came from, and a search can ask for what one seat taught.
    let mut sender = "from:handover".to_string();
    if manifest.with_extension("txt.sig").is_file() {
        let said = run_captured(
            "deedar",
            &["vouch", "check", &manifest.display().to_string()],
        )?
        .stdout
        .trim_end()
        .to_string();
        if !said.starts_with("signed by ") {
            bail!("receive: satchel is not signed by an accepted key: {said}");
        }
        if let Some(hex) = said
            .strip_prefix("signed by ")
            .and_then(|rest| rest.split(|c: char| !c.is_ascii_hexdigit()).next())
            .filter(|h| h.len() >= 12)
        {
            sender = format!("from:{}", &hex[..12]);
        }
        lines.push(said);
    } else if import {
        bail!("receive: unsigned satchel; will not import");
    } else {
        lines.push("unsigned".into());
    }

    let atoms = enclosed_atoms(dir)?;
    let rows = trust_rows(&atoms);
    lines.push(format!(
        "{} atoms enclosed, {} trust rows",
        atoms.len(),
        rows.len()
    ));
    if import {
        let client = pack()?;
        let workspace = client.workspace();
        let (mut kept, mut refused) = (0usize, Vec::new());
        for atom in &atoms {
            // The atoms arrive stamped with the sender's workspace; they join
            // this seat's, or the import lands in a workspace nobody reads.
            let mut atom = atom.clone();
            if let Some(map) = atom.as_object_mut() {
                map.insert("workspace".into(), Value::String(workspace.clone()));
                let mut entities: Vec<Value> = map
                    .get("entities")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !entities.iter().any(|e| e.as_str() == Some(sender.as_str())) {
                    entities.push(Value::String(sender.clone()));
                }
                map.insert("entities".into(), Value::Array(entities));
            }
            match client.post_atom(&atom) {
                Ok(_) => kept += 1,
                Err(e) => refused.push(e.to_string()),
            }
        }
        lines.push(format!("{kept} atoms imported, {} refused", refused.len()));
        lines.extend(refused.into_iter().take(5));
        if kept > 0 {
            lines.push(
                "imported claims may rewrite held ones; `ljos consolidate` reports the pairs, `--apply` closes them"
                    .to_string(),
            );
        }
    }
    Ok(lines)
}

/// Every atom in a satchel's `data/atoms/*.jsonl`.
pub fn enclosed_atoms(dir: &Path) -> Result<Vec<Value>> {
    let atoms_dir = dir.join("data").join("atoms");
    let Ok(entries) = std::fs::read_dir(&atoms_dir) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let text = std::fs::read_to_string(entry.path())?;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            out.push(
                serde_json::from_str(line).with_context(|| entry.path().display().to_string())?,
            );
        }
    }
    Ok(out)
}

/// Kinds that are weighed, not recalled, and so never come up for review.
const UNREVIEWED_KINDS: &[&str] = &["trust", "persona", "playbook"];

/// Whether an atom is a claim the review clock should hold at all.
fn reviewable(a: &Value) -> bool {
    !UNREVIEWED_KINDS.contains(&a.get("kind").and_then(Value::as_str).unwrap_or(""))
}

/// The live atoms whose review is due at `now` (RFC 3339 UTC), soonest first.
/// A claim that has never entered the review clock has no `due_at`; it is
/// due now, and grading it puts it on the clock. Trust and persona rows are
/// weighed, not recalled, and never come up.
pub fn due_of(atoms: &[Value], now: &str) -> Vec<Value> {
    let mut due: Vec<Value> = atoms
        .iter()
        .filter(|a| reviewable(a))
        .filter(|a| {
            a.get("due_at")
                .and_then(Value::as_str)
                .is_none_or(|d| d.is_empty() || d <= now)
        })
        .cloned()
        .collect();
    due.sort_by(|a, b| {
        a["due_at"]
            .as_str()
            .unwrap_or("")
            .cmp(b["due_at"].as_str().unwrap_or(""))
    });
    due
}

/// One line on the state of the review clock: how many are due, how many
/// are scheduled, and when the next one comes up. An empty `due` with a
/// next date is a clock that is running; an empty `due` with nothing
/// scheduled is a seat that has remembered nothing.
pub fn review_summary(atoms: &[Value], now: &str) -> String {
    let due = due_of(atoms, now).len();
    let mut later: Vec<&str> = atoms
        .iter()
        .filter(|a| reviewable(a))
        .filter_map(|a| a.get("due_at").and_then(Value::as_str))
        .filter(|d| !d.is_empty() && *d > now)
        .collect();
    later.sort_unstable();
    match later.first() {
        Some(next) => format!("{due} due; {} scheduled, next at {next}", later.len()),
        None if due == 0 => "0 due; nothing scheduled: this seat has remembered nothing yet".into(),
        None => format!("{due} due; nothing else scheduled"),
    }
}

/// The due claims with the island's first, keeping each group's due
/// order: the claims a sitting's work bears on are the ones its agent can
/// grade from what it is about to read, rather than the oldest in the pack.
#[must_use]
pub fn due_on_island_first(due: Vec<Value>, island: &Value) -> Vec<Value> {
    // A weak island is the pack's best-connected cluster, not the issue's.
    if island["weak"].as_bool().unwrap_or(false) {
        return due;
    }
    let on: std::collections::BTreeSet<&str> = island["island"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["id"].as_str())
        .collect();
    let (mut first, rest): (Vec<Value>, Vec<Value>) = due
        .into_iter()
        .partition(|a| a["id"].as_str().is_some_and(|id| on.contains(id)));
    first.extend(rest);
    first
}

/// How many due rows a sitting prints before the summary line.
pub const SITTING_DUE: usize = 8;

/// How many dated events a sitting's timeline prints. Protocol: last twelve.
pub const SITTING_TIMELINE: usize = 12;

/// The review clock as a sitting prints it: a short prefix, then the summary.
pub fn sitting_due_report(island: &Value) -> Result<String> {
    let client = pack()?;
    // The same sweep `ljos due` runs. A sitting is the clock's ordinary
    // opening; a review left due past twice its interval lapses here.
    let swept = client.sweep(&client.workspace()).ok();
    let atoms = atoms_lean(&client, &client.workspace()).context("due: GET /v1/atoms failed")?;
    let now = now_utc();
    let due = due_on_island_first(due_of(&atoms, &now), island);
    let shown = due.len().min(SITTING_DUE);
    Ok(format!(
        "{}{}{}\n",
        format_due(&due[..shown]),
        review_summary(&atoms, &now),
        format_sweep(swept.as_ref())
    ))
}

/// The review clock as `ljos due` prints it: the due atoms, then the summary.
pub fn due_report() -> Result<String> {
    let client = pack()?;
    // The sweep runs first, so a review left due past twice its interval is
    // lapsed or forgotten before the list is read, and the report says so.
    let swept = client.sweep(&client.workspace()).ok();
    let atoms = atoms_lean(&client, &client.workspace()).context("due: GET /v1/atoms failed")?;
    let now = now_utc();
    Ok(format!(
        "{}{}{}\n",
        format_due(&due_of(&atoms, &now)),
        review_summary(&atoms, &now),
        format_sweep(swept.as_ref())
    ))
}

/// One line on what the sweep did, or nothing when it found nothing.
pub fn format_sweep(report: Option<&Value>) -> String {
    let Some(report) = report else {
        return String::new();
    };
    let lapsed = report.get("lapsed").and_then(Value::as_u64).unwrap_or(0);
    let forgotten = report.get("forgotten").and_then(Value::as_u64).unwrap_or(0);
    if lapsed == 0 && forgotten == 0 {
        return String::new();
    }
    format!(
        "\nswept: {lapsed} review{} lapsed past twice {} interval, {forgotten} never-recalled claim{} forgotten by neglect",
        if lapsed == 1 { "" } else { "s" },
        if lapsed == 1 { "its" } else { "their" },
        if forgotten == 1 { "" } else { "s" }
    )
}

/// What the pack holds for review now.
pub fn due() -> Result<Vec<Value>> {
    let client = pack()?;
    let atoms = atoms_lean(&client, &client.workspace()).context("due: GET /v1/atoms failed")?;
    Ok(due_of(&atoms, &now_utc()))
}

/// The soonest [`SITTING_DUE`] claims, how many are due in all, and the
/// clock line. Read-only: the sweep stays on `ljos due` and on a sitting.
pub fn due_page() -> Result<(Vec<Value>, usize, String)> {
    let client = pack()?;
    let atoms = atoms_lean(&client, &client.workspace()).context("due: GET /v1/atoms failed")?;
    let now = now_utc();
    let all = due_of(&atoms, &now);
    let total = all.len();
    let shown: Vec<Value> = all.into_iter().take(SITTING_DUE).collect();
    Ok((shown, total, review_summary(&atoms, &now)))
}

// ---- habits ----------------------------------------------------------------

/// The entity a habit's readings carry, so a name finds them.
pub const HABIT_ENTITY: &str = "habit:";
/// A habit's cadence when none is given: a week, in seconds.
pub const HABIT_EVERY_S: i64 = 7 * 86_400;

/// One reading of a habit: a number the seat keeps measuring, with the
/// cadence it is measured at. A reading is a claim of kind `habit` that
/// supersedes the reading before it, so the pack holds one live value a
/// habit and `search --as-of` still answers what it stood at then; its
/// review clock is the cadence, so `due` and the hook say when the next
/// reading is late.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Reading {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub source: String,
    /// Seconds between readings.
    pub every_s: i64,
    /// The reading before this one, when there was one.
    pub was: Option<f64>,
    pub was_ts: Option<String>,
    pub id: Option<String>,
    pub ts: Option<String>,
    pub due_at: Option<String>,
}

/// `7d`, `24h`, `2w`, `30m`, or bare seconds.
pub fn parse_every(text: &str) -> Result<i64> {
    let t = text.trim();
    let split = t.trim_end_matches(|c: char| c.is_ascii_alphabetic()).len();
    let (num, unit) = t.split_at(split);
    let n: i64 = num
        .trim()
        .parse()
        .with_context(|| format!("habit: --every {t:?} is not a span; write 7d, 24h, 2w or 30m"))?;
    let each = match unit {
        "" | "s" => 1,
        "m" => 60,
        "h" => 3_600,
        "d" => 86_400,
        "w" => 7 * 86_400,
        other => bail!("habit: unknown unit {other:?} in --every; write d, h, w, m or s"),
    };
    if n <= 0 {
        bail!("habit: --every must be positive");
    }
    Ok(n * each)
}

/// An RFC 3339 stamp `secs` after `now` (`YYYY-MM-DDTHH:MM:SSZ`, to the
/// second). None when `now` does not read as a stamp.
fn stamp_after(now: &str, secs: i64) -> Option<String> {
    let days = days_of_stamp(Some(now))?;
    let clock = now.get(11..19)?;
    let mut it = clock.split(':');
    let h: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let s: i64 = it.next()?.parse().ok()?;
    let total = days * 86_400 + h * 3_600 + m * 60 + s + secs;
    let day = total.div_euclid(86_400);
    let rem = total.rem_euclid(86_400);
    Some(format!(
        "{}T{:02}:{:02}:{:02}.000Z",
        civil_of_days(day),
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    ))
}

/// A number as a person writes it: up to four decimals, no trailing zeros.
#[must_use]
pub fn trim_num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// The claim a reading is stored as. The words are for a reader; the
/// numbers travel in the atom's `habit` field.
#[must_use]
pub fn habit_text(name: &str, value: f64, unit: &str, source: &str) -> String {
    let unit = unit.trim();
    let source = source.trim();
    let mut text = format!("habit {} stands at {}", name.trim(), trim_num(value));
    if !unit.is_empty() {
        text.push(' ');
        text.push_str(unit);
    }
    if !source.is_empty() {
        text.push_str(&format!(" ({source})"));
    }
    text.push('.');
    text
}

fn reading_of(atom: &Value) -> Option<Reading> {
    if atom.get("kind").and_then(Value::as_str) != Some("habit") {
        return None;
    }
    let h = atom.get("habit")?;
    Some(Reading {
        name: h.get("name")?.as_str()?.to_string(),
        value: h.get("value")?.as_f64()?,
        unit: h
            .get("unit")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        source: h
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        every_s: h
            .get("every_s")
            .and_then(Value::as_i64)
            .unwrap_or(HABIT_EVERY_S),
        was: h.get("was").and_then(Value::as_f64),
        was_ts: h.get("was_ts").and_then(Value::as_str).map(str::to_string),
        id: atom.get("id").and_then(Value::as_str).map(str::to_string),
        ts: atom.get("ts").and_then(Value::as_str).map(str::to_string),
        due_at: atom
            .get("due_at")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// The live readings among `atoms`, one a habit, by name.
#[must_use]
pub fn readings_of(atoms: &[Value]) -> Vec<Reading> {
    let mut rows: Vec<Reading> = atoms.iter().filter_map(reading_of).collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name).then(b.ts.cmp(&a.ts)));
    rows.dedup_by(|a, b| a.name == b.name);
    rows
}

/// The live readings in the seat's pack.
pub fn habits() -> Result<Vec<Reading>> {
    let client = pack()?;
    let atoms = atoms_lean(&client, &client.workspace()).context("habit: GET /v1/atoms failed")?;
    Ok(readings_of(&atoms))
}

/// Take a reading: write it as a claim that supersedes the habit's earlier
/// reading, carrying that reading as `was`, with its review due one
/// cadence from now. Returns the pack's answer and the reading it closed.
pub fn habit(
    name: &str,
    value: f64,
    unit: &str,
    every_s: i64,
    source: &str,
) -> Result<(Value, Option<Reading>)> {
    let name = name.trim();
    if name.is_empty() {
        bail!("habit: a reading needs a name");
    }
    if !value.is_finite() {
        bail!("habit: {value} is not a reading");
    }
    let client = pack()?;
    let workspace = client.workspace();
    let atoms = atoms_lean(&client, &workspace).context("habit: GET /v1/atoms failed")?;
    let prev = readings_of(&atoms).into_iter().find(|r| r.name == name);
    let now = now_utc();
    let mut atom = atom_body("habit", &habit_text(name, value, unit, source), &workspace);
    add_entities(&mut atom, [format!("{HABIT_ENTITY}{name}")]);
    if let Some(due) = stamp_after(&now, every_s) {
        atom["due_at"] = Value::String(due);
    }
    atom["habit"] = serde_json::json!({
        "name": name,
        "value": value,
        "unit": unit.trim(),
        "source": source.trim(),
        "every_s": every_s,
        "was": prev.as_ref().map(|p| p.value),
        "was_ts": prev.as_ref().and_then(|p| p.ts.clone()),
    });
    if let Some(id) = prev.as_ref().and_then(|p| p.id.clone()) {
        atom["supersedes"] = Value::Array(vec![Value::String(id)]);
    }
    let body = client
        .post_atom(&atom)
        .context("habit: POST /v1/atoms failed")?;
    Ok((body, prev))
}

/// The change since the reading before, signed, or nothing for a first
/// reading.
#[must_use]
pub fn format_change(r: &Reading, now: &str) -> String {
    match r.was {
        Some(was) => {
            let d = r.value - was;
            let sign = if d >= 0.0 { "+" } else { "" };
            format!(
                "{sign}{} since {} ({})",
                trim_num(d),
                trim_num(was),
                age_of(r.was_ts.as_deref(), now)
            )
        }
        None => "first reading".to_string(),
    }
}

/// `ljos habit`: one line a habit: name, value with unit, the change since
/// the last reading, the age of this one, when the next is due, source.
#[must_use]
pub fn format_readings(rows: &[Reading], now: &str) -> String {
    rows.iter()
        .map(|r| {
            let due = match r.due_at.as_deref() {
                Some(d) if d <= now => format!("next reading late ({})", age_of(Some(d), now)),
                Some(d) => format!("next reading {}", age_of(Some(d), now)),
                None => "no cadence".to_string(),
            };
            format!(
                "{}\t{}{}{}\t{}\t{}\t{}\t{}\n",
                r.name,
                trim_num(r.value),
                if r.unit.is_empty() { "" } else { " " },
                r.unit,
                format_change(r, now),
                age_of(r.ts.as_deref(), now),
                due,
                r.source
            )
        })
        .collect()
}

pub fn format_due(atoms: &[Value]) -> String {
    atoms
        .iter()
        .map(|a| {
            format!(
                "{}	{}	{}	{}
",
                a["due_at"]
                    .as_str()
                    .filter(|d| !d.is_empty())
                    .unwrap_or("unreviewed"),
                a["kind"].as_str().unwrap_or(""),
                a["id"].as_str().unwrap_or("-"),
                a["text"].as_str().unwrap_or("")
            )
        })
        .collect()
}

/// Grade one review: recalled moves the atom out, lapsed brings it back sooner.
pub fn graded(id: &str, recalled: bool) -> Result<Value> {
    let id = id.trim();
    if id.is_empty() {
        bail!("graded: an atom id is required");
    }
    let client = pack()?;
    client
        .grade(&client.workspace(), id, recalled)
        .with_context(|| format!("graded: POST /v1/grade failed for {id}"))
}

/// Now, RFC 3339 UTC to the second, the stamp the pack writes.
#[must_use]
pub fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    // Civil date from days since the epoch (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Run a habitat's verb with `input` on stdin.
pub fn run_fed(bin: &str, args: &[impl AsRef<str>], input: &str) -> Result<Said> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let path = which::which(bin).with_context(|| format!("{bin} not on PATH"))?;
    let mut cmd = Command::new(path);
    for a in args {
        cmd.arg(a.as_ref());
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("{bin}: could not start"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        let why = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        bail!("{bin} exited {}: {why}", out.status);
    }
    Ok(Said { stdout, stderr })
}

/// A claimdag id for a name: the name itself when it is already 32 hex, else
/// FNV-1a 128 of it. One tracker id maps to one node; one assignee to one actor.
pub fn work_id(name: &str) -> String {
    let name = name.trim();
    if name.len() == 32 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
        return name.to_ascii_lowercase();
    }
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut h = OFFSET;
    for b in name.bytes() {
        h ^= u128::from(b);
        h = h.wrapping_mul(PRIME);
    }
    format!("{h:032x}")
}

/// The claimdag node standing for `issue`, minted with the tracker id as its
/// summary when the graph does not hold it yet.
pub fn node_for(issue: &str) -> Result<String> {
    let id = work_id(issue);
    if id != issue.trim() && run_captured("claimdag", &["get", &id]).is_err() {
        run_captured(
            "claimdag",
            &["upsert", "--id", &id, "--summary", issue.trim()],
        )
        .with_context(|| format!("claim: could not mint a node for {issue}"))?;
    }
    Ok(id)
}

/// The memories a task activates: the pack's island around the cue. With
/// `fire`, the strongest of them fire together and their links gain weight.
pub fn packset_island(cue: &str, fire: bool) -> Result<Value> {
    packset_island_as(cue, fire, None)
}

/// [`packset_island`] through a persona's lens: the spread follows the
/// weights that persona fired, and a fire writes its weights and not the
/// seat's. The seat's own island is the one with no lens.
pub fn packset_island_as(cue: &str, fire: bool, lens: Option<&str>) -> Result<Value> {
    let cue = cue.trim();
    if cue.is_empty() {
        bail!("island: pass the task or question at hand");
    }
    let client = pack()?;
    let workspace = client.workspace();
    let lens = lens
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_lowercase);
    let mut body = client
        .activate_as(&workspace, cue, 24, fire, lens.as_deref())
        .context("island: GET /v1/activate failed")?;
    if body["fired"].as_u64().unwrap_or(0) > 0 {
        match record_fire(cue, lens.as_deref(), &body) {
            Ok(id) => body["trace"] = Value::String(id),
            Err(err) => body["trace_error"] = Value::String(err.to_string()),
        }
    }
    Ok(body)
}

/// Record a fire as why-provenance: which links were strengthened, under
/// whose weights. A trace does not replace another trace.
fn record_fire(cue: &str, lens: Option<&str>, body: &Value) -> Result<String> {
    let fired = body["fired"].as_u64().unwrap_or(0);
    let who = lens.unwrap_or("seat");
    let ids: Vec<String> = body["island"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_string))
        .take(8)
        .collect();
    let mut nonce = 0xcbf29ce484222325u64;
    for part in [cue, who].into_iter().chain(ids.iter().map(String::as_str)) {
        for byte in part.as_bytes() {
            nonce ^= u64::from(*byte);
            nonce = nonce.wrapping_mul(0x100000001b3);
        }
    }
    let text = format!(
        "Fire {:08x} under {who} strengthened {fired} links.",
        nonce as u32
    );
    let client = pack()?;
    let workspace = client.workspace();
    let mut atom = atom_body("trace", &text, &workspace);
    add_entities(&mut atom, ids);
    let posted = client
        .post_atom(&atom)
        .context("trace: POST /v1/atoms failed")?;
    Ok(posted
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string())
}

/// The claims the pack's link graph turns on, highest first: what matters
/// in this seat's memory by its own connections, before any query.
pub fn packset_hubs(limit: usize) -> Result<Value> {
    let client = pack()?;
    let workspace = client.workspace();
    client
        .hubs(&workspace, limit)
        .context("hubs: GET /v1/hubs failed")
}

/// Consolidate the seat's memory: every claim that replaces an earlier
/// one (a rewrite, a new object under the same head, a correction, an
/// explicit supersedes) closes the earlier one's window and names it.
/// Candidate contradictions from the geometry of the seat's memory: the
/// `landscape` binary reads the pack's embeddings at the point scale and
/// prints the lowest passes between single memories, which on a record of
/// planted contradictions were the contradictions nine times in ten. The
/// replacement rule reads words; this reads distance, in any language.
/// A candidate is for a person or `consolidate` to judge; nothing is
/// written here. `landscape` is an optional habitat: absent, this says so.
///
/// # Errors
///
/// The binary absent or refusing, or the pack not answering.
pub fn conflicts(limit: usize) -> Result<String> {
    if which::which("landscape").is_err() {
        bail!(
            "conflicts: `landscape` is not on PATH; it is the optional habitat that reads the pack's geometry (leidarljos/landscape)"
        );
    }
    let client = pack()?;
    let said = match run_captured(
        "landscape",
        &[
            "--atoms",
            client.base(),
            "--workspace",
            &client.workspace(),
            "--conflicts",
        ],
    ) {
        Ok(said) => said,
        // A pack whose memories carry no embeddings has no landscape to
        // read; that is a fact about the pack, not a refusal.
        Err(e) if e.to_string().contains("at least two") => {
            return Ok(
                "fewer than two memories with embeddings in the pack; conflicts by geometry need the encoder (`packset doctor` shows it)\n"
                    .to_string(),
            );
        }
        Err(e) => return Err(e),
    };
    let v: Value =
        serde_json::from_str(&said.stdout).context("conflicts: landscape printed no JSON")?;
    let now = now_utc();
    let atoms = atoms_lean(&client, &client.workspace()).unwrap_or_default();
    let stamp_of = |id: &str| -> Option<String> {
        atoms
            .iter()
            .find(|a| a["id"].as_str() == Some(id))
            .and_then(|a| a["ts"].as_str().map(str::to_string))
    };
    // Trust rows, personas, forecasts and rules are weighed, not recalled;
    // a pass between two of them is not a contradiction to judge.
    let recalled = |id: &str| -> bool {
        atoms
            .iter()
            .find(|a| a["id"].as_str() == Some(id))
            .is_none_or(reviewable)
    };
    let mut out = String::new();
    for pair in v["pairs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| {
            recalled(p["a"].as_str().unwrap_or("")) && recalled(p["b"].as_str().unwrap_or(""))
        })
        .take(limit)
    {
        let a = pair["a"].as_str().unwrap_or("-");
        let b = pair["b"].as_str().unwrap_or("-");
        out.push_str(&format!(
            "pass {:.3}\n  {a} {}  {}\n  {b} {}  {}\n",
            pair["barrier"].as_f64().unwrap_or(0.0),
            age_of(stamp_of(a).as_deref(), &now),
            pair["a_text"].as_str().unwrap_or("").trim(),
            age_of(stamp_of(b).as_deref(), &now),
            pair["b_text"].as_str().unwrap_or("").trim()
        ));
    }
    let n = v["pairs"].as_array().map_or(0, Vec::len);
    out.push_str(&format!(
        "{n} passes between single memories at kernel width {:.3}; the lowest are the likeliest contradictions. `ljos forget ID --why DEED` retires one, `ljos remember` a rewrite closes it.\n",
        v["sigma"].as_f64().unwrap_or(0.0)
    ));
    Ok(out)
}

/// The rule a write applies on arrival, run over what the pack already
/// holds. Without `apply` nothing is written; the pairs are reported.
pub fn packset_consolidate(apply: bool) -> Result<Value> {
    let client = pack()?;
    let workspace = client.workspace();
    client
        .consolidate(&workspace, apply)
        .context("consolidate: POST /v1/consolidate failed")
}

/// The pairs a consolidation closed or would close, one a line, then the
/// count and whether it was applied.
pub fn format_consolidation(body: &Value) -> String {
    let mut out = String::new();
    for pair in body["pairs"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "closes {}  {}\n    for {}  {}\n",
            pair["old"].as_str().unwrap_or("-"),
            pair["old_text"].as_str().unwrap_or("").trim(),
            pair["new"].as_str().unwrap_or("-"),
            pair["new_text"].as_str().unwrap_or("").trim()
        ));
    }
    let closed = body["closed"].as_u64().unwrap_or(0);
    let live = body["live"].as_u64().unwrap_or(0);
    if body["applied"].as_bool().unwrap_or(false) {
        out.push_str(&format!("{closed} of {live} live memories closed\n"));
    } else {
        out.push_str(&format!(
            "{closed} of {live} live memories would close; `ljos consolidate --apply` closes them\n"
        ));
    }
    out
}

/// One line per hub: score, links, id, text.
pub fn format_hubs(body: &Value) -> String {
    let mut out = String::new();
    for hub in body["hubs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| reviewable(a))
    {
        out.push_str(&format!(
            "{:.4}\t{}\t{}\t{}\n",
            hub["score"].as_f64().unwrap_or(0.0),
            hub["links"].as_u64().unwrap_or(0),
            hub["id"].as_str().unwrap_or("-"),
            hub["text"].as_str().unwrap_or("")
        ));
    }
    out
}

/// What an activation number is, and whether this call rewrote weights.
///
/// The number on a row is spread from the search seeds along the pack's
/// links. It is not a relevance rank. `fire` strengthens the links of the
/// strongest rows under the lens that walked them, so the next walk of the
/// same cue follows those links. A weak island does not fire.
#[must_use]
pub fn island_reading(body: &Value) -> String {
    let lens = body["as"].as_str().unwrap_or("").trim();
    let fired = body["fired"].as_u64().unwrap_or(0);
    let held = body["held"].as_bool().unwrap_or(false);
    let weak = body["weak"].as_bool().unwrap_or(false);
    let rows = body["island"].as_array().is_some_and(|a| !a.is_empty());
    if !rows && !weak && fired == 0 && !held && lens.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    if lens.is_empty() {
        out.push_str(
            "Seat island. Activation is spread from search seeds along links. It is not a relevance rank.\n",
        );
    } else {
        out.push_str(&format!(
            "Persona {lens} island. The spread follows the weights that persona fired, not the seat's. It is not a relevance rank.\n"
        ));
    }
    if weak {
        out.push_str(
            "Not fired: fewer than two seeds that two scorers agreed on, so firing would wire the wrong links.\n",
        );
    } else if held {
        out.push_str(
            "Not fired: this cue already fired inside the hour, so the weights were left as they were.\n",
        );
    } else if fired > 0 {
        let who = if lens.is_empty() { "the seat" } else { lens };
        out.push_str(&format!(
            "Fired: {fired} links gained weight under {who}. The next walk of this cue follows those links. Fire only after the island was used.\n"
        ));
        if let Some(id) = body["trace"].as_str().filter(|s| !s.is_empty()) {
            out.push_str(&format!(
                "Recorded as trace {id}: the links this fire strengthened.\n"
            ));
        } else if let Some(err) = body["trace_error"].as_str() {
            out.push_str(&format!("The fire was not recorded: {err}\n"));
        }
    } else {
        out.push_str(
            "Not fired. Pass fire after the island is used, so the links that served gain weight. Firing on the first look wires whatever the spread touched.\n",
        );
    }
    out
}

/// One line per activated memory: activation, seed mark, id, text.
pub fn format_island(body: &Value) -> String {
    let mut out = island_reading(body);
    let now = now_utc();
    if body["weak"].as_bool().unwrap_or(false) {
        out.push_str(&format!(
            "weak island: {} seed{} two scorers agreed on{}; read it as the pack's best-connected cluster, not as what the cue is about; it will not fire\n",
            body["agreed_seeds"].as_u64().unwrap_or(0),
            if body["agreed_seeds"].as_u64().unwrap_or(0) == 1 { "" } else { "s" },
            if body["dense"].as_bool().unwrap_or(true) { "" } else { "; the encoder is down, ranking is lexical only" }
        ));
    }
    for atom in body["island"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| reviewable(a))
    {
        out.push_str(&format!(
            "{:.3}\t{}\t{}\t{}\t{}\n",
            atom["activation"].as_f64().unwrap_or(0.0),
            if atom["seed"].as_bool().unwrap_or(false) {
                "seed"
            } else {
                "    "
            },
            atom["id"].as_str().unwrap_or("-"),
            age_of(atom["ts"].as_str(), &now),
            atom["text"].as_str().unwrap_or("")
        ));
    }
    out
}

pub fn packset_search(query: &str) -> Result<Vec<Hit>> {
    packset_search_opts(query, 10, false)
}

/// [`packset_search`] with a limit and the cross-encoder rerank: the
/// writer scores the top hits against the query with its reranker, which
/// costs a model call and buys precision. For a brief or a person reading,
/// not for the hook.
pub fn packset_search_opts(query: &str, limit: u32, rerank: bool) -> Result<Vec<Hit>> {
    packset_search_as_of(query, limit, None, rerank)
}

/// [`packset_search_opts`] asked of the pack as it stood at `as_of` (RFC
/// 3339; a date alone reads as its start): only memories live then answer,
/// what was withdrawn since included and what was learnt since left out.
/// `None` is now. This is the question "what did the seat know when it
/// decided that", and the pack keeps every record so it can be asked.
pub fn packset_search_as_of(
    query: &str,
    limit: u32,
    as_of: Option<&str>,
    rerank: bool,
) -> Result<Vec<Hit>> {
    let q = query.trim();
    if q.is_empty() {
        bail!("search: empty query");
    }
    let as_of = as_of.map(str::trim).filter(|s| !s.is_empty());
    let stamp = match as_of {
        Some(at) if days_of_stamp(Some(at)).is_none() => {
            bail!("search: --as-of {at:?} is not a date; write YYYY-MM-DD or RFC 3339")
        }
        // A date alone is its start; the pack wants the instant spelt out.
        Some(at) if at.len() == 10 => Some(format!("{at}T00:00:00.000Z")),
        Some(at) => Some(at.to_string()),
        None => None,
    };
    with_writer(|| {
        let client = pack()?;
        let workspace = client.workspace();
        client
            .search_opts(&workspace, q, limit, stamp.as_deref(), rerank)
            .context("search: GET /v1/search failed")
    })
}

/// The actor id in a `claimdag get` line (`assignee=HEX`), if any.
/// The live generation on a `claimdag get` line: the `gen=N` field.
fn gen_of(get_output: &str) -> Option<u64> {
    get_output
        .split_whitespace()
        .find_map(|w| w.strip_prefix("gen="))
        .and_then(|g| g.parse().ok())
}

/// The generation a finish or complete acts on: the one given, else the live
/// one read off the claim graph, so a sitting need not carry a number the
/// graph already holds. A stale explicit gen is still refused by the graph.
fn live_gen(id: &str, gen: Option<u64>) -> Result<u64> {
    if let Some(g) = gen {
        return Ok(g);
    }
    let got = run_captured("claimdag", &["get", id])?.stdout;
    gen_of(&got).ok_or_else(|| {
        anyhow::anyhow!("complete: no generation on the claim graph's line for {id}: {got}")
    })
}

/// Refusal when another conversation holds the node: names that holder
/// and still says `held by another`, so a concurrent sitting can match it.
#[must_use]
pub fn held_by_another_message(node: &str, assignee: &str, hold: &Hold, running: &str) -> String {
    format!(
        "claim: {node} is held by another ({}, seat {}, {running}, since {}), not by {assignee} (this one). That conversation frees it with `ljos release {node}` or `ljos complete {node} --gen` from its sitting; when it is gone, `ljos release {node} --assignee {}` releases it under the name it held",
        hold.assignee,
        hold.seat,
        hold.since,
        hold.assignee
    )
}

fn holder_of(get_output: &str) -> Option<String> {
    get_output
        .split_whitespace()
        .find_map(|w| w.strip_prefix("assignee="))
        .filter(|h| h.len() == 32 && *h != "00000000000000000000000000000000")
        .map(str::to_string)
}

/// Stamp the tracker to match the claim graph. The claim graph holds
/// occupancy; the tracker answers who holds what, and a sitting that takes
/// one without the other leaves `vissue claims` blind to a held issue.
/// `vissue claim ISSUE` moves the issue to STARTED under `assignee` and is
/// idempotent for the name that already holds it. A node the tracker does
/// not know (a raw claim-graph id) has nothing to stamp and gives `None`.
///
/// # Errors
///
/// The tracker refusing the name. The claim graph already holds the node
/// by then, so the message names the verb that frees it.
fn tracker_claim_needs_force(text: &str) -> bool {
    text.contains("pass --force") || text.contains("claimed by")
}

fn stamp_tracker_claim(node: &str, assignee: &str, force: bool) -> Result<Said> {
    if force {
        run_captured_as("vissue", &["claim", node, "--force"], Some(assignee))
    } else {
        run_captured_as("vissue", &["claim", node], Some(assignee))
    }
}

fn stamp_tracker(node: &str, assignee: &str) -> Result<Option<String>> {
    if run_captured("vissue", &["show", node, "--json"]).is_err() {
        return Ok(None);
    }
    let claimed = match stamp_tracker_claim(node, assignee, false) {
        Ok(said) => Ok(said),
        Err(e) => {
            let text = e.to_string();
            // A new sitting on work the tracker already closed: reopen the
            // heading to STARTED, then stamp occupancy. The claim graph
            // already took the node.
            let after_reopen = if text.contains("already DONE")
                || text.contains("already CANCELLED")
            {
                run_captured("vissue", &["update", node, "-s", "STARTED"]).with_context(|| {
                    format!(
                        "claim: the claim graph took {node} but the tracker would not reopen {node} to STARTED under {assignee}"
                    )
                })?;
                stamp_tracker_claim(node, assignee, false)
            } else {
                Err(e)
            };
            match after_reopen {
                Ok(said) => Ok(said),
                Err(e2) if tracker_claim_needs_force(&e2.to_string()) => {
                    stamp_tracker_claim(node, assignee, true)
                }
                Err(e2) => Err(e2),
            }
        }
    };
    claimed
        .map(|_| Some(format!("tracker: {node} STARTED under {assignee}")))
        .with_context(|| {
            format!(
                "claim: the claim graph took {node} but the tracker refused to stamp it under {assignee}; `ljos release {node} --assignee {assignee}` frees the graph, or `vissue claim {node} --force` takes the tracker over"
            )
        })
}

/// What the claim graph said, followed by the tracker's line when the node
/// is an issue.
fn with_tracker(said: String, node: &str, assignee: &str) -> Result<String> {
    let mut out = said;
    if let Some(line) = stamp_tracker(node, assignee)? {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

/// Take a session node, and when the claim graph refuses because the
/// assignee still holds another node, say which tracker id that is and the
/// two verbs that free it. The bare refusal names a 32-hex id nobody can
/// act on.
///
/// # Errors
///
/// The refusal, explained, or any other failure of the claim graph.
pub fn claim(node: &str, assignee: &str) -> Result<String> {
    let id = node_for(node)?;
    let actor = work_id(&occupancy_scope(assignee, node));
    match run_captured("claimdag", &["claim", &id, "--assignee", &actor]) {
        Ok(said) => {
            write_hold(&actor, assignee);
            with_tracker(said.stdout, node, assignee)
        }
        Err(e) => {
            let text = e.to_string();
            // A tracker id maps to one node. When an earlier sitting finished
            // it, this is a new sitting on the same work: reopen, then claim.
            if ["status done", "status failed", "status cancelled"]
                .iter()
                .any(|s| text.contains(s))
            {
                run_captured("claimdag", &["reopen", &id, "--actor", &actor])?;
                let said = run_captured("claimdag", &["claim", &id, "--assignee", &actor])?;
                write_hold(&actor, assignee);
                return with_tracker(
                    format!("reopened a finished session node\n{}", said.stdout),
                    node,
                    assignee,
                );
            }
            // The node is already claimed. By this name it is a sitting
            // resumed: renew the lease and go on. By another it is theirs.
            if text.contains("status claimed") {
                let got = run_captured("claimdag", &["get", &id])?.stdout;
                return match holder_of(&got) {
                    Some(holder) if holder == actor => {
                        let renewed = run_captured("claimdag", &["renew", &id, "--actor", &actor])
                            .map(|s| s.stdout)
                            .unwrap_or_default();
                        write_hold(&actor, assignee);
                        with_tracker(
                            format!("already held by {assignee}; the sitting resumes\n{renewed}"),
                            node,
                            assignee,
                        )
                    }
                    Some(holder) => match read_hold(&holder) {
                        // This seat's own conversation, and it is gone: a
                        // runner that exited without finishing. The seat
                        // owns its conversations, so the sitting takes the
                        // node over rather than waiting on nobody.
                        Some(h) if h.seat == seat_name() && !hold_alive(&h) => {
                            run_captured("claimdag", &["release", &id, "--actor", &holder])?;
                            drop_hold(&holder);
                            let said =
                                run_captured("claimdag", &["claim", &id, "--assignee", &actor])?;
                            write_hold(&actor, assignee);
                            with_tracker(
                                format!(
                                    "took over from {}, this seat's conversation, gone (held since {})\n{}",
                                    h.assignee, h.since, said.stdout
                                ),
                                node,
                                assignee,
                            )
                        }
                        Some(h) => bail!(
                            "{}",
                            held_by_another_message(
                                node,
                                assignee,
                                &h,
                                if hold_alive(&h) {
                                    "still running"
                                } else {
                                    "its runner is gone"
                                }
                            )
                        ),
                        None => bail!(
                            "claim: {node} is held by another conversation, not by {assignee} (this one; `ljos seat` says where the name came from), and no record on this host names it. That conversation frees it with `ljos release {node}` or `ljos complete {node} --gen` from its sitting; a conversation that is gone is released with `ljos release {node} --assignee NAME` under the name it held"
                        ),
                    },
                    None => Err(e),
                };
            }
            if !text.contains("assignee busy") {
                return Err(e);
            }
            let held: Vec<String> = text
                .split_whitespace()
                .filter(|w| w.len() == 32 && w.chars().all(|c| c.is_ascii_hexdigit()))
                .map(str::to_string)
                .collect();
            let mut lines = vec![format!(
                "claim: {assignee} already holds a live node; one live claim per assignee."
            )];
            for hex in &held {
                let name = run_captured("claimdag", &["get", hex])
                    .ok()
                    .and_then(|s| {
                        s.stdout
                            .lines()
                            .next()
                            .and_then(|l| l.split_whitespace().last())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| hex.clone());
                lines.push(format!(
                    "  holds {name}: `ljos complete {name} --status done` finishes it, \
                     `ljos release {name} --assignee {assignee}` hands it back"
                ));
            }
            bail!("{}", lines.join("\n"))
        }
    }
}

/// Hand a session node back before it is terminal: ready again, assignee
/// cleared, generation moved.
///
/// # Errors
///
/// The claim graph's refusal: not held, or held by somebody else.
pub fn release(node: &str, assignee: &str) -> Result<String> {
    let id = node_for(node)?;
    let actor = work_id(&occupancy_scope(assignee, node));
    let said = run_captured("claimdag", &["release", &id, "--actor", &actor])?;
    drop_hold(&actor);
    drop_playbook(node);
    Ok(said.stdout)
}

/// What a conversation left beside the claim graph when it took a node:
/// the name it held under, its seat, the runner process, and when. The
/// claim graph keeps only the hashed actor; this is how a later
/// conversation that finds the node held learns who holds it, and whether
/// that conversation is still running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    pub assignee: String,
    pub seat: String,
    pub pid: u32,
    pub comm: String,
    pub since: String,
}

fn hold_record_path(actor: &str) -> PathBuf {
    runtime_dir().join(format!("hold-{actor}"))
}

/// The process that owns this conversation: the first ancestor that is
/// not a shell or a wrapper. For the MCP server that is the runner; for
/// the command line it is the runner above the shell, else the shell the
/// person types into.
fn conversation_process() -> (u32, String) {
    let chain = ancestry();
    chain
        .iter()
        .skip(1)
        .find(|(_, comm)| !WRAPPERS.contains(&comm.as_str()))
        .or_else(|| chain.get(1))
        .cloned()
        .unwrap_or((std::process::id(), String::new()))
}

fn write_hold(actor: &str, assignee: &str) {
    let (pid, comm) = conversation_process();
    let path = hold_record_path(actor);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(
        path,
        format!(
            "{assignee}\n{}\n{pid}\n{comm}\n{}\n",
            seat_name(),
            now_utc()
        ),
    );
}

fn drop_hold(actor: &str) {
    let _ = std::fs::remove_file(hold_record_path(actor));
}

fn read_hold(actor: &str) -> Option<Hold> {
    let text = std::fs::read_to_string(hold_record_path(actor)).ok()?;
    let mut lines = text.lines();
    Some(Hold {
        assignee: lines.next()?.to_string(),
        seat: lines.next()?.to_string(),
        pid: lines.next()?.trim().parse().ok()?,
        comm: lines.next()?.to_string(),
        since: lines.next()?.to_string(),
    })
}

/// Whether the conversation that wrote a hold is still running: its
/// process exists and is still the program it was. Off Linux nothing can
/// be read, and an unknown conversation is taken as running.
fn hold_alive(hold: &Hold) -> bool {
    match parent_and_comm(hold.pid) {
        Some((_, comm)) => comm == hold.comm,
        None => !cfg!(target_os = "linux"),
    }
}

/// `; revises N earlier` when the pack closed earlier memories' windows
/// for this one (same kind, a rewrite of the same claim or an explicit
/// `supersedes`), else empty. The revision is the pack's; this names it.
fn revision_note(body: &Value) -> String {
    match body["supersedes"].as_array().map(Vec::len).unwrap_or(0) {
        0 => String::new(),
        1 => "; revises 1 earlier memory, now closed".to_string(),
        n => format!("; revises {n} earlier memories, now closed"),
    }
}

/// One issue as JSON from the tracker library. Same card as `vissue show --json`.
///
/// # Errors
///
/// The tracker root cannot be resolved, or `id` is not in it.
pub fn tracker_show_json(id: &str) -> Result<Value> {
    let layout = vissue_core::Layout::resolve(None, None).map_err(anyhow::Error::from)?;
    let found = vissue_core::Router::load(layout)
        .map_err(anyhow::Error::from)?
        .find_by_id(id)
        .map_err(anyhow::Error::from)?;
    vissue_core::agent::show_json(&found.layout, id).map_err(anyhow::Error::from)
}

/// Whether an issue asks for a decision: a `decision` tag, a `decision`
/// type, or a body line opening `Options:`.
#[must_use]
pub fn is_decision(v: &Value) -> bool {
    let tagged = v["tags"]
        .as_array()
        .is_some_and(|t| t.iter().any(|x| x.as_str() == Some("decision")));
    let typed = v["properties"]["TYPE"].as_str() == Some("decision");
    let listed = v["body"]
        .as_str()
        .is_some_and(|b| b.lines().any(|l| l.trim_start().starts_with("Options:")));
    tagged || typed || listed
}

/// The issue's title, for a cue, from the tracker.
fn issue_title(issue: &str) -> Result<String> {
    let v = tracker_show_json(issue)?;
    Ok(v.get("title")
        .and_then(Value::as_str)
        .unwrap_or(issue)
        .to_string())
}

/// One dated event on an issue's timeline, from whichever store holds it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Event {
    /// Days since the epoch of the event's date.
    pub days: i64,
    /// `HH:MM` when the stamp carries a time, else empty; sorts after the
    /// day.
    pub clock: String,
    /// `tracker`, `deed` or `memory`: the store the event came from.
    pub source: &'static str,
    /// The event in one line.
    pub text: String,
}

/// The issue's timeline as dated rows. The HUD paints this; it does not
/// parse `ljos timeline` stdout. Tracker rows come from
/// [`vissue_core::agent::show_json`]. Deed rows still shell `deedar evidence`,
/// a named gap (`deedar::Store::evidence`).
///
/// # Errors
///
/// The tracker not answering. A deed store or pack that does not answer
/// leaves its rows out; the tracker's rows are the spine.
pub fn timeline_events(issue: &str, limit: usize) -> Result<Vec<Event>> {
    Ok(timeline_of(issue, limit)?.1)
}

fn timeline_of(issue: &str, limit: usize) -> Result<(String, Vec<Event>)> {
    let v = tracker_show_json(issue)?;
    let title = v["title"].as_str().unwrap_or(issue).to_string();
    let mut events = tracker_events(&v);
    for accession in v["deeds"].as_array().into_iter().flatten() {
        let Some(accession) = accession.as_str() else {
            continue;
        };
        if let Ok(said) = run_captured("deedar", &["evidence", accession]) {
            if let Some(ev) = deed_event(accession, &said.stdout, local_offset) {
                events.push(ev);
            }
        }
    }
    if let Ok(island) = packset_island(&title, false) {
        for atom in island["island"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| reviewable(a))
            .take(8)
        {
            if let Some((days, clock)) = stamp_key(atom["ts"].as_str().map(local_stamp).as_deref())
            {
                events.push(Event {
                    days,
                    clock,
                    source: "memory",
                    text: format!(
                        "[{}] {}",
                        atom["kind"].as_str().unwrap_or("claim"),
                        atom["text"].as_str().unwrap_or("").trim()
                    ),
                });
            }
        }
    }
    events.sort_by(|a, b| (a.days, &a.clock).cmp(&(b.days, &b.clock)));
    let skip = events.len().saturating_sub(limit);
    Ok((title, events[skip..].to_vec()))
}

/// The issue's timeline, the three stores read as one dated list, oldest
/// first: the tracker's logbook (creation, state changes, claims, notes),
/// the deeds the issue cites with the time each was produced, and the
/// memories the issue's title activates with the time each was written.
/// The reader gets time as data, not as stamps to do arithmetic on: each
/// line carries its age and the gap since the line before it, and a later
/// line supersedes an earlier one on the same matter.
///
/// # Errors
///
/// The tracker not answering. A deed store or pack that does not answer
/// leaves its rows out; the tracker's rows are the spine.
pub fn timeline(issue: &str, limit: usize) -> Result<String> {
    let (title, events) = timeline_of(issue, limit)?;
    Ok(format!(
        "timeline of {issue}: {title}
{}",
        format_events(&events, &now_local())
    ))
}

/// The reader's seconds east of UTC at the instant `secs`. The tracker
/// writes org stamps in local wall time; a timeline reads every store in it.
fn local_offset(secs: i64) -> i64 {
    use chrono::{Local, Offset, TimeZone};
    Local
        .timestamp_opt(secs, 0)
        .single()
        .map_or(0, |t| i64::from(t.offset().fix().local_minus_utc()))
}

/// Now in local wall time, `YYYY-MM-DDTHH:MM:SS`, the zone of the tracker's
/// org stamps.
fn now_local() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// An RFC 3339 stamp as local wall time, `YYYY-MM-DDTHH:MM`; any other shape
/// comes back unchanged.
fn local_stamp(ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(ts.trim()).map_or_else(
        |_| ts.to_string(),
        |t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%dT%H:%M")
                .to_string()
        },
    )
}

/// The tracker's own events on an issue: created, each state change, the
/// claim, each note.
fn tracker_events(v: &Value) -> Vec<Event> {
    let mut events = Vec::new();
    let mut push = |stamp: Option<&str>, source: &'static str, text: String| {
        if let Some((days, clock)) = stamp_key(stamp) {
            events.push(Event {
                days,
                clock,
                source,
                text,
            });
        }
    };
    push(
        v["properties"]["CREATED"].as_str(),
        "tracker",
        "created".to_string(),
    );
    if let Some(by) = v["claimed_by"].as_str() {
        push(
            v["claimed_at"].as_str(),
            "tracker",
            format!("claimed by {by}"),
        );
    }
    if let Some(d) = v["properties"]["DEADLINE"].as_str() {
        push(
            v["properties"]["DEADLINE"].as_str(),
            "tracker",
            format!("DEADLINE {d}"),
        );
    }
    if let Some(s) = v["properties"]["SCHEDULED"].as_str() {
        push(
            v["properties"]["SCHEDULED"].as_str(),
            "tracker",
            format!("SCHEDULED {s}"),
        );
    }
    // The logbook is newest first; the timeline reads oldest first.
    for e in v["logbook"].as_array().into_iter().flatten().rev() {
        let stamp = e["timestamp"].as_str();
        if let Some(note) = e["note"].as_str() {
            push(stamp, "tracker", format!("note: {}", note.trim()));
        } else if let Some(to) = e["to_state"].as_str() {
            push(
                stamp,
                "tracker",
                format!("{} -> {to}", e["from_state"].as_str().unwrap_or("-")),
            );
        }
    }
    events
}

/// A deed's event from `deedar evidence`: the time it was produced, by
/// whom.
/// `offset_of` gives the reader's seconds east of UTC at that instant, so
/// the deed lands on the same wall-clock day as the tracker's org stamps.
fn deed_event(accession: &str, evidence: &str, offset_of: fn(i64) -> i64) -> Option<Event> {
    let utc: i64 = evidence
        .lines()
        .find_map(|l| l.strip_prefix("time="))?
        .trim()
        .parse()
        .ok()?;
    let secs = utc + offset_of(utc);
    let by = evidence
        .lines()
        .find_map(|l| l.strip_prefix("producedBy="))
        .map(str::trim)
        .unwrap_or("-");
    Some(Event {
        days: secs.div_euclid(86_400),
        clock: format!(
            "{:02}:{:02}",
            secs.rem_euclid(86_400) / 3600,
            secs.rem_euclid(86_400) % 3600 / 60
        ),
        source: "deed",
        text: format!("{accession} produced by {by}"),
    })
}

/// The sort key of a stamp in any of the three stores' shapes: RFC 3339
/// (`2026-09-12T21:54:00Z`), an org stamp (`[2026-09-12 Sat 21:54]`), or a
/// date alone. Day, then `HH:MM` when the stamp has one.
fn stamp_key(stamp: Option<&str>) -> Option<(i64, String)> {
    let s = stamp?
        .trim()
        .trim_start_matches(['[', '<'])
        .trim_end_matches([']', '>']);
    let days = days_of_stamp(Some(s))?;
    let rest = &s[10..];
    let clock = rest
        .split(['T', ' '])
        .find(|t| t.len() >= 5 && t.as_bytes()[2] == b':')
        .map(|t| t[..5].to_string())
        .unwrap_or_default();
    Some((days, clock))
}

/// One line per event: date, age, gap since the line before, store, text.
fn format_events(events: &[Event], now: &str) -> String {
    let today = days_of_stamp(Some(now)).unwrap_or(0);
    let mut out = String::new();
    let mut last: Option<i64> = None;
    for e in events {
        let gap = match last {
            None => String::new(),
            Some(d) if e.days == d => "same day".to_string(),
            Some(d) => format!("+{} d", e.days - d),
        };
        last = Some(e.days);
        out.push_str(&format!(
            "{} {}	{}	{}	{}	{}
",
            civil_of_days(e.days),
            e.clock,
            age_of(Some(&civil_of_days(e.days)), &civil_of_days(today)),
            gap,
            e.source,
            e.text
        ));
    }
    out
}

/// `YYYY-MM-DD` of a day count since the epoch.
fn civil_of_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Open a sitting on an issue, in the protocol's order, and stop at the
/// first habitat that does not answer: doctor, cards, the review clock,
/// the island the issue's title activates, the working set, the timeline,
/// the claim.
/// One verb, so the loop that makes the seat a memory runs every time and
/// not only when somebody remembers to run it.
///
/// # Errors
///
/// A required habitat down, or the claim refused (the refusal names what
/// the assignee still holds).
pub fn sitting(issue: &str, assignee: &str, cards_dir: &Path) -> Result<String> {
    sitting_gated(issue, assignee, cards_dir, false, None)
}

/// The blockers of an issue that are still open, as `id (STATE)`, read
/// from the tracker. Empty when the issue is workable, or when the tracker
/// does not answer (the sitting's doctor already said so).
pub fn open_blockers(issue: &str) -> Vec<String> {
    let Ok(shown) = tracker_show_json(issue) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for id in shown["blocked_by"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let state = tracker_show_json(id)
            .ok()
            .and_then(|v| v["state"].as_str().map(str::to_string))
            .unwrap_or_else(|| "?".to_string());
        if !matches!(state.as_str(), "DONE" | "CANCELLED") {
            out.push(format!("{id} ({state})"));
        }
    }
    out
}

/// [`sitting`], and with `anyway` the claim goes through even when the
/// issue's blockers are open. Without it a blocked issue is refused before
/// anything is claimed: the tracker's graph says what is workable, and a
/// seat that sits on blocked work sits on nothing it can finish.
/// `playbook` names the recipe copied into `== playbook` before recall;
/// absent, a name already bound, else a closed-set token in the title,
/// else `sit`. Sitting always binds one of the five before claim. Finish
/// and release drop the sticky name.
pub fn sitting_gated(
    issue: &str,
    assignee: &str,
    cards_dir: &Path,
    anyway: bool,
    playbook: Option<&str>,
) -> Result<String> {
    let mut out = String::new();
    let rows = doctor_seat();
    out.push_str("== doctor\n");
    out.push_str(&format_doctor(&rows));
    if !healthy(&rows) {
        bail!("{out}sitting: a required habitat does not answer; nothing was claimed");
    }
    // Other machines' memories of this scope arrive before the island is
    // walked, or the sitting orients on half the seat.
    out.push_str("== sync\n");
    out.push_str(&sync::sync_repo(true, false).unwrap_or_else(|e| format!("sync: {e:#}\n")));
    out.push_str("== cards\n");
    out.push_str(&cards(cards_dir)?);
    let title = issue_title(issue)?;
    let island = packset_island(&title, false)?;
    out.push_str("== due\n");
    out.push_str(&sitting_due_report(&island)?);
    out.push_str(&format!("== island: {title}\n"));
    // The strongest eight: a sitting wants orientation, not the whole
    // cluster; `ljos island` prints it all.
    let mut top = island.clone();
    if let Some(rows) = top["island"].as_array_mut() {
        rows.truncate(8);
    }
    out.push_str(&format_island(&top));
    out.push_str("== blockers\n");
    let blockers = open_blockers(issue);
    if blockers.is_empty() {
        out.push_str("none open; the issue is workable\n");
    } else {
        out.push_str(&format!("open: {}\n", blockers.join(", ")));
        if !anyway {
            bail!(
                "{out}sitting: {issue} is blocked by {}; finish those first, or `ljos sitting {issue} --anyway` to sit on it regardless. Nothing was claimed",
                blockers.join(", ")
            );
        }
        out.push_str("sitting anyway, as asked\n");
    }
    // A decision is handed to the panel by the sitting itself: agents ran
    // only the verbs the loop put in front of them, never an optional
    // `ljos panel`, so the sitting binds the panel recipe and writes the
    // briefs.
    let decision = tracker_show_json(issue).is_ok_and(|v| is_decision(&v));
    let name = match (playbook, decision) {
        (None, true) if bound_playbook(issue).is_none() => "company-panel".to_string(),
        _ => resolve_sitting_playbook(issue, &title, playbook)?,
    };
    out.push_str("== playbook\n");
    out.push_str(&copy_playbook(issue, &name)?);
    if decision {
        out.push_str("== panel\n");
        let dir = runtime_dir().join(format!("panel-{issue}"));
        match panel(issue, &dir) {
            Ok(said) => out.push_str(&format!(
                "{issue} is a decision. Run the panel before the work: one subagent per brief, each casts its ballot, then `ljos consensus {issue}`. `ljos finish {issue} --close` refuses with fewer than two ballots.\n{said}"
            )),
            Err(e) => out.push_str(&format!("{issue} is a decision, and the panel could not be written: {e:#}\n")),
        }
    }
    out.push_str("== recall\n");
    out.push_str(&run_captured("vissue", &["recall", issue])?.stdout);
    // The last twelve dated events across the three stores; `ljos
    // timeline` prints them all.
    out.push_str("== timeline\n");
    out.push_str(&timeline(issue, SITTING_TIMELINE)?);
    out.push_str("== claim\n");
    out.push_str(&claim(issue, assignee)?);
    out.push_str(&persist_tracker(issue, "claimed"));
    Ok(out)
}

/// Close a sitting: remember the lesson when there is one, fire the island
/// the issue's title activates, complete the session node, and learn from
/// the outcome when one is named. Without a lesson the report says so,
/// because a sitting that taught nothing worth two sentences is rare and
/// worth noticing.
///
/// # Errors
///
/// Any habitat refusing; the pack refuses a lesson longer than two
/// sentences, the claim graph a status that is not terminal.
/// Finish a session node only if `gen` is still the live lease.
///
/// # Errors
///
/// The claim graph refuses a stale generation, a missing actor, or a
/// status that is not terminal.
pub fn complete(
    node: &str,
    status: Option<&str>,
    assignee: &str,
    gen: Option<u64>,
) -> Result<String> {
    let id = node_for(node)?;
    let actor = work_id(&occupancy_scope(assignee, node));
    let gen_s = live_gen(&id, gen)?.to_string();
    let mut args = vec![
        "complete",
        id.as_str(),
        "--actor",
        actor.as_str(),
        "--gen",
        gen_s.as_str(),
    ];
    if let Some(s) = status {
        args.push("--status");
        args.push(s);
    }
    let said = run_captured("claimdag", &args)?;
    drop_hold(&actor);
    drop_playbook(node);
    Ok(said.stdout)
}

#[expect(
    clippy::too_many_arguments,
    reason = "The public finish signature preserves its independent command options"
)]
pub fn finish(
    issue: &str,
    status: &str,
    lesson: Option<&str>,
    outcome: Option<&str>,
    beta: f64,
    assignee: &str,
    gen: Option<u64>,
    close: bool,
) -> Result<String> {
    // A decision closes on ballots, not on the say of the seat that sat on
    // it; refused before anything is written, so nothing half-happens.
    if close && tracker_show_json(issue).is_ok_and(|v| is_decision(&v)) {
        let said = run_captured("vissue", &["vote", issue, "--json"])?;
        let ballots = forecasts_from_json(&said.stdout)?.len();
        if ballots < 2 {
            bail!(
                "finish: {issue} is a decision and holds {ballots} ballot{}; run the panel \
                 (`ljos panel {issue}`), have each persona cast `ljos vote {issue} --for OPTION --as NAME`, \
                 settle with `ljos consensus {issue}`, then --close. Nothing was written",
                if ballots == 1 { "" } else { "s" }
            );
        }
    }
    let mut out = String::new();
    match lesson.map(str::trim).filter(|l| !l.is_empty()) {
        Some(text) => {
            // A lesson learned on an issue belongs to the scope of the
            // repository that holds the issue, wherever it was written.
            let scope = sync::scope_for_issue(issue);
            let body = packset_write_scoped("Remember", text, issue, scope.as_deref())?;
            out.push_str(&format!(
                "remembered {}{}\n",
                body.get("id").and_then(Value::as_str).unwrap_or("-"),
                revision_note(&body)
            ));
        }
        None => out.push_str(
            "no lesson remembered this sitting; `ljos remember` takes one in two sentences\n",
        ),
    }
    let title = issue_title(issue)?;
    let island = packset_island(&title, true)?;
    if island["weak"].as_bool().unwrap_or(false) {
        out.push_str(&format!(
            "did not fire the island for {title:?}: its seeds are hits no two scorers agreed on{}; wiring them would tighten the wrong links\n",
            if island["dense"].as_bool().unwrap_or(true) { "" } else { " (the encoder is down, ranking is lexical only)" }
        ));
    } else if island["held"].as_bool().unwrap_or(false) {
        // Another sitting on this issue, or another persona's, fired the
        // same claims within the hour; the pack tightened them once.
        out.push_str(&format!(
            "the island for {title:?} fired within the hour; not fired again\n"
        ));
    } else {
        let fired = island["island"].as_array().map_or(0, Vec::len);
        out.push_str(&format!(
            "fired the island for {title:?}: {fired} memories. Those links gained weight under the seat, not under a persona. The next walk of this title follows them.\n"
        ));
    }
    let terminal = ["done", "failed", "cancelled"];
    if !terminal.contains(&status) {
        bail!("finish: status {status:?} is not one of done, failed, cancelled");
    }
    complete(issue, Some(status), assignee, gen)?;
    out.push_str(&format!(
        "completed the session node for {issue} as {status}\n"
    ));
    if let Some(option) = outcome.map(str::trim).filter(|o| !o.is_empty()) {
        let said = run_captured("vissue", &["vote", issue, "--json"])?;
        let forecasts = forecasts_from_json(&said.stdout)?;
        if forecasts.len() < 2 {
            out.push_str("outcome named but fewer than two ballots; nothing to learn from\n");
        } else {
            let ballots: Vec<(String, String)> = forecasts
                .iter()
                .map(|f| (f.agent.clone(), f.choice.clone()))
                .collect();
            let about = island_entities(issue).unwrap_or_default();
            let (rows, moved, calibration) =
                learn_and_write(&ballots, option, beta, &about, &forecasts)?;
            out.push_str(&learn_reading(
                rows.len(),
                moved.len(),
                &forecasts,
                option,
                &calibration,
            ));
            out.push('\n');
        }
    }
    // A sitting ending is not the work being accepted: a review can be
    // posted and still be open, a build can be green and still unmerged.
    // The ticket closes only when asked, so a blocker on it stays a blocker.
    if close && status.eq_ignore_ascii_case("done") {
        run_as("vissue", &["update", issue, "-s", "DONE"], None)
            .with_context(|| format!("finish: could not close the ticket {issue}"))?;
        out.push_str(&format!("closed the ticket {issue}\n"));
    } else {
        out.push_str(&format!(
            "the ticket {issue} keeps its state; `ljos finish {issue} --close` or `vissue update {issue} -s DONE` closes it when the work is accepted\n"
        ));
    }
    out.push_str(&persist_tracker(issue, "finished"));
    // What this sitting taught leaves the machine with the tracker.
    out.push_str(&sync::sync_repo(false, true).unwrap_or_else(|e| format!("sync: {e:#}\n")));
    Ok(out)
}

/// An exclusive advisory lock on a file, held until dropped. Taking it
/// blocks; a lock that cannot be opened is no lock, and the commit goes on
/// as it would have without one.
pub struct CommitLock(Option<std::fs::File>);

impl CommitLock {
    #[must_use]
    pub fn acquire(path: &std::path::Path) -> Self {
        use std::os::unix::io::AsRawFd;
        let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        else {
            return Self(None);
        };
        // SAFETY: flock on a descriptor this struct owns until drop.
        let ok = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0;
        Self(ok.then_some(file))
    }
}

impl Drop for CommitLock {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        if let Some(file) = &self.0 {
            // SAFETY: the descriptor is still open; unlocking it cannot fail
            // in a way that matters, since close releases it too.
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

/// Commit the tracker file that holds `issue` and push it, when the tracker
/// is a git checkout. A write that stays in one working tree is lost to
/// every other host and to a rebuilt one; closures made on one laptop and
/// never committed were how tickets came back open. Only that file is
/// committed (`--only`), so another seat's staged work is left alone. Never
/// an error: the verb already happened, and the line says what did not.
/// `LJOS_TRACKER_GIT=off` skips it; `=commit` commits without pushing.
pub fn persist_tracker(issue: &str, verb: &str) -> String {
    let mode = std::env::var("LJOS_TRACKER_GIT").unwrap_or_default();
    if matches!(mode.as_str(), "off" | "0" | "false") {
        return "tracker git: off (LJOS_TRACKER_GIT)\n".into();
    }
    let path = match vissue_core::Layout::resolve(None, None)
        .and_then(vissue_core::Router::load)
        .and_then(|router| router.find_by_id(issue))
    {
        Ok(hit) => hit.path,
        Err(e) => return format!("tracker git: could not find {issue}: {e}\n"),
    };
    let Some(dir) = path.parent() else {
        return format!("tracker git: {} has no directory\n", path.display());
    };
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
    };
    let file = path.to_string_lossy().to_string();
    match git(&["rev-parse", "--is-inside-work-tree"]) {
        Ok(o) if o.status.success() => {}
        _ => return "tracker git: the tracker is not a git checkout\n".into(),
    }
    match git(&["status", "--porcelain", "--", &file]) {
        Ok(o) if o.status.success() && o.stdout.is_empty() => {
            return "tracker git: nothing to commit\n".into();
        }
        Ok(o) if o.status.success() => {}
        Ok(o) => return format!("tracker git: {}\n", first_line(&o.stderr)),
        Err(e) => return format!("tracker git: {e}\n"),
    }
    let message = format!("chore(issues): {issue} {verb}");
    // Every seat on the host commits this one checkout. The add and the
    // commit run under one lock in the git directory, so ljos writers queue
    // instead of meeting on index.lock; a git process outside ljos that
    // holds the index is waited out a few times before the line says so.
    let common = git(&["rev-parse", "--git-common-dir"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| dir.join(String::from_utf8_lossy(&o.stdout).trim()))
        .unwrap_or_else(|| dir.join(".git"));
    let _held = CommitLock::acquire(&common.join("ljos-commit.lock"));
    let mut committed = git(&["add", "--", &file])
        .and_then(|_| git(&["commit", "-q", "--only", "-m", &message, "--", &file]));
    for wait_ms in [200_u64, 400, 800, 1600, 3200] {
        let busy = matches!(&committed, Ok(o) if !o.status.success()
            && String::from_utf8_lossy(&o.stderr).contains("index.lock"));
        if !busy {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(wait_ms));
        committed = git(&["add", "--", &file])
            .and_then(|_| git(&["commit", "-q", "--only", "-m", &message, "--", &file]));
    }
    drop(_held);
    match committed {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            return format!(
                "tracker git: commit refused: {}\n",
                first_line(if o.stderr.is_empty() {
                    &o.stdout
                } else {
                    &o.stderr
                })
            );
        }
        Err(e) => return format!("tracker git: {e}\n"),
    }
    if mode == "commit" {
        return format!("tracker git: committed {message}; not pushed (LJOS_TRACKER_GIT=commit)\n");
    }
    // A push can run a repository's pre-push hook that publishes data first
    // and takes minutes. The sitting waits a bounded time; a push still going
    // after that finishes on its own and writes its log where the line says.
    let log = runtime_dir().join(format!("tracker-push-{}.log", std::process::id()));
    let _ = std::fs::create_dir_all(runtime_dir());
    let Ok(out) = std::fs::File::create(&log) else {
        return format!("tracker git: committed {message}; push not started: no log file\n");
    };
    let err = out.try_clone();
    // Every other remote that carries the branch gets it too: seats that
    // read a tracker through different remotes see each other's claims
    // only when every push reaches all of them.
    let mirrors = tracker_upstream(dir)
        .and_then(|up| tracker_mirrors(dir, &up))
        .unwrap_or_default();
    // A push another host beat is merged, not left ahead: the next catch-up
    // only fast-forwards, so a clone left diverged never recovered. A merge
    // rather than a rebase, because other seats keep uncommitted edits in
    // the same worktree; issues.org merges by heading through vissue.
    let mut script =
        String::from("git push -q || { git pull -q --no-rebase --no-edit && git push -q; }; rc=$?");
    for (remote, branch) in &mirrors {
        script.push_str(&format!(
            "; git push -q '{remote}' 'HEAD:refs/heads/{branch}' || rc=1"
        ));
    }
    script.push_str("; exit $rc");
    let mut push = std::process::Command::new("sh");
    push.current_dir(dir)
        .args(["-c", &script])
        .stdin(std::process::Stdio::null())
        .stdout(out);
    if let Ok(err) = err {
        push.stderr(err);
    }
    let mut child = match push.spawn() {
        Ok(c) => c,
        Err(e) => return format!("tracker git: committed {message}; push failed: {e}\n"),
    };
    let wait = push_wait();
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let _ = std::fs::remove_file(&log);
                return format!("tracker git: committed and pushed {message}\n");
            }
            Ok(Some(_)) => {
                let said = std::fs::read(&log).unwrap_or_default();
                return format!(
                    "tracker git: committed {message}; push refused: {}\n",
                    first_line(&said)
                );
            }
            Ok(None) if started.elapsed() < wait => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Ok(None) => {
                return format!(
                    "tracker git: committed {message}; push still running after {}s, finishing in the background (log {})\n",
                    wait.as_secs(),
                    log.display()
                );
            }
            Err(e) => return format!("tracker git: committed {message}; push failed: {e}\n"),
        }
    }
}

/// How long a sitting waits for the tracker push: `LJOS_TRACKER_PUSH_WAIT`
/// seconds, else 5: agents wrap a finish in a timeout of about ten seconds.
fn push_wait() -> std::time::Duration {
    let secs = std::env::var("LJOS_TRACKER_PUSH_WAIT")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(5);
    std::time::Duration::from_secs(secs)
}

fn first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// The weight a voter of estimated accuracy `p` earns: the log odds
/// `ln(p / (1 - p))`, the optimal weight for independent voters on a
/// two-way choice (Nitzan and Paroush, doi:10.2307/2526438; a weighted
/// majority under these weights is the maximum-likelihood decision), with
/// `p` held inside `[0.01, 0.99]` so a perfect record does not become an
/// infinite vote, and a voter at or under chance at [`TRUST_FLOOR`]. The
/// weights are scaled so the most reliable voter stands at one, which is
/// the scale the trust rows live on; the ratios between voters are the
/// rule's.
#[must_use]
pub fn calibration_weights(accuracy: &[(String, f64)]) -> Vec<(String, f64)> {
    let logit = |p: f64| {
        let p = p.clamp(0.01, 0.99);
        (p / (1.0 - p)).ln()
    };
    let raw: Vec<(String, f64)> = accuracy
        .iter()
        .map(|(who, p)| (who.clone(), logit(*p).max(0.0)))
        .collect();
    let top = raw.iter().map(|(_, w)| *w).fold(0.0_f64, f64::max);
    raw.into_iter()
        .map(|(who, w)| {
            let scaled = if top > 0.0 { w / top } else { 0.0 };
            (who, scaled.clamp(TRUST_FLOOR, 1.0))
        })
        .collect()
}

/// Turn a project's voting history into trust rows without anyone naming
/// an outcome: Dawid and Skene's accuracy per voter
/// (doi:10.2307/2346806), from `ljos-consensus reliability`, turned into
/// the weight every other voter gives that voter by
/// [`calibration_weights`]: log odds, so a voter right nine times in ten
/// outweighs one right six times in ten by five to one, not three to two.
/// Rows are complete and floored at [`TRUST_FLOOR`], so the settle sees
/// the whole graph.
///
/// # Errors
///
/// No issue with two or more ballots, the consensus binary absent, or the
/// pack refusing a row.
pub fn calibrate(project: &str, rounds: usize) -> Result<Vec<Trust>> {
    let said = run_captured(
        "ljos-consensus",
        &[
            "reliability",
            "--project",
            project,
            "--rounds",
            &rounds.to_string(),
        ],
    )?;
    let v: Value = serde_json::from_str(&said.stdout).context("reliability: not JSON")?;
    let accuracy = v
        .get("accuracy")
        .and_then(Value::as_object)
        .context("reliability: no accuracy object")?;
    let mut voters: Vec<(String, f64)> = accuracy
        .iter()
        .filter_map(|(k, val)| val.as_f64().map(|a| (k.clone(), a)))
        .collect();
    voters.sort_by(|a, b| a.0.cmp(&b.0));
    if voters.len() < 2 {
        bail!("calibrate: fewer than two voters in {project}");
    }
    let weights = calibration_weights(&voters);
    let mut rows = Vec::new();
    for (from, _) in &voters {
        for (to, weight) in &weights {
            if from == to {
                continue;
            }
            rows.push(Trust {
                from: from.clone(),
                to: to.clone(),
                weight: *weight,
                about: Vec::new(),
            });
        }
    }
    for row in &rows {
        write_trust(row, &[])?;
    }
    Ok(rows)
}

/// What a search score is. Empty and nonempty are different facts from a
/// writer that did not answer.
#[must_use]
pub fn search_reading(n: usize) -> &'static str {
    if n == 0 {
        "No hits. The pack holds nothing on this query. A failure would say the writer did not answer."
    } else {
        "Score is how the scorers ranked this query. The fraction is how many of them named the hit. Neither is whether the claim is true. A later line on the same matter supersedes an earlier one."
    }
}

/// One line per hit: score, how many scorers named it out of how many
/// ran, kind, id, age, text. The age is the one column a reader needs to
/// lay the hits on a timeline; the count is what the hook keys on.
pub fn format_hits(hits: &[Hit]) -> String {
    let now = now_utc();
    let mine = seat_name();
    let mut out = format!("{}\n", search_reading(hits.len()));
    for h in hits {
        let id = h.id.as_deref().unwrap_or("-");
        let named = match (h.ballots, h.of) {
            (Some(b), Some(of)) => format!("{b}/{of}"),
            _ => "-".to_string(),
        };
        let from = other_seat(&h.entities, &mine)
            .map(|s| format!(" (from {s})"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{:.4}\t{}\t{}\t{}\t{}{}\t{}\n",
            h.score,
            named,
            h.kind,
            id,
            age_of(h.ts.as_deref(), &now),
            from,
            h.text
        ));
    }
    out
}

/// The seat that wrote a hit, when it was another than this one. Many
/// seats share a pack; a reader is told whose lesson it is reading only
/// when that is news.
#[must_use]
pub fn other_seat(entities: &[String], mine: &str) -> Option<String> {
    entities
        .iter()
        .filter_map(|e| e.strip_prefix(SEAT_ENTITY))
        .find(|s| !s.is_empty() && *s != mine)
        .map(str::to_string)
}

/// The line a hit takes in injected context and in a brief: kind, age and,
/// when another seat wrote it, that seat in the bracket, then the text.
fn hit_line(h: &Hit, now: &str) -> String {
    let from = other_seat(&h.entities, &seat_name())
        .map(|s| format!(", from {s}"))
        .unwrap_or_default();
    format!(
        "- [{}{}{}] {}",
        if h.kind.is_empty() { "claim" } else { &h.kind },
        age_tag(h.ts.as_deref(), now),
        from,
        h.text.trim()
    )
}

/// `, N days ago` for a bracket, empty when the stamp is missing.
fn age_tag(ts: Option<&str>, now: &str) -> String {
    let age = age_of(ts, now);
    if age.is_empty() {
        age
    } else {
        format!(", {age}")
    }
}

/// How long ago a stamp was, in words a reader can place: `today`,
/// `yesterday`, `N days ago`, then weeks, months and years once the count
/// stops fitting the smaller unit. Empty when the stamp is missing or
/// unreadable, `in N days` for a stamp ahead of `now`.
#[must_use]
pub fn age_of(ts: Option<&str>, now: &str) -> String {
    let (Some(then), Some(today)) = (days_of_stamp(ts), days_of_stamp(Some(now))) else {
        return String::new();
    };
    let days = today - then;
    match days {
        d if d < 0 => format!("in {} day{}", -d, if d == -1 { "" } else { "s" }),
        0 => "today".into(),
        1 => "yesterday".into(),
        d if d < 14 => format!("{d} days ago"),
        d if d < 61 => format!("{} weeks ago", d / 7),
        d if d < 730 => format!("{} months ago", d / 30),
        d => format!("{} years ago", d / 365),
    }
}

/// Days since the epoch of an RFC 3339 stamp's date, or none when the
/// first ten characters do not read as `YYYY-MM-DD`.
fn days_of_stamp(ts: Option<&str>) -> Option<i64> {
    let ts = ts?;
    let date = ts.get(..10)?;
    let mut it = date.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Civil date to days since the epoch (Howard Hinnant's algorithm).
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// Read-only cards. Only [`CARD_NAMES`], never created, never written.
pub fn cards(dir: &Path) -> Result<String> {
    let mut out = String::new();
    for name in CARD_NAMES {
        let p = dir.join(name);
        if p.is_file() {
            out.push_str(&format!("--- {} ---\n", p.display()));
            out.push_str(&std::fs::read_to_string(&p)?);
        }
    }
    Ok(out)
}

pub fn policy_line(argv: &[String]) -> Result<String> {
    if argv.is_empty() {
        bail!("policy: pass the argv to check");
    }
    Ok(argv.join(" "))
}

/// The argv line, then what the pack knows that bears on it: the memory a
/// policy layer injects beside its verdict. The line prints even when the
/// pack is down; the memory is the part that may be empty.
pub fn policy_with_memory(argv: &[String]) -> Result<String> {
    let line = policy_line(argv)?;
    let call = HookCall {
        event: "argv".into(),
        cue: line.clone(),
        session: None,
        shape: HookShape::Asks,
    };
    let context = hook_context(&call, 5);
    // The rules are the law's memory: a deny or an ask fires before the
    // context, so a reader sees the verdict first.
    let rules = rules_from_pack().unwrap_or_default();
    let ruled = hook_output_ruled(&call, &context, verdict_for(&rules, &line));
    match tcb_check(argv) {
        Some(tcb) if !tcb.is_empty() => Ok(format!("{line}\n{tcb}\n{ruled}")),
        None if policyd_required() => Ok(format!("{line}\ndeny\tTCB required\n{ruled}")),
        _ => Ok(format!("{line}\n{ruled}")),
    }
}

/// Operator switch: missing TCB is a deny. Unset, absence stays open.
pub fn policyd_required() -> bool {
    matches!(
        std::env::var("POLICYD_REQUIRED").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

/// `POLICYD_BIN`, else `ljos-policyd` on PATH.
pub fn policyd_bin() -> Option<std::path::PathBuf> {
    std::env::var_os("POLICYD_BIN")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| which::which("ljos-policyd").ok())
}

/// One line from `ljos-policyd check -- argv`. None if the binary is absent
/// or failed to start. Absence is not a deny.
pub fn tcb_check(argv: &[String]) -> Option<String> {
    let bin = policyd_bin()?;
    let out = std::process::Command::new(bin)
        .arg("check")
        .arg("--")
        .args(argv)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsensusStep {
    pub bin: &'static str,
    pub args: Vec<String>,
}

/// `ljos-consensus` first, then `vissue consensus`, both under the pack's
/// trust rows when there are any. Missing bins are skipped.
pub fn consensus_steps(
    id: &str,
    have_ljos: bool,
    have_vissue: bool,
    trust: &[Trust],
) -> Result<Vec<ConsensusStep>> {
    consensus_steps_anchored(id, have_ljos, have_vissue, trust, &[])
}

/// The tag on an issue that asks for bounded confidence: a panel for a
/// broad audience is allowed to settle into clusters, and the settle says
/// how far apart they are, where a single-position model would average
/// them away. Without it the anchored model runs.
pub const BROAD_TAG: &str = "broad";

/// The confidence bound a `broad` issue settles under: voters within this
/// L1 distance of each other's opinion listen to each other.
pub const BROAD_EPSILON: f64 = 1.0;

/// The model flags an issue's tags ask for, beside the rows and anchors.
/// The kind of work sets the dynamics: `broad` runs bounded confidence.
#[must_use]
pub fn settle_flags_for(tags: &[String]) -> Vec<String> {
    if tags.iter().any(|t| t == BROAD_TAG) {
        vec!["--epsilon".into(), BROAD_EPSILON.to_string()]
    } else {
        Vec::new()
    }
}

/// [`consensus_steps_anchored`] with the model flags the issue's tags ask
/// for on the model crate's settle.
pub fn consensus_steps_for(
    id: &str,
    have_ljos: bool,
    have_vissue: bool,
    trust: &[Trust],
    personas: &[Persona],
    tags: &[String],
) -> Result<Vec<ConsensusStep>> {
    let mut steps = consensus_steps_anchored(id, have_ljos, have_vissue, trust, personas)?;
    let flags = settle_flags_for(tags);
    if !flags.is_empty() {
        for step in steps.iter_mut().filter(|s| s.bin == "ljos-consensus") {
            step.args.extend(flags.iter().cloned());
        }
    }
    Ok(steps)
}

/// The two readings beside a settle, when the pack holds what they need:
/// the surprisingly popular answer when two or more voters forecast the
/// others (`predict`), and the EigenTrust standing of the voters when
/// trust rows exist. Both are the model crate's verbs.
pub fn panel_steps(
    id: &str,
    have_ljos: bool,
    trust: &[Trust],
    predictions: &[Prediction],
) -> Vec<ConsensusStep> {
    let mut steps = Vec::new();
    if !have_ljos {
        return steps;
    }
    if predictions.len() >= 2 {
        steps.push(ConsensusStep {
            bin: "ljos-consensus",
            args: vec![
                "surprising".into(),
                "--issue".into(),
                id.into(),
                "--predictions".into(),
                predictions_json(predictions),
            ],
        });
    }
    if !trust.is_empty() {
        steps.push(ConsensusStep {
            bin: "ljos-consensus",
            args: vec!["reputation".into(), "--trust".into(), trust_json(trust)],
        });
    }
    steps
}

/// [`consensus_steps`] passing the personas' anchors to both settles as
/// `--susceptibility-of`, so a persona holds its ballot as much as it says.
pub fn consensus_steps_anchored(
    id: &str,
    have_ljos: bool,
    have_vissue: bool,
    trust: &[Trust],
    personas: &[Persona],
) -> Result<Vec<ConsensusStep>> {
    if !have_ljos && !have_vissue {
        bail!("neither ljos-consensus nor vissue is on PATH");
    }
    let mut steps = Vec::new();
    if have_ljos {
        let mut args = vec!["settle".to_string(), "--issue".into(), id.into()];
        if !trust.is_empty() {
            args.push("--trust".into());
            args.push(trust_json(trust));
        }
        if !personas.is_empty() {
            args.push("--susceptibility-of".into());
            args.push(anchors_json(personas));
        }
        steps.push(ConsensusStep {
            bin: "ljos-consensus",
            args,
        });
    }
    if have_vissue {
        let mut args = vec!["consensus".to_string(), id.into()];
        if !trust.is_empty() {
            args.push("--trust".into());
            args.push(trust_json(trust));
        }
        if !personas.is_empty() {
            args.push("--susceptibility-of".into());
            args.push(anchors_json(personas));
        }
        steps.push(ConsensusStep {
            bin: "vissue",
            args,
        });
    }
    Ok(steps)
}

pub fn on_path(bin: &str) -> bool {
    which::which(bin).is_ok()
}

pub fn run(bin: &str, args: &[impl AsRef<str>]) -> Result<()> {
    run_as(bin, args, None)
}

/// The identity a ballot is cast under: the persona named, else the seat
/// ([`whoami`]), the same name across a runner's conversations so its
/// record accrues to one voter.
#[must_use]
pub fn identity_or_seat(identity: Option<&str>) -> Option<String> {
    identity
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .or_else(|| Some(seat_name()))
}

/// [`run`] with `VISSUE_AGENT` set to `identity`, so a ballot or a claim is
/// recorded under a persona's name rather than the seat's.
pub fn run_as(bin: &str, args: &[impl AsRef<str>], identity: Option<&str>) -> Result<()> {
    use std::process::{Command, Stdio};
    let path = which::which(bin).with_context(|| format!("{bin} not on PATH"))?;
    let mut cmd = Command::new(path);
    if let Some(who) = identity_or_seat(identity) {
        cmd.env("VISSUE_AGENT", who);
    }
    for a in args {
        cmd.arg(a.as_ref());
    }
    let st = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    // A child that died of a closed pipe was cut off by our own reader
    // going away (`ljos consensus ID | head`); that is not the habitat
    // refusing.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if st.signal() == Some(libc::SIGPIPE) {
            return Ok(());
        }
    }
    if !st.success() {
        bail!("{bin} exited {st}");
    }
    Ok(())
}

/// What a habitat printed, kept for a caller that has to hand it on. A
/// non-zero exit is an error carrying stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub stdout: String,
    pub stderr: String,
}

pub fn run_captured(bin: &str, args: &[impl AsRef<str>]) -> Result<Said> {
    run_captured_as(bin, args, None)
}

/// [`run_captured`] with `VISSUE_AGENT` set to `identity`, for a tracker
/// write whose output the caller has to hand on. `None` leaves the
/// environment as it is.
pub fn run_captured_as(
    bin: &str,
    args: &[impl AsRef<str>],
    identity: Option<&str>,
) -> Result<Said> {
    use std::process::{Command, Stdio};
    let path = which::which(bin).with_context(|| format!("{bin} not on PATH"))?;
    let mut cmd = Command::new(path);
    if let Some(who) = identity {
        cmd.env("VISSUE_AGENT", who);
    }
    for a in args {
        cmd.arg(a.as_ref());
    }
    let out = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("{bin}: could not start"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        let why = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        bail!("{bin} exited {}: {why}", out.status);
    }
    Ok(Said { stdout, stderr })
}

pub fn card_paths(dir: &Path) -> Vec<PathBuf> {
    CARD_NAMES.iter().map(|n| dir.join(n)).collect()
}

/// One typed finding from an eb-stack campaign state file, flattened to
/// what a seat reads and remembers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: String,
    pub status: String,
    pub class: String,
    pub disposition: String,
    pub stage: String,
    /// The recipe the campaign drives, as its file stem:
    /// `eOn-2.17.10-foss-2026.1`.
    pub recipe: String,
    /// The module whose build failed, when the evidence names one:
    /// `GCCcore-15.2.0`, `gettext-0.26-GCCcore-15.2.0`. A campaign fails in
    /// its dependencies far more often than in the recipe it drives.
    pub module: String,
    pub summary: String,
    /// The last error line the evidence carries, else the summary.
    pub error: String,
    /// The resolution's action, when it is resolved.
    pub action: String,
    pub changes: Vec<String>,
}

/// A campaign state file: the package it builds, the target, its findings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Campaign {
    pub package: String,
    pub version: String,
    pub target: String,
    pub status: String,
    pub attempts: u64,
    pub findings: Vec<Finding>,
}

fn recipe_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// The line a reader recognises the failure by: the last line of the
/// evidence that names an error, else the summary.
fn error_line(evidence: &str, summary: &str) -> String {
    let lower = |l: &str| l.to_ascii_lowercase();
    evidence
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| {
            let l = lower(l);
            l.contains("error") || l.contains("fatal") || l.contains("failed")
        })
        .rfind(|l| !l.starts_with("srun:"))
        .map(str::to_string)
        .unwrap_or_else(|| summary.to_string())
}

/// The module EasyBuild was installing when it stopped: `ERROR:
/// Installation of X.eb failed` names it; else the last `== building and
/// installing NAME/VERSION...` line does.
fn failed_module(evidence: &str) -> Option<String> {
    let installation = evidence.lines().rev().find_map(|l| {
        let rest = l.split("Installation of ").nth(1)?;
        let eb = rest.split(".eb failed").next()?;
        // `.eb` is already off; a stem call here would take a version's
        // last component for an extension.
        let name = eb.rsplit('/').next()?;
        (!name.is_empty() && !name.contains(' ')).then(|| name.to_string())
    });
    installation.or_else(|| {
        evidence.lines().rev().find_map(|l| {
            let rest = l.trim().strip_prefix("== building and installing ")?;
            let name = rest.trim_end_matches('.').trim();
            (!name.is_empty()).then(|| name.replacen('/', "-", 1))
        })
    })
}

/// What EasyBuild said after naming the module, else the whole line.
fn error_reason(error: &str) -> &str {
    error
        .split(".eb failed: ")
        .nth(1)
        .unwrap_or(error)
        .trim_start_matches("ERROR: ")
}

fn text_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Read an eb-stack campaign state (`campaign.json`).
///
/// # Errors
///
/// The file is missing, not JSON, or not a campaign state.
pub fn read_campaign(state: &Path) -> Result<Campaign> {
    let text = std::fs::read_to_string(state)
        .with_context(|| format!("findings: cannot read {}", state.display()))?;
    let doc: Value = serde_json::from_str(&text)
        .with_context(|| format!("findings: {} is not JSON", state.display()))?;
    let rows = doc
        .get("findings")
        .and_then(Value::as_array)
        .with_context(|| format!("findings: {} has no findings list", state.display()))?;
    let findings = rows
        .iter()
        .map(|f| {
            let summary = text_of(f, "summary");
            let resolution = f.get("resolution");
            let evidence = text_of(f, "evidence");
            Finding {
                id: text_of(f, "id"),
                status: text_of(f, "status"),
                class: text_of(f, "class"),
                disposition: text_of(f, "disposition"),
                stage: text_of(f, "stage"),
                recipe: recipe_stem(&text_of(f, "recipe")),
                module: failed_module(&evidence).unwrap_or_default(),
                error: error_line(&evidence, &summary),
                summary,
                action: resolution.map(|r| text_of(r, "action")).unwrap_or_default(),
                changes: resolution
                    .and_then(|r| r.get("changes"))
                    .and_then(Value::as_array)
                    .map(|c| {
                        c.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();
    Ok(Campaign {
        package: text_of(&doc, "package"),
        version: text_of(&doc, "version"),
        target: text_of(&doc, "target"),
        status: text_of(&doc, "status"),
        attempts: doc.get("attempts").and_then(Value::as_u64).unwrap_or(0),
        findings,
    })
}

/// The automatic resolution a campaign writes when a later attempt got
/// past the stage: not a lesson, nothing was learned about the recipe.
fn superseded_by_retry(f: &Finding) -> bool {
    f.status == "superseded" || f.action.contains("superseded this finding")
}

/// At most `n` words, with the pack's sentence marks taken out so the
/// lesson stays two sentences.
fn clip_words(text: &str, n: usize) -> String {
    // A stop inside a word (`scc.h`, `2.17.10`) is not a sentence mark; an
    // ellipsis (`'make ...'`) is EasyBuild eliding a command and goes.
    let text = text.replace(" ...", "").replace("...", "");
    let chars: Vec<char> = text.chars().collect();
    let mut flat = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let ends_word = chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        flat.push(match c {
            '.' | '!' | '?' | ';' if ends_word => ',',
            '\n' | '\t' => ' ',
            c => c,
        });
    }
    let words: Vec<&str> = flat.split_whitespace().collect();
    let mut out = words[..words.len().min(n)].join(" ");
    while out.ends_with([',', ':', ' ']) {
        out.pop();
    }
    out
}

/// The lesson a finding leaves: what failed where, then the fix, or that a
/// later attempt got past it. Two short sentences; the pack refuses more,
/// and refuses hard prose.
#[must_use]
pub fn finding_lesson(campaign: &Campaign, f: &Finding) -> String {
    let what = clip_words(error_reason(&f.error), 10);
    let subject = if f.module.is_empty() {
        f.recipe.clone()
    } else if f.module == f.recipe {
        f.module.clone()
    } else {
        format!("{} for {}", f.module, f.recipe)
    };
    let mut first = format!(
        "{subject} on {}: {} failed in the {} step",
        campaign.target, f.class, f.stage
    );
    if !what.is_empty() && what != f.summary {
        first.push_str(&format!(" with {what}"));
    }
    first.push('.');
    if superseded_by_retry(f) {
        return format!("{first} A later attempt got past it.");
    }
    let mut fix = clip_words(&f.action, 14);
    if !f.changes.is_empty() {
        let files: Vec<String> = f
            .changes
            .iter()
            .map(String::as_str)
            .map(recipe_stem)
            .collect();
        fix.push_str(&format!(" in {}", files.join(", ")));
    }
    if fix.is_empty() {
        first
    } else {
        format!("{first} Fix: {fix}.")
    }
}

/// The entities a finding's lesson is about, so a later cue on the
/// recipe, the package or the failure class activates it.
fn finding_entities(campaign: &Campaign, f: &Finding) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for stem in [&f.module, &f.recipe] {
        if stem.is_empty() || out.contains(stem) {
            continue;
        }
        out.push(stem.clone());
        if let Some(name) = stem.split('-').next() {
            if !name.is_empty() && name != stem && !out.iter().any(|e| e == name) {
                out.push(name.to_string());
            }
        }
    }
    if !campaign.package.is_empty() {
        out.push(campaign.package.clone());
    }
    out.push(f.class.clone());
    out.dedup();
    out
}

/// One line per finding: id, status, class, stage, recipe, then the fix
/// or the summary.
#[must_use]
pub fn format_findings(campaign: &Campaign) -> String {
    let mut out = format!(
        "{} {} on {}: {} after {} attempt{}, {} finding{}\n",
        campaign.package,
        campaign.version,
        campaign.target,
        campaign.status,
        campaign.attempts,
        if campaign.attempts == 1 { "" } else { "s" },
        campaign.findings.len(),
        if campaign.findings.len() == 1 {
            ""
        } else {
            "s"
        },
    );
    for f in &campaign.findings {
        let tail = if f.action.is_empty() {
            f.summary.clone()
        } else {
            format!("fix: {}", f.action)
        };
        out.push_str(&format!(
            "{}\t{}\t{}/{}\t{}\t{}\t{}\n",
            f.id,
            f.status,
            f.class,
            f.disposition,
            f.stage,
            if f.module.is_empty() {
                &f.recipe
            } else {
                &f.module
            },
            tail
        ));
    }
    out
}

/// What `remember_findings` did with one finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    pub id: String,
    pub lesson: String,
    /// The pack's answer: the atom id, `held` when the pack already had
    /// it, `skipped` for a retry supersession, else the refusal.
    pub result: String,
}

/// Write one lesson per finding a person or a seat resolved (every
/// finding with `all`), cite the state file on the issue when one is
/// named, and say what happened to each.
///
/// # Errors
///
/// The state cannot be read, or the pack is down. A refusal of one lesson
/// is reported in its row, not returned.
pub fn remember_findings(state: &Path, issue: Option<&str>, all: bool) -> Result<Vec<Remembered>> {
    let campaign = read_campaign(state)?;
    let client = pack()?;
    let workspace = client.workspace();
    let mut out = Vec::new();
    for f in &campaign.findings {
        if !all && superseded_by_retry(f) {
            out.push(Remembered {
                id: f.id.clone(),
                lesson: String::new(),
                result: "skipped: a later attempt got past it, nothing was learned".into(),
            });
            continue;
        }
        if !all && f.status != "resolved" {
            out.push(Remembered {
                id: f.id.clone(),
                lesson: String::new(),
                result: format!("skipped: {}", f.status),
            });
            continue;
        }
        let lesson = finding_lesson(&campaign, f);
        let mut atom = atom_body("lesson", &lesson, &workspace);
        add_entities(&mut atom, finding_entities(&campaign, f));
        let result = match client.post_atom(&atom) {
            Ok(body) => format!(
                "{}{}",
                body["id"].as_str().unwrap_or("written"),
                revision_note(&body)
            ),
            Err(e) => format!("refused: {e}"),
        };
        out.push(Remembered {
            id: f.id.clone(),
            lesson,
            result,
        });
    }
    if let Some(issue) = issue.map(str::trim).filter(|i| !i.is_empty()) {
        let name = format!(
            "{} {} campaign state on {}, {} after {} attempts",
            campaign.package, campaign.version, campaign.target, campaign.status, campaign.attempts
        );
        let seat = seat_name();
        // The same state file under the same name is the same deed: a
        // second run finds it frozen, and the refusal names the accession.
        let said = match run_captured(
            "deedar",
            &[
                "create",
                "file",
                "--name",
                &name,
                "--path",
                &state.display().to_string(),
                "--agent",
                &seat,
            ],
        ) {
            Ok(said) => said.stdout,
            Err(e) if e.to_string().contains("deed frozen") => e.to_string(),
            Err(e) => return Err(e),
        };
        // `deedar create` prints `id=deed-...` on its first line; an older
        // build printed the accession bare.
        let accession = said
            .split_whitespace()
            .find_map(|w| {
                let at = w.find("deed-")?;
                let tail = &w[at..];
                let end = tail
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                    .unwrap_or(tail.len());
                Some(tail[..end].to_string())
            })
            .filter(|a| a.len() > "deed-".len())
            .context("findings: deedar create printed no accession")?;
        run_captured("vissue", &["deed", issue, "--add", &accession])?;
        let _ = persist_tracker(issue, "cited the campaign state");
        out.push(Remembered {
            id: "state".into(),
            lesson: name,
            result: format!("cited on {issue} as {accession}"),
        });
    }
    Ok(out)
}

#[must_use]
pub fn format_remembered(rows: &[Remembered]) -> String {
    rows.iter()
        .map(|r| {
            if r.lesson.is_empty() {
                format!("{}\t{}\n", r.id, r.result)
            } else {
                format!("{}\t{}\n\t{}\n", r.id, r.result, r.lesson)
            }
        })
        .collect()
}

/// One module of a bump bundle as the tracker will hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BumpRow {
    /// The issue id, the same on every run: a hash of the module and the
    /// generation under the project.
    pub id: String,
    /// The module as EasyBuild names it: `CMake-4.2.1-GCCcore-15.2.0`.
    pub module: String,
    /// The recipe path the lock names, when it does.
    pub recipe: String,
    /// The modules this one is built after, by issue id.
    pub blockers: Vec<String>,
    /// What this run did: `made`, `held` (it existed), or `would make`.
    pub result: String,
}

/// The stem of an EasyBuild module: `name-version[-toolchain-version]`.
fn module_stem(name: &str, version: &str, toolchain: Option<(&str, &str)>) -> String {
    match toolchain {
        Some((tn, tv)) if !tn.is_empty() && tn != "system" => {
            format!("{name}-{version}-{tn}-{tv}")
        }
        _ => format!("{name}-{version}"),
    }
}

/// A deterministic issue id for a module of a generation: the project,
/// then eight base-36 digits of the module and generation hashed.
#[must_use]
pub fn bump_issue_id(project: &str, module: &str, generation: &str) -> String {
    let hex = work_id(&format!("bump:{module}:{generation}"));
    let mut n = u128::from_str_radix(&hex[..24], 16).unwrap_or(0);
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    for _ in 0..8 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    format!("{project}-{}", String::from_utf8(out).unwrap_or_default())
}

/// The name behind a CycloneDX purl `pkg:generic/NAME@==VERSION`.
fn purl_name(purl: &str) -> String {
    purl.rsplit('/')
        .next()
        .unwrap_or(purl)
        .split('@')
        .next()
        .unwrap_or(purl)
        .to_string()
}

/// The plan a bundle implies for the tracker: one row per module the lock
/// builds, blockers along the SBOM's dependency edges. Nothing is written.
///
/// # Errors
///
/// The bundle lacks `locks/default.lock.json` or `package.sbom.cdx.json`,
/// or either is not what eb-stack writes.
pub fn bump_rows(
    bundle: &Path,
    project: &str,
    generation: Option<&str>,
) -> Result<(String, Vec<BumpRow>)> {
    let lock_path = bundle.join("locks").join("default.lock.json");
    let sbom_path = bundle.join("package.sbom.cdx.json");
    let lock: Value = serde_json::from_str(
        &std::fs::read_to_string(&lock_path)
            .with_context(|| format!("bump-plan: cannot read {}", lock_path.display()))?,
    )
    .with_context(|| format!("bump-plan: {} is not JSON", lock_path.display()))?;
    let sbom: Value = serde_json::from_str(
        &std::fs::read_to_string(&sbom_path)
            .with_context(|| format!("bump-plan: cannot read {}", sbom_path.display()))?,
    )
    .with_context(|| format!("bump-plan: {} is not JSON", sbom_path.display()))?;
    let tc = &lock["toolchain"];
    let generation = generation.map(str::to_string).unwrap_or_else(|| {
        format!(
            "{}/{}",
            tc["name"].as_str().unwrap_or("system"),
            tc["version"].as_str().unwrap_or("")
        )
        .trim_end_matches('/')
        .to_string()
    });
    // Every module the lock names, the root package first.
    let mut modules: Vec<(String, String, String)> = Vec::new(); // name, stem, recipe
    let root_name = lock["package"].as_str().unwrap_or("").to_string();
    let root_stem = module_stem(
        &root_name,
        lock["version"].as_str().unwrap_or(""),
        Some((
            tc["name"].as_str().unwrap_or(""),
            tc["version"].as_str().unwrap_or(""),
        )),
    ) + lock["versionsuffix"].as_str().unwrap_or("");
    modules.push((root_name.clone(), root_stem, String::new()));
    // `build` on a lock entry says whether it is a build dependency, not
    // whether it is built: every entry is a module the generation needs.
    for dep in lock["dependencies"].as_array().into_iter().flatten() {
        let name = dep["name"].as_str().unwrap_or("").to_string();
        let dtc = &dep["toolchain"];
        let stem = module_stem(
            &name,
            dep["version"].as_str().unwrap_or(""),
            Some((
                dtc["name"].as_str().unwrap_or(""),
                dtc["version"].as_str().unwrap_or(""),
            )),
        );
        let recipe = dep["easyconfig_path"].as_str().unwrap_or("").to_string();
        if !name.is_empty() && !modules.iter().any(|(n, _, _)| *n == name) {
            modules.push((name, stem, recipe));
        }
    }
    let id_of = |name: &str| -> Option<String> {
        modules
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, stem, _)| bump_issue_id(project, stem, &generation))
    };
    // Edges from the SBOM, by name; only edges between modules the lock builds.
    let mut edges: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for d in sbom["dependencies"].as_array().into_iter().flatten() {
        let from = purl_name(d["ref"].as_str().unwrap_or(""));
        for on in d["dependsOn"].as_array().into_iter().flatten() {
            let to = purl_name(on.as_str().unwrap_or(""));
            if let Some(id) = id_of(&to) {
                edges.entry(from.clone()).or_default().push(id);
            }
        }
    }
    let rows = modules
        .iter()
        .map(|(name, stem, recipe)| BumpRow {
            id: bump_issue_id(project, stem, &generation),
            module: stem.clone(),
            recipe: recipe.clone(),
            blockers: edges.get(name).cloned().unwrap_or_default(),
            result: "would make".into(),
        })
        .collect();
    Ok((generation, rows))
}

/// Put a bundle's modules on the tracker: one child issue per module under
/// `parent`, blockers along the dependency edges, ids the same on every run
/// so a rerun holds what exists and adds what is missing. `vissue ready`
/// then lists the modules a seat can build now, and a sitting refuses the
/// rest until their blockers close.
///
/// # Errors
///
/// The bundle is not readable, or the tracker refuses a create or an edge.
pub fn bump_plan(
    bundle: &Path,
    project: &str,
    parent: &str,
    generation: Option<&str>,
    dry: bool,
) -> Result<(String, Vec<BumpRow>)> {
    let (generation, mut rows) = bump_rows(bundle, project, generation)?;
    if dry {
        return Ok((generation, rows));
    }
    for row in &mut rows {
        let exists = tracker_show_json(&row.id).is_ok();
        if exists {
            row.result = "held".into();
        } else {
            let title = format!("Bump {} onto {generation}", row.module);
            let body = if row.recipe.is_empty() {
                format!("The bundle at {} names this module. Ladder: recipe check, package bump, lint, then the campaign.", bundle.display())
            } else {
                format!("Recipe {} in the bundle at {}. Ladder: recipe check, package bump, lint, then the campaign.", row.recipe, bundle.display())
            };
            run_captured(
                "vissue",
                &[
                    "create", "-p", project, "--id", &row.id, "--parent", parent, "-t", "task",
                    "--quiet", "--body", &body, &title,
                ],
            )
            .with_context(|| format!("bump-plan: create {} ({})", row.id, row.module))?;
            row.result = "made".into();
        }
    }
    // Edges after every node exists; an edge already held is not an error.
    for row in &rows {
        let held: Vec<String> = tracker_show_json(&row.id)
            .ok()
            .and_then(|v| v["blocked_by"].as_array().cloned())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        for dep in &row.blockers {
            if held.iter().any(|h| h == dep) {
                continue;
            }
            run_captured("vissue", &["update", &row.id, "--block", dep])
                .with_context(|| format!("bump-plan: {} --block {dep}", row.id))?;
        }
    }
    // Every module lands in one project file; one persist carries them all.
    if let Some(first) = rows.first() {
        let _ = persist_tracker(&first.id, "planned the bump");
    }
    Ok((generation, rows))
}

#[must_use]
pub fn format_bump_rows(generation: &str, rows: &[BumpRow]) -> String {
    let mut out = format!(
        "{} module{} onto {generation}\n",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    );
    for r in rows {
        out.push_str(&format!(
            "{}\t{}\t{}\tafter {}\n",
            r.id,
            r.result,
            r.module,
            if r.blockers.is_empty() {
                "nothing".to_string()
            } else {
                r.blockers.join(" ")
            }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    /// The tests that set or read the process environment take this lock:
    /// cargo runs tests on threads, and one process has one environment.
    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
        ENV.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A root that kept its tilde is the home one.
    #[test]
    fn a_tilde_tracker_root_expands_against_home() {
        use super::expand_leading_tilde as x;
        assert_eq!(x("~/vault", "/home/s"), Some("/home/s/vault".into()));
        assert_eq!(x("~", "/home/s/"), Some("/home/s".into()));
        assert_eq!(x("/abs/vault", "/home/s"), None);
        assert_eq!(x("~other/vault", "/home/s"), None);
    }

    /// A slow pre-push hook does not hold the sitting: the push outlives the
    /// wait and the line says so; a quick one reports the push.
    #[test]
    fn a_slow_tracker_push_finishes_in_the_background() {
        let _env = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let (root, remote, hooks) = (
            dir.path().join("work"),
            dir.path().join("remote.git"),
            dir.path().join("hooks"),
        );
        let git = |cwd: &std::path::Path, args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(args)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        };
        std::fs::create_dir_all(root.join("Software/probe")).unwrap();
        std::fs::create_dir_all(&hooks).unwrap();
        git(
            dir.path(),
            &["init", "-q", "--bare", remote.to_str().unwrap()],
        );
        git(&root, &["init", "-q"]);
        for (k, v) in [
            ("user.email", "seat@example.invalid"),
            ("user.name", "seat"),
            ("core.hooksPath", hooks.to_str().unwrap()),
        ] {
            git(&root, &["config", k, v]);
        }
        let hook = hooks.join("pre-push");
        std::fs::write(&hook, "#!/bin/sh\nsleep 4\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let issues = root.join("Software/probe/issues.org");
        let heading = "* TODO [#C] Probe\n:PROPERTIES:\n:ID:         probe-c3d4\n:END:\n";
        std::fs::write(&issues, heading).unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "seed"]);
        git(
            &root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        std::fs::write(&hook, "#!/bin/sh\nexit 0\n").unwrap();
        git(&root, &["push", "-q", "-u", "origin", "HEAD"]);
        std::fs::write(&hook, "#!/bin/sh\nsleep 4\n").unwrap();
        std::env::set_var("VISSUE_ROOT", &root);
        std::env::set_var("VISSUE_NO_ROUTE", "1");
        std::env::remove_var("ISSUE_ROOT");
        std::env::remove_var("LJOS_TRACKER_GIT");
        std::env::set_var("LJOS_TRACKER_PUSH_WAIT", "1");
        std::env::set_var("XDG_RUNTIME_DIR", dir.path());

        std::fs::write(&issues, heading.replace("TODO", "STARTED")).unwrap();
        let started = std::time::Instant::now();
        let said = super::persist_tracker("probe-c3d4", "claimed");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "{said}"
        );
        assert!(said.contains("still running after 1s"), "{said}");

        std::thread::sleep(std::time::Duration::from_secs(5));
        std::fs::write(&hook, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(&issues, heading.replace("TODO", "DONE")).unwrap();
        std::env::set_var("LJOS_TRACKER_PUSH_WAIT", "10");
        let said = super::persist_tracker("probe-c3d4", "finished");
        assert!(said.contains("committed and pushed"), "{said}");
        for var in [
            "VISSUE_ROOT",
            "VISSUE_NO_ROUTE",
            "LJOS_TRACKER_PUSH_WAIT",
            "XDG_RUNTIME_DIR",
        ] {
            std::env::remove_var(var);
        }
    }

    /// A tracker write reaches git: the ticket's file alone is committed, a
    /// clean file is left alone, and the switch turns it off.
    #[test]
    fn a_tracker_write_is_committed_alone() {
        let _env = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            String::from_utf8_lossy(&o.stdout).to_string()
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "seat@example.invalid"]);
        run(&["config", "user.name", "seat"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::create_dir_all(root.join("Software/probe")).unwrap();
        let issues = root.join("Software/probe/issues.org");
        let heading = "* TODO [#C] Probe\n:PROPERTIES:\n:ID:         probe-a1b2\n:END:\n";
        std::fs::write(&issues, heading).unwrap();
        std::fs::write(root.join("other.org"), "one\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "seed"]);
        std::env::set_var("VISSUE_ROOT", root);
        std::env::set_var("VISSUE_NO_ROUTE", "1");
        std::env::remove_var("ISSUE_ROOT");
        std::env::set_var("LJOS_TRACKER_GIT", "commit");
        assert!(super::persist_tracker("probe-a1b2", "claimed").contains("nothing to commit"));

        std::fs::write(&issues, heading.replace("TODO", "STARTED")).unwrap();
        std::fs::write(root.join("other.org"), "two\n").unwrap();
        run(&["add", "other.org"]);
        let said = super::persist_tracker("probe-a1b2", "claimed");
        assert!(
            said.contains("committed chore(issues): probe-a1b2 claimed"),
            "{said}"
        );
        assert_eq!(
            run(&["log", "-1", "--format=%s"]).trim(),
            "chore(issues): probe-a1b2 claimed"
        );
        // Another seat's staged file is not swept into the commit.
        assert_eq!(
            run(&["diff", "--cached", "--name-only"]).trim(),
            "other.org"
        );

        std::fs::write(&issues, heading.replace("TODO", "DONE")).unwrap();
        std::env::set_var("LJOS_TRACKER_GIT", "off");
        assert!(super::persist_tracker("probe-a1b2", "finished").contains("off"));
        for var in ["VISSUE_ROOT", "VISSUE_NO_ROUTE", "LJOS_TRACKER_GIT"] {
            std::env::remove_var(var);
        }
    }

    /// A scratch tracker with no remote still reports the commit: the
    /// default path pushes, and a refused push is a suffix, not silence.
    #[test]
    fn a_tracker_commit_with_no_remote_still_reports_the_commit() {
        let _env = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            String::from_utf8_lossy(&o.stdout).to_string()
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "seat@example.invalid"]);
        run(&["config", "user.name", "seat"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::create_dir_all(root.join("Software/probe")).unwrap();
        let issues = root.join("Software/probe/issues.org");
        let heading = "* TODO [#C] Probe\n:PROPERTIES:\n:ID:         probe-a1b2\n:END:\n";
        std::fs::write(&issues, heading).unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "seed"]);
        std::fs::write(&issues, heading.replace("TODO", "STARTED")).unwrap();
        std::env::set_var("VISSUE_ROOT", root);
        std::env::set_var("VISSUE_NO_ROUTE", "1");
        std::env::remove_var("ISSUE_ROOT");
        std::env::remove_var("LJOS_TRACKER_GIT");
        let said = super::persist_tracker("probe-a1b2", "claimed");
        assert!(
            said.contains("tracker git: committed chore(issues): probe-a1b2 claimed"),
            "{said}"
        );
        assert!(
            said.contains("push refused") || said.contains("not pushed"),
            "a missing remote must still name the commit: {said}"
        );
        assert_eq!(
            run(&["log", "-1", "--format=%s"]).trim(),
            "chore(issues): probe-a1b2 claimed"
        );
        for var in ["VISSUE_ROOT", "VISSUE_NO_ROUTE", "LJOS_TRACKER_GIT"] {
            std::env::remove_var(var);
        }
    }

    /// A fresh host's missing claim graph is a first sitting, not a fault;
    /// any other claimdag refusal still is.
    #[test]
    fn a_claim_graph_nobody_made_yet_is_not_a_fault() {
        let fresh = "claimdag exited exit status: 1: no work graph at /h/claims: the directory does not exist, so nothing has been claimed on this seat. Set CLAIMDAG_DIR";
        assert_eq!(
            super::claim_graph_absent(fresh),
            Some("/h/claims".to_string())
        );
        assert_eq!(
            super::claim_graph_absent("claimdag exited exit status: 1: work.bin is corrupt"),
            None
        );
        assert_eq!(
            super::claim_graph_absent("no work graph at /h/claims: permission denied"),
            None
        );
    }

    /// The tracker row names the root and fails one other seats cannot see.
    #[test]
    fn tracker_row_names_the_root_and_refuses_a_private_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Software")).unwrap();
        let id = |root: &str| format!("vissue 0.16.2\nprotocol: 1\nroot={root}\nprefix=Software\n");
        let root = dir.path().display().to_string();

        let (state, ok) = super::tracker_state(&id(&root), "VISSUE_ROOT=x");
        assert!(ok, "{state}");
        assert!(state.contains(&format!("root={root}")), "{state}");
        assert!(state.contains("from VISSUE_ROOT=x"), "{state}");

        let (state, ok) = super::tracker_state(&id("~/Git/vault"), "VISSUE_ROOT=~/Git/vault");
        assert!(!ok);
        assert!(state.contains("relative root"), "{state}");

        let missing = dir.path().join("gone").display().to_string();
        assert!(!super::tracker_state(&id(&missing), "cwd").1);

        std::fs::remove_dir(dir.path().join("Software")).unwrap();
        let (state, ok) = super::tracker_state(&id(&root), "cwd");
        assert!(!ok);
        assert!(state.contains("no prefix directory"), "{state}");

        assert!(!super::tracker_state("vissue 0.16.1\n", "cwd").1);
    }

    fn git_scratch(root: &std::path::Path) {
        let run = |args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "seat@example.invalid"]);
        run(&["config", "user.name", "seat"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
    }

    /// Two remotes of one tracker with different heads fail the row, and
    /// agreeing again clears it.
    #[test]
    fn tracker_row_fails_when_two_remotes_disagree() {
        let _env = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("work");
        std::fs::create_dir_all(root.join("Software")).unwrap();
        let git = |cwd: &std::path::Path, args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(args)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        };
        for bare in ["origin.git", "mirror.git"] {
            git(dir.path(), &["init", "-q", "--bare", bare]);
        }
        git_scratch(&root);
        std::fs::write(root.join("Software/.keep"), "").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "seed"]);
        for name in ["origin", "mirror"] {
            let url = dir.path().join(format!("{name}.git"));
            git(&root, &["remote", "add", name, url.to_str().unwrap()]);
            git(&root, &["push", "-q", name, "HEAD:refs/heads/main"]);
        }
        git(&root, &["branch", "-q", "-M", "main"]);
        git(&root, &["fetch", "-q", "--all"]);
        git(&root, &["branch", "-q", "-u", "origin/main"]);
        let (state, ok) = super::tracker_git_drift(&root).unwrap();
        assert!(ok, "{state}");
        assert_eq!(
            super::tracker_mirrors(&root, "origin/main").unwrap(),
            vec![("mirror".to_string(), "main".to_string())],
            "a tracker push reaches the mirror too"
        );

        std::fs::write(root.join("Software/.keep"), "one side\n").unwrap();
        git(&root, &["commit", "-qam", "only origin"]);
        git(&root, &["push", "-q", "origin", "main"]);
        git(&root, &["fetch", "-q", "--all"]);
        let (state, ok) = super::tracker_git_drift(&root).unwrap();
        assert!(!ok, "{state}");
        assert!(
            state.contains("mirror/main differs from origin/main"),
            "{state}"
        );

        git(&root, &["push", "-q", "mirror", "main"]);
        git(&root, &["fetch", "-q", "--all"]);
        let (state, ok) = super::tracker_git_drift(&root).unwrap();
        assert!(ok, "{state}");
    }

    /// The tracker row names how many commits origin lacks, and fails when
    /// they have sat through the push wait or the last push was refused.
    #[test]
    fn tracker_row_fails_when_origin_never_got_the_commits() {
        let _env = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let (root, remote) = (dir.path().join("work"), dir.path().join("remote.git"));
        std::fs::create_dir_all(root.join("Software")).unwrap();
        let git = |cwd: &std::path::Path, args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(args)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        };
        git(
            dir.path(),
            &["init", "-q", "--bare", remote.to_str().unwrap()],
        );
        git_scratch(&root);
        std::fs::write(root.join("Software/.keep"), "").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "seed"]);
        git(
            &root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&root, &["push", "-q", "-u", "origin", "HEAD"]);

        let id = |r: &str| format!("vissue 0.16.2\nprotocol: 1\nroot={r}\nprefix=Software\n");
        let root_s = root.display().to_string();
        std::env::set_var("LJOS_TRACKER_PUSH_WAIT", "5");
        std::env::set_var("XDG_RUNTIME_DIR", dir.path());

        let (state, ok) = super::tracker_state(&id(&root_s), "VISSUE_ROOT=x");
        assert!(ok, "{state}");
        assert!(state.contains("0 unpushed"), "{state}");

        std::fs::write(root.join("Software/.keep"), "local\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "ahead"]);
        let (state, ok) = super::tracker_state(&id(&root_s), "VISSUE_ROOT=x");
        assert!(ok, "a commit younger than the wait stays healthy: {state}");
        assert!(state.contains("1 unpushed"), "{state}");

        std::env::set_var("LJOS_TRACKER_PUSH_WAIT", "0");
        let (state, ok) = super::tracker_state(&id(&root_s), "VISSUE_ROOT=x");
        assert!(!ok, "{state}");
        assert!(state.contains("1 unpushed"), "{state}");

        let mut dead = std::process::Command::new("true").spawn().unwrap();
        let dead_pid = dead.id();
        let _ = dead.wait();
        let logs = dir.path().join("ljos");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(
            logs.join(format!("tracker-push-{dead_pid}.log")),
            "remote: pre-push hook declined\nerror: failed to push some refs\n",
        )
        .unwrap();
        let (state, ok) = super::tracker_state(&id(&root_s), "VISSUE_ROOT=x");
        assert!(!ok, "{state}");
        assert!(state.contains("1 unpushed"), "{state}");
        assert!(
            state.contains("last push refused: remote: pre-push hook declined"),
            "{state}"
        );

        for var in ["LJOS_TRACKER_PUSH_WAIT", "XDG_RUNTIME_DIR"] {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn tracker_row_stays_healthy_while_a_background_push_runs() {
        let _env = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let (root, remote) = (dir.path().join("work"), dir.path().join("remote.git"));
        std::fs::create_dir_all(root.join("Software")).unwrap();
        let git = |cwd: &std::path::Path, args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(args)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        };
        git(
            dir.path(),
            &["init", "-q", "--bare", remote.to_str().unwrap()],
        );
        git_scratch(&root);
        std::fs::write(root.join("Software/.keep"), "").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "seed"]);
        git(
            &root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&root, &["push", "-q", "-u", "origin", "HEAD"]);
        std::fs::write(root.join("Software/.keep"), "local\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "ahead"]);

        let mut sleeper = std::process::Command::new("sleep")
            .arg("8")
            .spawn()
            .unwrap();
        let pid = sleeper.id();
        let logs = dir.path().join("ljos");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join(format!("tracker-push-{pid}.log")), "").unwrap();
        std::env::set_var("LJOS_TRACKER_PUSH_WAIT", "0");
        std::env::set_var("XDG_RUNTIME_DIR", dir.path());
        let id = format!(
            "vissue 0.16.2\nprotocol: 1\nroot={}\nprefix=Software\n",
            root.display()
        );
        let (state, ok) = super::tracker_state(&id, "VISSUE_ROOT=x");
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        assert!(ok, "{state}");
        assert!(state.contains("1 unpushed; push still running"), "{state}");
        for var in ["LJOS_TRACKER_PUSH_WAIT", "XDG_RUNTIME_DIR"] {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn a_session_id_occupies_not_the_product_name_on_the_box() {
        let _g = env_guard();
        unsafe {
            std::env::remove_var("VISSUE_AGENT");
            std::env::set_var("LJOS_SEAT", "runner-x");
            std::env::set_var("GROK_SESSION_ID", "01a09b25-ffe9-7972-881a-3cee2ea6efd6");
        }
        let holder = resolve_assignee(None);
        assert_eq!(
            holder, "01a09b25-ffe9-7972-881a-3cee2ea6efd6",
            "the session is the occupancy, not a prefix and not the seat"
        );
        assert_eq!(resolve_assignee(Some("seat")), holder);
        assert_eq!(
            resolve_assignee(Some("runner-x")),
            holder,
            "the process naming itself is omitted"
        );
        assert_eq!(resolve_assignee(Some("alice")), "alice");
        assert_eq!(seat_name(), "runner-x");
        unsafe {
            std::env::remove_var("GROK_SESSION_ID");
            std::env::remove_var("LJOS_SEAT");
        }
    }

    #[test]
    fn two_session_ids_that_share_a_prefix_occupy_different_slots() {
        let _g = env_guard();
        unsafe {
            std::env::remove_var("LJOS_SEAT");
            std::env::remove_var("VISSUE_AGENT");
            std::env::set_var("GROK_SESSION_ID", "01a09b25-aaaa-7972-881a-3cee2ea6efd6");
        }
        let a = resolve_assignee(None);
        unsafe {
            std::env::set_var("GROK_SESSION_ID", "01a09b25-bbbb-7972-881a-3cee2ea6efd6");
        }
        let b = resolve_assignee(None);
        assert_ne!(
            a, b,
            "a shared eight-character prefix is not one conversation"
        );
        assert_eq!(a, "01a09b25-aaaa-7972-881a-3cee2ea6efd6");
        assert_eq!(b, "01a09b25-bbbb-7972-881a-3cee2ea6efd6");
        unsafe {
            std::env::remove_var("GROK_SESSION_ID");
        }
    }

    #[test]
    fn a_named_holder_refusal_still_says_held_by_another() {
        let hold = Hold {
            assignee: "acme".into(),
            seat: "acme".into(),
            pid: 1,
            comm: "ljos".into(),
            since: "2026-01-01T00:00:00.000Z".into(),
        };
        let said = super::held_by_another_message("demo-aaaa", "brio", &hold, "still running");
        assert!(said.contains("held by another"), "{said}");
        assert!(said.contains("acme"), "{said}");
        assert!(said.contains("not by brio"), "{said}");
    }

    /// Two seats on one ticket: LJOS_SEAT plus a distinct session id each.
    #[test]
    fn two_seats_with_distinct_session_ids_are_distinct_holders() {
        let _g = env_guard();
        let dir = std::env::temp_dir().join(format!("ljos-rt-two-seat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let session_keys: Vec<String> = std::env::vars()
            .map(|(k, _)| k)
            .filter(|k| k.ends_with("_SESSION_ID"))
            .collect();
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
            std::env::remove_var("VISSUE_AGENT");
            for k in &session_keys {
                std::env::remove_var(k);
            }
            std::env::set_var("LJOS_SEAT", "acme");
            std::env::set_var("ACME_SESSION_ID", "acme-sess-aaaaaa");
        }
        let a_seat = seat_name();
        let a_holder = resolve_assignee(None);
        unsafe {
            std::env::remove_var("ACME_SESSION_ID");
            std::env::set_var("LJOS_SEAT", "brio");
            std::env::set_var("BRIO_SESSION_ID", "brio-sess-bbbbbb");
        }
        let b_seat = seat_name();
        let b_holder = resolve_assignee(None);
        assert_eq!(a_seat, "acme");
        assert_eq!(b_seat, "brio");
        assert_eq!(a_holder, "acme-sess-aaaaaa");
        assert_eq!(b_holder, "brio-sess-bbbbbb");
        assert_ne!(a_holder, b_holder);
        unsafe {
            std::env::remove_var("LJOS_SEAT");
            std::env::remove_var("BRIO_SESSION_ID");
            std::env::remove_var("ACME_SESSION_ID");
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
    }

    #[test]
    fn occupancy_is_per_issue_so_two_sittings_do_not_unseat() {
        let _g = env_guard();
        unsafe {
            std::env::remove_var("LJOS_SEAT");
            std::env::remove_var("VISSUE_AGENT");
        }
        let holder = resolve_assignee(None);
        let a = occupancy_assignee(None, "ljos-aaaa");
        let b = occupancy_assignee(None, "ljos-bbbb");
        assert_ne!(
            a, b,
            "two issues under one conversation must not share a slot"
        );
        assert_eq!(a, format!("{holder}:ljos-aaaa"), "{a}");
        assert_eq!(b, format!("{holder}:ljos-bbbb"), "{b}");
        assert_eq!(
            occupancy_assignee(Some("alice"), "ljos-aaaa"),
            "alice:ljos-aaaa"
        );
        assert_eq!(
            occupancy_assignee(Some("alice"), "ljos-bbbb"),
            "alice:ljos-bbbb"
        );
    }

    #[test]
    fn doctor_lists_ljos_hud_but_does_not_require_it() {
        assert!(SEAT_BINS
            .iter()
            .any(|(n, c)| *n == "ljos-hud" && *c == "ljos-hud"));
        assert!(!REQUIRED.contains(&"ljos-hud"));
    }

    #[test]
    fn doctor_names_the_session_not_the_default_seat() {
        let _g = env_guard();
        // A runtime directory of its own: a record another process left for
        // this id would name its holder instead.
        let dir = std::env::temp_dir().join(format!("ljos-rt-doctor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
            std::env::remove_var("LJOS_SEAT");
            std::env::remove_var("VISSUE_AGENT");
            std::env::set_var("GROK_SESSION_ID", "01a09b25-ffe9-7972-881a-3cee2ea6efd6");
        }
        let row = format_seat_row();
        assert!(
            row.contains("01a09b25-ffe9-7972-881a-3cee2ea6efd6"),
            "doctor names the whole session: {row}"
        );
        assert!(
            row.contains("GROK_SESSION_ID"),
            "doctor names where the session came from: {row}"
        );
        assert!(!row.contains("the default"), "{row}");
        unsafe {
            std::env::remove_var("GROK_SESSION_ID");
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_shared_name_does_not_occupy_the_whole_host() {
        let _g = env_guard();
        // A pronoun is treated as omitted: the holder is this conversation's,
        // whatever the tree above the test says the seat is. A name that is
        // not a pronoun is a named worker and stands as given.
        let holder = resolve_assignee(None);
        assert_eq!(resolve_assignee(Some("you")), holder);
        assert_eq!(resolve_assignee(Some("seat")), holder);
        assert_eq!(resolve_assignee(Some("agent")), holder);
        assert_ne!(holder, "seat");
        assert_eq!(resolve_assignee(Some("alice")), "alice");
    }

    #[test]
    fn a_reading_supersedes_the_one_before_and_keeps_it_as_was() {
        assert_eq!(parse_every("7d").unwrap(), 7 * 86_400);
        assert_eq!(parse_every("24h").unwrap(), 86_400);
        assert_eq!(parse_every("2w").unwrap(), 14 * 86_400);
        assert_eq!(parse_every("90").unwrap(), 90);
        assert!(parse_every("soon").is_err());
        assert!(parse_every("0d").is_err());
        assert_eq!(
            stamp_after("2026-09-19T23:30:00.000Z", 3_600).as_deref(),
            Some("2026-09-20T00:30:00.000Z")
        );
        assert_eq!(trim_num(0.5790), "0.579");
        assert_eq!(trim_num(12.0), "12");
        assert_eq!(
            habit_text("mab cr all", 0.579, "acc", "job 11793"),
            "habit mab cr all stands at 0.579 acc (job 11793)."
        );
        let first = serde_json::json!({
            "id": "a1", "kind": "habit", "ts": "2026-09-12T10:00:00.000Z",
            "due_at": "2026-09-19T10:00:00.000Z",
            "habit": {"name": "mab cr all", "value": 0.535, "unit": "acc", "source": "11750", "every_s": 604800}
        });
        let second = serde_json::json!({
            "id": "a2", "kind": "habit", "ts": "2026-09-19T10:00:00.000Z",
            "due_at": "2026-09-26T10:00:00.000Z",
            "habit": {"name": "mab cr all", "value": 0.579, "unit": "acc", "source": "11793", "every_s": 604800,
                       "was": 0.535, "was_ts": "2026-09-12T10:00:00.000Z"}
        });
        let other = serde_json::json!({
            "id": "l1", "kind": "lesson", "text": "not a habit", "ts": "2026-09-19T10:00:00.000Z"
        });
        // The pack hands back one live reading a habit; a stale copy sorts out.
        let rows = readings_of(&[first.clone(), other, second]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id.as_deref(), Some("a2"));
        assert_eq!(rows[0].was, Some(0.535));
        let now = "2026-09-20T09:00:00.000Z";
        let line = format_readings(&rows, now);
        assert!(line.starts_with("mab cr all\t0.579 acc\t+0.044 since 0.535 (8 days ago)\tyesterday\tnext reading in 6 days\t11793\n"), "{line}");
        let late = readings_of(&[first]);
        assert!(format_readings(&late, now).contains("next reading late (yesterday)"));
        assert_eq!(format_change(&late[0], now), "first reading");
    }

    #[test]
    fn a_program_is_named_by_its_path_not_its_version() {
        assert!(version_like("2.1.266"));
        assert!(version_like("v18.2.0"));
        assert!(!version_like("acme"));
        // The kernel's short name of a binary installed under a versions
        // directory is the version; the program is the directory above.
        let me = program_name(std::process::id(), "comm");
        assert!(!me.is_empty() && !version_like(&me), "{me}");
    }

    #[test]
    fn a_hit_names_the_seat_that_wrote_it_only_when_that_is_another() {
        let ents = vec!["seat:brio".to_string(), "habit:x".to_string()];
        assert_eq!(other_seat(&ents, "acme-cli").as_deref(), Some("brio"));
        assert_eq!(other_seat(&ents, "brio"), None);
        assert_eq!(other_seat(&["habit:x".to_string()], "brio"), None);
    }

    #[test]
    fn two_session_ids_that_share_a_prefix_take_two_slots() {
        let a = session_tag("01a09b25-ffe9-7972-881a-3cee2ea6efd6");
        let b = session_tag("01a09b25-ffe9-7972-881a-3cee2ea6efd7");
        assert_ne!(a, b);
        assert_eq!(a.len(), 10);
        assert_eq!(a, session_tag(" 01a09b25-ffe9-7972-881a-3cee2ea6efd6 "));
    }

    /// Two conversations started from one terminal share the line editor's
    /// id; each finds its own server's record, never the other's.
    #[test]
    fn a_record_from_another_conversation_is_not_this_ones() {
        let ble = "1000000000.000001/4242".to_string();
        let me = "01a09b25-ffe9-7972-881a-000000000001".to_string();
        let other = "01a09b25-ffe9-7972-881a-000000000002".to_string();
        let mine = vec![ble.clone(), me.clone()];
        let theirs = format!("acme-cli\nsess-other\nids\t{ble}\t{other}\n");
        assert!(super::record_for(&theirs, &mine, "t".into()).is_none());
        let ours = format!("acme-cli\nsess-mine\nids\t{ble}\t{me}\n");
        assert_eq!(
            super::record_for(&ours, &mine, "t".into()).unwrap().holder,
            "sess-mine"
        );
        // A shell that adds an id of its own still finds its server's record.
        let shell = vec![ble.clone(), me.clone(), "9f9f9f9f-extra".into()];
        assert!(super::record_for(&ours, &shell, "t".into()).is_some());
        // A record from before the ids line is taken as it stands.
        assert!(super::record_for("acme-cli\nsess-old\n", &mine, "t".into()).is_some());
    }

    #[test]
    fn the_host_row_reads_oom_kills_and_this_logins_servers() {
        assert_eq!(
            parse_oom_kills("pgfault 12\noom_kill 43\nnr_free_pages 1\n"),
            Some(43)
        );
        assert_eq!(parse_oom_kills("pgfault 12\n"), None);
        assert_eq!(
            status_field("Name:\tx\nVmRSS:\t  2692 kB\n", "VmRSS:").as_deref(),
            Some("2692")
        );
        let row = host_row();
        assert_eq!(row.name, "host");
        assert!(row.state.contains("ljos-mcp"), "{}", row.state);
    }

    #[test]
    fn a_library_default_client_name_is_not_a_seat() {
        assert_eq!(seat_for_client("Acme CLI"), "acme-cli");
        for library in ["mcp", "MCP", "mcp-client"] {
            let seat = seat_for_client(library);
            assert!(
                !LIBRARY_CLIENT_NAMES.contains(&seat.as_str()) || ancestry().is_empty(),
                "{library} named the seat {seat}"
            );
        }
    }

    #[test]
    fn a_runner_started_inside_another_keeps_its_own_holder() {
        let _g = env_guard();
        let dir = std::env::temp_dir().join(format!("ljos-nest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
            std::env::set_var("ACME_SESSION_ID", "01a09b25-1111-7972-881a-3cee2ea6efd6");
        }
        let parent = announce_seat("Acme CLI", 5151);
        // The child inherits the parent's id and connects under its own name.
        let child = announce_seat("Brio Agent", 5252);
        assert_eq!(child.seat, "brio-agent");
        assert_ne!(child.holder, parent.holder);
        assert_eq!(
            seat_from_session_records()
                .expect("the parent's record")
                .holder,
            parent.holder,
            "the child leaves the parent's record alone"
        );
        retire_seat(5252);
        assert_eq!(
            seat_from_session_records()
                .expect("still the parent's")
                .holder,
            parent.holder,
            "the child's exit does not take the parent's record"
        );
        retire_seat(5151);
        assert!(seat_from_session_records().is_none());
        unsafe {
            std::env::remove_var("ACME_SESSION_ID");
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_thread_named_on_a_call_holds_as_its_shells_do() {
        let _g = env_guard();
        let dir = std::env::temp_dir().join(format!("ljos-thread-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
        assert!(runner_session_var("ACME_THREAD_ID", "0199a1b2-c3d4"));
        assert!(!runner_session_var("ACME_THREAD_ID", "short"));
        // No shell has sat yet: the thread id is the holder, and recorded.
        let first = seat_for_thread("0199a1b2-aaaa-thread");
        assert_eq!(first.holder, "0199a1b2-aaaa-thread");
        let text = std::fs::read_to_string(session_record_path("0199a1b2-aaaa-thread")).unwrap();
        assert_eq!(
            holder_naming(&text, "0199a1b2-aaaa-thread").as_deref(),
            Some("0199a1b2-aaaa-thread")
        );
        // A shell of the thread sat first: the call takes the shell's holder.
        let shell = Seat {
            seat: "acme".into(),
            holder: "sess-shellfirst".into(),
            source: String::new(),
        };
        write_record_ids(
            &session_record_path("0199a1b2-bbbb-thread"),
            &shell,
            &["line-editor-id".into(), "0199a1b2-bbbb-thread".into()],
        );
        assert_eq!(
            seat_for_thread("0199a1b2-bbbb-thread").holder,
            "sess-shellfirst"
        );
        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_shell_with_one_more_session_variable_finds_the_servers_record() {
        let _g = env_guard();
        let dir = std::env::temp_dir().join(format!("ljos-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
            std::env::set_var("ACME_SESSION_ID", "01a09b25-ffe9-7972-881a-3cee2ea6efd6");
        }
        let server = announce_seat("Acme CLI", 4242);
        assert_eq!(server.seat, "acme-cli");
        // The shell's line editor stamps its own id; the shared one still
        // finds the record, and the holder is the server's.
        unsafe {
            std::env::set_var(
                "AAA_LINE_EDITOR_SESSION_ID",
                "9f9f9f9f-0000-0000-0000-000000000000",
            );
        }
        let shell = seat_from_session_records().expect("the shared id finds the record");
        assert_eq!(shell.holder, server.holder);
        assert_eq!(shell.seat, server.seat);
        retire_seat(4242);
        assert!(seat_from_session_records().is_none());
        unsafe {
            std::env::remove_var("ACME_SESSION_ID");
            std::env::remove_var("AAA_LINE_EDITOR_SESSION_ID");
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_ne!(session_tag("01a09b25-aaaa"), session_tag("01a09b25-bbbb"));
    }

    #[test]
    fn a_panel_seats_the_personas_that_speak_to_the_issue() {
        let mk = |name: &str, about: &[&str]| Persona {
            name: name.into(),
            anchor: 0.5,
            view: String::new(),
            entities: about.iter().map(|s| (*s).to_string()).collect(),
        };
        let all = vec![
            mk("reviewer", &["docs"]),
            mk("cuda", &["gpu", "kernels"]),
            mk("reader", &[]),
        ];
        let docs = personas_speaking_to(&all, &["Docs".to_string(), "site".to_string()]);
        assert_eq!(
            docs.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["reviewer"]
        );
        let nobody = personas_speaking_to(&all, &["fortran".to_string()]);
        assert_eq!(
            nobody.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["reader"],
            "no domain match seats only personas with no domains"
        );
        let specialists = vec![mk("reviewer", &["docs"]), mk("cuda", &["gpu"])];
        assert!(personas_speaking_to(&specialists, &["fortran".to_string()]).is_empty());
        let mut merger = mk("merger", &["git"]);
        merger.view = "Reads a merge for the writer it silently drops.".into();
        let mut other = mk("other", &["gpu"]);
        other.view = "Wants the kernel to be fast.".into();
        let by_view = personas_speaking_to(
            &[merger, other],
            &["merge".to_string(), "writers".to_string()],
        );
        assert_eq!(
            by_view.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["merger"],
            "a specialist whose view uses the issue's words is seated"
        );
    }

    #[test]
    fn a_client_name_is_one_seat_however_it_is_spelt() {
        assert_eq!(seat_slug("Acme CLI"), "acme-cli");
        assert_eq!(seat_slug("acme_cli/1.2"), "acme-cli-1-2");
        assert_eq!(seat_slug("  --  "), "runner");
        assert_eq!(conversation_tag(4242), "39u");
        assert_eq!(conversation_tag(0), "0");
    }

    #[test]
    fn the_server_leaves_a_record_a_shell_below_the_runner_reads() {
        let dir = std::env::temp_dir().join(format!("ljos-seat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The record path is pure in the directory, so build it the way the
        // server does and read it back the way a shell does.
        let path = dir.join("ljos").join("seat-4242");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let seat = Seat::tagged(
            seat_slug("Acme CLI"),
            &conversation_tag(4242),
            "test".to_string(),
        );
        std::fs::write(&path, format!("{}\n{}\n", seat.seat, seat.holder)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("acme-cli"));
        assert_eq!(lines.next(), Some("acme-cli-39u"));
        assert_eq!(
            format_seat(&seat),
            "seat\tacme-cli\nholder\tacme-cli-39u\nsource\ttest\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_record_weighs_a_voter_by_what_it_got_right() {
        let ballots = vec![
            ("a".to_string(), "ship".to_string()),
            ("b".to_string(), "ship".to_string()),
            ("c".to_string(), "hold".to_string()),
        ];
        let (rows, records) =
            learn_record(&ballots, "ship", &std::collections::BTreeMap::new(), &[]).unwrap();
        assert_eq!(records["a"], (1.0, 0.0));
        assert_eq!(records["c"], (0.0, 1.0));
        let w = |to: &str| rows.iter().find(|r| r.to == to).unwrap().weight;
        assert_eq!(w("a"), 1.0, "a right voter stands at one");
        assert!(w("c") < w("a"), "a wrong voter stands lower");
        assert_eq!(rows.len(), 6, "complete over the voters");
        // The record accumulates: a second outcome against c lowers it further.
        let (rows2, records2) = learn_record(&ballots, "ship", &records, &[]).unwrap();
        assert_eq!(records2["c"], (0.0, 2.0));
        let w2 = |to: &str| rows2.iter().find(|r| r.to == to).unwrap().weight;
        assert!(w2("c") <= w("c"));
        assert!(learn_record(&ballots, "  ", &records, &[]).is_err());
        // Records are read back off trust atoms, latest first.
        let atoms = vec![
            serde_json::json!({"kind": "trust", "from": "a", "to": "c", "weight": 0.2, "hits": 1.0, "misses": 3.0, "ts": "2026-09-13T01:00:00Z"}),
            serde_json::json!({"kind": "trust", "from": "b", "to": "c", "weight": 0.5, "hits": 1.0, "misses": 1.0, "ts": "2026-09-12T01:00:00Z"}),
        ];
        assert_eq!(records_from_atoms(&atoms)["c"], (1.0, 3.0));
    }

    #[test]
    fn a_correction_is_nudged_once_a_session_and_only_on_a_prompt() {
        let _g = env_guard();
        // The seen file lives under the runtime directory.
        let dir = std::env::temp_dir().join(format!("ljos-corr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
        let prompt = HookCall {
            event: "UserPromptSubmit".into(),
            cue: "Do you not remember to use uv for scripts?".into(),
            session: Some("corr-test".into()),
            shape: HookShape::Asks,
        };
        let first = correction_nudge(&prompt).expect("a correction is nudged");
        assert!(first.contains("ljos prefer"), "{first}");
        assert!(correction_nudge(&prompt).is_none(), "once a session");
        let tool = HookCall {
            event: "PreToolUse".into(),
            cue: "you should have used uv".into(),
            session: Some("corr-test".into()),
            shape: HookShape::Asks,
        };
        assert!(
            correction_nudge(&tool).is_none(),
            "tool calls are not prompts"
        );
        let plain = HookCall {
            event: "UserPromptSubmit".into(),
            cue: "add the timeline verb".into(),
            session: Some("corr-test-2".into()),
            shape: HookShape::Asks,
        };
        assert!(correction_nudge(&plain).is_none());
    }

    #[test]
    fn a_subagent_is_told_its_parents_issue_and_held_once_at_stop() {
        let grok = r#"{"hookEventName":"subagent_stop","sessionId":"child","subagentType":"explore","stopHookActive":false}"#;
        assert_eq!(
            hook_subagent(grok),
            (Some("explore".into()), false, String::new())
        );
        let shared = r#"{"hook_event_name":"SubagentStop","session_id":"p","agent_id":"a1","agent_type":"review","stop_hook_active":true}"#;
        assert_eq!(
            hook_subagent(shared),
            (Some("review".into()), true, "a1".into())
        );
        assert_eq!(hook_subagent(r#"{"hook_event_name":"Stop"}"#).0, None);
        let brief = subagent_brief("explore", "acme-12ab", true);
        assert!(
            brief.contains("Do not open a sitting") && brief.contains("ljos vote acme-12ab"),
            "{brief}"
        );
        let decide = subagent_stop_reason("explore", Some("acme-12ab"), true, false).unwrap();
        assert!(
            decide.contains("decision") && decide.contains("--as ROLE"),
            "{decide}"
        );
        let plain = subagent_stop_reason("explore", Some("acme-12ab"), false, false).unwrap();
        assert!(plain.contains("Otherwise stop"), "{plain}");
        assert!(
            subagent_stop_reason("explore", Some("acme-12ab"), true, true).is_none(),
            "held once"
        );
        assert!(
            subagent_stop_reason("explore", None, true, false).is_none(),
            "no issue, no gate"
        );
    }

    #[test]
    fn a_clone_without_the_named_merge_driver_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        assert!(
            tracker_merge_driver_missing(dir.path()).is_none(),
            "no attribute, no row"
        );
        std::fs::write(
            dir.path().join(".gitattributes"),
            "issues.org merge=vissue\n",
        )
        .unwrap();
        let said = tracker_merge_driver_missing(dir.path()).expect("named and missing");
        assert!(said.contains("vissue merge-driver --install"), "{said}");
        git(&[
            "config",
            "merge.vissue.driver",
            "vissue merge-driver %O %A %B %P",
        ]);
        assert!(tracker_merge_driver_missing(dir.path()).is_none());
    }

    #[test]
    fn a_second_commit_lock_waits_for_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ljos-commit.lock");
        let first = CommitLock::acquire(&path);
        assert!(first.0.is_some(), "the lock opens");
        let other = path.clone();
        let started = std::time::Instant::now();
        let waiter = std::thread::spawn(move || {
            let _second = CommitLock::acquire(&other);
            started.elapsed()
        });
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(first);
        let waited = waiter.join().unwrap();
        assert!(
            waited >= std::time::Duration::from_millis(250),
            "{waited:?}"
        );
    }

    #[test]
    fn a_choice_is_sent_to_a_panel_once_a_session() {
        let _g = env_guard();
        let dir = std::env::temp_dir().join(format!("ljos-dec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
        let call = |cue: &str, session: &str, event: &str| HookCall {
            event: event.into(),
            cue: cue.into(),
            session: Some(session.into()),
            shape: HookShape::Asks,
        };
        let prompt = call(
            "should we seal with age or gpg?",
            "dec-test",
            "UserPromptSubmit",
        );
        let first = decision_nudge(&prompt).expect("a choice is nudged");
        assert!(
            first.contains("Options:") && first.contains("--as NAME"),
            "{first}"
        );
        assert!(decision_nudge(&prompt).is_none(), "once a session");
        assert!(decision_nudge(&call("age vs gpg", "dec-test-2", "PreToolUse")).is_none());
        assert!(decision_nudge(&call(
            "add the timeline verb",
            "dec-test-3",
            "UserPromptSubmit"
        ))
        .is_none());
        assert!(
            decision_nudge(&call("go with option 2", "dec-test-4", "UserPromptSubmit")).is_some()
        );
    }

    #[test]
    fn calibration_weights_are_log_odds_with_the_best_at_one() {
        let w = calibration_weights(&[
            ("a".to_string(), 0.9),
            ("b".to_string(), 0.6),
            ("c".to_string(), 0.5),
            ("d".to_string(), 1.0),
        ]);
        let of = |who: &str| w.iter().find(|(n, _)| n == who).unwrap().1;
        assert_eq!(of("d"), 1.0, "a perfect record is the top of the scale");
        // ln(9) / ln(99) = 0.478; ln(1.5) / ln(99) = 0.088
        assert!((of("a") - 0.478).abs() < 0.01, "{}", of("a"));
        assert!((of("b") - 0.088).abs() < 0.01, "{}", of("b"));
        assert!(
            of("a") / of("b") > 5.0,
            "nine in ten outweighs six in ten by more than five"
        );
        assert_eq!(of("c"), TRUST_FLOOR, "chance earns the floor");
    }

    #[test]
    fn a_consolidation_report_names_the_pairs() {
        let body = serde_json::json!({"live": 5, "closed": 1, "applied": false, "pairs": [
            {"old": "a", "old_text": "The default fuse is Borda.", "new": "b", "new_text": "The default fuse is CombMNZ."}
        ]});
        let text = format_consolidation(&body);
        assert!(
            text.starts_with(
                "closes a  The default fuse is Borda.\n    for b  The default fuse is CombMNZ.\n"
            ),
            "{text}"
        );
        assert!(
            text.ends_with(
                "1 of 5 live memories would close; `ljos consolidate --apply` closes them\n"
            ),
            "{text}"
        );
        let applied = format_consolidation(
            &serde_json::json!({"live": 5, "closed": 0, "applied": true, "pairs": []}),
        );
        assert_eq!(applied, "0 of 5 live memories closed\n");
    }

    #[test]
    fn the_hook_keeps_what_two_scorers_agreed_on() {
        let hit = |ballots, of| Hit {
            id: None,
            text: "x".into(),
            score: 1.0,
            kind: "lesson".into(),
            ts: None,
            entities: vec![],
            ballots,
            of,
        };
        assert!(agreed(&hit(Some(2), Some(3))));
        assert!(!agreed(&hit(Some(1), Some(3))));
        assert!(agreed(&hit(Some(1), Some(1))));
        assert!(agreed(&hit(None, None)));
    }

    #[test]
    fn the_generation_is_read_off_a_get_line() {
        let line = "a25a…  claimed  task  unset  gen=2  assignee=69f917124f757277b806e9a0f48c0318  parent=0  x-1";
        assert_eq!(gen_of(line), Some(2));
        assert_eq!(gen_of("deps  -"), None);
        assert_eq!(gen_of("a  ready  task  unset  gen=x"), None);
    }

    #[test]
    fn the_holder_is_read_off_a_get_line() {
        let line = "a25a…  claimed  task  unset  gen=2  assignee=69f917124f757277b806e9a0f48c0318  parent=0  x-1";
        assert_eq!(
            holder_of(line).as_deref(),
            Some("69f917124f757277b806e9a0f48c0318")
        );
        assert_eq!(
            holder_of("a  ready  task  unset  gen=1  assignee=00000000000000000000000000000000"),
            None
        );
        assert_eq!(holder_of("deps  -"), None);
    }

    #[test]
    fn a_registration_carries_the_runners_name() {
        let argv: Vec<String> = ["run", "-e", "LJOS_SEAT={name}", "{server}"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let filled = filled(&argv, Path::new("/x/ljos-mcp"), "runner-a");
        assert_eq!(filled, ["run", "-e", "LJOS_SEAT=runner-a", "/x/ljos-mcp"]);
        assert_eq!(
            identity_or_seat(Some(" reviewer ")).as_deref(),
            Some("reviewer")
        );
    }

    #[test]
    fn a_timeline_reads_every_store_on_the_local_day() {
        let _g = env_guard();
        let before = std::env::var("TZ").ok();
        unsafe { std::env::set_var("TZ", "CET-1CEST,M3.5.0,M10.5.0/3") };
        // 22:28 UTC on the 26th is 00:28 on the 27th in Amsterdam, the day
        // the tracker stamps an issue created then.
        assert_eq!(local_stamp("2026-09-26T22:28:12.170Z"), "2026-09-27T00:28");
        assert_eq!(local_stamp("[2026-09-27 Sun]"), "[2026-09-27 Sun]");
        assert_eq!(local_offset(1_788_566_400), 7200);
        let deed = deed_event("deed-x", "time=1790461680\n", local_offset).unwrap();
        let v = serde_json::json!({"properties": {"CREATED": "[2026-09-27 Sun]"}});
        let mut events = tracker_events(&v);
        events.push(deed);
        let text = format_events(&events, "2026-09-27T00:30:00");
        assert!(text.lines().all(|l| l.contains("\ttoday\t")), "{text}");
        unsafe {
            match before {
                Some(tz) => std::env::set_var("TZ", tz),
                None => std::env::remove_var("TZ"),
            }
        }
    }

    #[test]
    fn a_timeline_merges_the_three_stores_oldest_first() {
        let v = serde_json::json!({
            "properties": {
                "CREATED": "[2026-09-01 Tue]",
                "SCHEDULED": "<2026-02-10 Tue>"
            },
            "claimed_by": "seat",
            "claimed_at": "[2026-09-03 Thu 11:48]",
            "logbook": [
                {"note": "second", "timestamp": "[2026-09-10 Thu 09:00]"},
                {"from_state": "TODO", "to_state": "STARTED", "timestamp": "[2026-09-03 Thu 11:48]"}
            ]
        });
        let mut events = tracker_events(&v);
        events.push(
            deed_event(
                "deed-x",
                "id=deed-x ok\nproducedBy=seat -\ntime=1788566400\n",
                |_| 0,
            )
            .unwrap(),
        );
        events.sort_by(|a, b| (a.days, &a.clock).cmp(&(b.days, &b.clock)));
        let text = format_events(&events, "2026-09-12T00:00:00Z");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 6, "{text}");
        assert!(
            lines[0].contains("tracker\tSCHEDULED <2026-02-10 Tue>"),
            "{}",
            lines[0]
        );
        assert!(
            lines[1].starts_with("2026-09-01 \t11 days ago"),
            "{}",
            lines[1]
        );
        assert!(lines[1].contains("tracker\tcreated"), "{}", lines[1]);
        assert!(
            lines[2].contains("+2 d\ttracker\tclaimed by seat"),
            "{}",
            lines[2]
        );
        assert!(
            lines[3].contains("same day\ttracker\tTODO -> STARTED"),
            "{}",
            lines[3]
        );
        assert!(
            lines[4]
                .starts_with("2026-09-05 00:00\t7 days ago\t+2 d\tdeed\tdeed-x produced by seat -"),
            "{}",
            lines[4]
        );
        assert!(
            lines[5].contains("2 days ago\t+5 d\ttracker\tnote: second"),
            "{}",
            lines[5]
        );
    }

    #[test]
    fn sitting_caps_are_the_protocol_numbers() {
        assert_eq!(SITTING_DUE, 8);
        assert_eq!(SITTING_TIMELINE, 12);
    }

    #[test]
    fn policyd_required_is_the_operator_switch() {
        let _g = env_guard();
        let before = std::env::var_os("POLICYD_REQUIRED");
        std::env::remove_var("POLICYD_REQUIRED");
        assert!(!policyd_required());
        std::env::set_var("POLICYD_REQUIRED", "1");
        assert!(policyd_required());
        std::env::set_var("POLICYD_REQUIRED", "0");
        assert!(!policyd_required());
        match before {
            Some(v) => std::env::set_var("POLICYD_REQUIRED", v),
            None => std::env::remove_var("POLICYD_REQUIRED"),
        }
    }

    #[test]
    fn stamps_of_every_shape_key_the_same() {
        assert_eq!(
            stamp_key(Some("[2026-09-12 Sat 21:54]")),
            stamp_key(Some("2026-09-12T21:54:00.000Z"))
        );
        assert_eq!(stamp_key(Some("[2026-09-12 Sat]")).unwrap().1, "");
        assert_eq!(
            stamp_key(Some("<2026-02-10 Tue>")).map(|k| k.0),
            stamp_key(Some("2026-02-10")).map(|k| k.0)
        );
        assert_eq!(stamp_key(Some("soon")), None);
        assert_eq!(
            civil_of_days(days_of_stamp(Some("2026-09-12")).unwrap()),
            "2026-09-12"
        );
    }

    #[test]
    fn ages_read_as_a_timeline() {
        let now = "2026-09-12T14:00:00.000Z";
        assert_eq!(age_of(Some("2026-09-12T01:00:00.000Z"), now), "today");
        assert_eq!(age_of(Some("2026-09-11T23:59:00.000Z"), now), "yesterday");
        assert_eq!(age_of(Some("2026-09-01T00:00:00.000Z"), now), "11 days ago");
        assert_eq!(age_of(Some("2026-08-01T00:00:00.000Z"), now), "6 weeks ago");
        assert_eq!(
            age_of(Some("2026-03-01T00:00:00.000Z"), now),
            "6 months ago"
        );
        assert_eq!(age_of(Some("2023-09-12T00:00:00.000Z"), now), "3 years ago");
        assert_eq!(age_of(Some("2026-09-13T00:00:00.000Z"), now), "in 1 day");
        assert_eq!(age_of(None, now), "");
        assert_eq!(age_of(Some("card"), now), "");
    }

    #[test]
    fn a_hit_line_carries_kind_and_age() {
        let h = Hit {
            id: Some("a".into()),
            text: " keep the smoke green ".into(),
            score: 1.0,
            kind: "lesson".into(),
            ts: Some("2026-09-10T00:00:00.000Z".into()),
            entities: vec![],
            ballots: None,
            of: None,
        };
        assert_eq!(
            hit_line(&h, "2026-09-12T00:00:00.000Z"),
            "- [lesson, 2 days ago] keep the smoke green"
        );
        let bare = Hit {
            id: None,
            text: "x".into(),
            score: 1.0,
            kind: String::new(),
            ts: None,
            entities: vec![],
            ballots: None,
            of: None,
        };
        assert_eq!(hit_line(&bare, "2026-09-12T00:00:00.000Z"), "- [claim] x");
    }

    /// A hook call is read from the runner's JSON or from plain text, and
    /// the answer is the runner's shape only when there is something to say.
    #[test]
    fn hook_calls_are_read_and_answered_in_the_runners_shape() {
        let tool = hook_call(
            r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo test","description":"run"}}"#,
        );
        assert_eq!(tool.event, "PreToolUse");
        assert_eq!(tool.cue, "cargo test");
        let prompt = hook_call(r#"{"hook_event_name":"UserPromptSubmit","prompt":"fix the fuse"}"#);
        assert_eq!(prompt.cue, "fix the fuse");
        let grok = hook_call(r#"{"hookEventName":"post_tool_use","sessionId":"s1"}"#);
        assert_eq!(grok.event, "PostToolUse");
        assert_eq!(grok.session.as_deref(), Some("s1"));
        hold_hook_context(Some("s1"), "held pack");
        assert_eq!(take_hook_context(Some("s1")), "held pack");
        assert!(take_hook_context(Some("s1")).is_empty());
        let argv = hook_call("rm -rf build");
        assert_eq!(argv.event, "argv");
        assert_eq!(argv.session, None);
        let with_session = hook_call(
            r#"{"session_id":"abc/../x 1","hook_event_name":"PreToolUse","tool_input":{"command":"ls"}}"#,
        );
        assert_eq!(with_session.session.as_deref(), Some("abc/../x 1"));
        assert!(seen_path("abc/../x 1")
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("hook-seen-abcx1"));
        assert_eq!(seen_path("/../"), None);
        assert_eq!(hook_output(&argv, ""), "");
        assert_eq!(hook_output(&argv, "- [lesson] x"), "- [lesson] x\n");
        let out = hook_output(&tool, "- [preference] y");
        let v: Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(
            v["hookSpecificOutput"]["additionalContext"],
            "- [preference] y"
        );
        assert!(
            hook_context(
                &HookCall {
                    event: "argv".into(),
                    cue: "ab".into(),
                    session: None,
                    shape: HookShape::Asks,
                },
                8
            )
            .is_empty(),
            "a cue too short asks nothing"
        );
    }

    /// The injected ids of a session are read back without the nudge marker,
    /// and the seen file goes with the session.
    #[test]
    fn a_sessions_injected_memories_are_read_back_and_cleared() {
        let session = format!("end-test-{}", std::process::id());
        mark_seen(
            Some(&session),
            &["a".to_string(), "due-nudge".to_string(), "b".to_string()],
        );
        let (ids, path) = injected_ids(&session);
        assert_eq!(ids, ["a", "b"]);
        assert!(path.as_ref().is_some_and(|p| p.is_file()));
        // No pack in a unit test: nothing fires, the file still goes.
        let _ = session_end(Some(&session));
        assert!(!path.unwrap().is_file());
        assert_eq!(session_end(None), 0);
    }

    /// The memory hook merges into a runner's hooks file once per event and
    /// is not added twice.
    #[test]
    fn the_memory_hook_is_merged_once() {
        let dir = std::env::temp_dir().join(format!("ljos-hook-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(
            &file,
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"other"}]}]},"theme":"dark"}"#,
        )
        .unwrap();
        let both: Vec<String> = vec!["UserPromptSubmit".into(), "PreToolUse".into()];
        let prompts: Vec<String> = HOOK_EVENTS.iter().map(|e| (*e).to_string()).collect();
        assert_eq!(
            prompts,
            ["UserPromptSubmit", "SessionEnd"],
            "the panel's default, and the session end that wires what it used"
        );
        assert!(!hook_installed(&file, &both));
        let dry = hook_step(&file, &both, true);
        assert!(
            dry.ok && dry.detail.starts_with("would add it on"),
            "{dry:?}"
        );
        let step = hook_step(&file, &both, false);
        assert!(step.ok, "{step:?}");
        assert!(hook_installed(&file, &both));
        let again = hook_step(&file, &both, false);
        assert!(
            again.detail.contains("carries the memory hook on"),
            "{again:?}"
        );
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["theme"], "dark", "the rest of the file is kept");
        assert_eq!(
            v["hooks"]["PreToolUse"].as_array().unwrap().len(),
            2,
            "the other hook stays"
        );
        assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
        // Narrowing to the default drops the seat's tool-call group and
        // leaves the other tool's group alone.
        let narrowed = hook_step(&file, &prompts, false);
        assert!(
            narrowed.detail.contains("drop it from PreToolUse"),
            "{narrowed:?}"
        );
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "other");
        assert!(hook_installed(&file, &prompts));
        assert!(!hook_installed(&file, &both));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rules are globs over the whole line; deny wins over ask; the hook
    /// carries the verdict as the runner's permission decision.
    #[test]
    fn rules_match_the_line_and_the_hook_carries_the_verdict() {
        assert!(glob_matches("rm -rf *", "rm -rf /tmp/x"));
        assert!(!glob_matches("rm -rf *", "ls -la"));
        assert!(glob_matches("*sudo*", "echo hi && sudo reboot"));
        assert!(glob_matches("git push*", "git push origin main"));
        assert!(!glob_matches("git push*", "git pull"));
        let rules = vec![
            Rule {
                pattern: "git push*".into(),
                verdict: "ask".into(),
                reason: "A push is the trust gate.".into(),
            },
            Rule {
                pattern: "*--force*".into(),
                verdict: "deny".into(),
                reason: "Never force push.".into(),
            },
        ];
        assert_eq!(
            verdict_for(&rules, "git push --force").unwrap().verdict,
            "deny"
        );
        assert_eq!(
            verdict_for(&rules, "git push origin x").unwrap().verdict,
            "ask"
        );
        assert!(verdict_for(&rules, "cargo test").is_none());
        let call = hook_call(
            r#"{"hook_event_name":"PreToolUse","tool_input":{"command":"git push --force"}}"#,
        );
        let out = hook_output_ruled(&call, "", verdict_for(&rules, &call.cue));
        let v: Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(v["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("Never force push"));
        assert!(v["hookSpecificOutput"].get("additionalContext").is_none());
        let argv = HookCall {
            event: "argv".into(),
            cue: "git push origin x".into(),
            session: None,
            shape: HookShape::Asks,
        };
        assert!(
            hook_output_ruled(&argv, "", verdict_for(&rules, &argv.cue)).starts_with("ask: A push")
        );
        // grok: camelCase in, a top-level decision out.
        let grok = hook_call(
            r#"{"hookEventName":"pre_tool_use","sessionId":"g-1","toolName":"run_terminal_command","toolInput":{"command":"git push --force"}}"#,
        );
        assert_eq!(grok.shape, HookShape::CamelCase);
        assert_eq!(grok.event, "PreToolUse");
        assert_eq!(grok.cue, "git push --force");
        let v: Value = serde_json::from_str(
            hook_output_ruled(&grok, "", verdict_for(&rules, &grok.cue)).trim(),
        )
        .unwrap();
        assert_eq!(v["decision"], "deny");
        assert!(v["reason"].as_str().unwrap().contains("Never force push"));
        // Lower-case events: the prompt under extra, answers at the top.
        let turn = hook_call(
            r#"{"hook_event_name":"pre_llm_call","tool_name":null,"tool_input":null,"session_id":"h-1","extra":{"user_message":"fix the fuse"}}"#,
        );
        assert_eq!(turn.shape, HookShape::Context);
        assert_eq!(turn.event, "UserPromptSubmit");
        assert_eq!(turn.cue, "fix the fuse");
        let v: Value =
            serde_json::from_str(hook_output_ruled(&turn, "- [lesson] x", None).trim()).unwrap();
        assert_eq!(v["context"], "- [lesson] x");
        assert!(v.get("hookSpecificOutput").is_none());
        let tool = hook_call(
            r#"{"hook_event_name":"pre_tool_call","tool_name":"terminal","tool_input":{"command":"git push origin x"},"session_id":"h-1","extra":{}}"#,
        );
        assert_eq!(tool.event, "PreToolUse");
        let v: Value = serde_json::from_str(
            hook_output_ruled(&tool, "", verdict_for(&rules, &tool.cue)).trim(),
        )
        .unwrap();
        assert_eq!(v["decision"], "block");
        assert!(v["reason"]
            .as_str()
            .unwrap()
            .starts_with("ask the person before running this"));
        assert_eq!(
            hook_call(r#"{"hook_event_name":"on_session_end","session_id":"h-1","extra":{}}"#)
                .event,
            "TurnEnd"
        );
        assert_eq!(
            hook_call(r#"{"hook_event_name":"on_session_finalize","session_id":"h-1","extra":{}}"#)
                .event,
            "SessionEnd"
        );
        // An ask on a runner that cannot ask stops the tool.
        let deny_only = hook_call(
            r#"{"hook_event_name":"PreToolUse","session_id":"c-1","turn_id":"t-1","tool_name":"Bash","tool_input":{"command":"git push origin x"}}"#,
        );
        assert_eq!(deny_only.shape, HookShape::DenyOnly);
        let v: Value = serde_json::from_str(
            hook_output_ruled(&deny_only, "", verdict_for(&rules, &deny_only.cue)).trim(),
        )
        .unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(v["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .starts_with("ask the person before running this: A push"));
        assert!(v.get("decision").is_none());
        let asks = hook_call(
            r#"{"hook_event_name":"PreToolUse","session_id":"k-1","tool_name":"Bash","tool_input":{"command":"git push origin x"}}"#,
        );
        let v: Value = serde_json::from_str(
            hook_output_ruled(&asks, "", verdict_for(&rules, &asks.cue)).trim(),
        )
        .unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "ask");
        let steps = panel_steps("x-1", true, &[], &[]);
        assert!(steps.is_empty());
        let preds = vec![
            Prediction {
                issue: "x-1".into(),
                agent: "a".into(),
                expect: Value::String("ship".into()),
            },
            Prediction {
                issue: "x-1".into(),
                agent: "b".into(),
                expect: serde_json::json!({"ship": 0.6, "hold": 0.4}),
            },
        ];
        let steps = panel_steps("x-1", true, &[row("a", "b", 0.5)], &preds);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].args[0], "surprising");
        assert_eq!(steps[1].args[0], "reputation");
    }

    /// A scoped row applies when the issue is about one of its domains; an
    /// unscoped row applies everywhere; a scoped learn starts from the
    /// unscoped row and leaves it standing.
    #[test]
    fn scoped_rows_apply_to_their_topic_and_learn_writes_in_scope() {
        let everywhere = row("a", "b", 0.9);
        let mut on_docs = row("a", "b", 0.2);
        on_docs.about = vec!["docs".into()];
        let rows = vec![everywhere.clone(), on_docs.clone()];
        let topic = topic_words("Rewrite the docs site");
        assert_eq!(topic, ["docs", "rewrite", "site", "the"]);
        // On the docs topic the scoped row stands in for the unscoped one;
        // elsewhere the unscoped row is the one that applies.
        assert_eq!(rows_about(&rows, &topic), vec![on_docs.clone()]);
        assert_eq!(
            rows_about(&rows, &topic_words("Fix the fuse")),
            vec![everywhere.clone()]
        );

        let ballots = vec![
            ("a".to_string(), "ship".to_string()),
            ("b".to_string(), "hold".to_string()),
        ];
        let learned = learn_about(&ballots, "ship", &rows, 0.5, &["fuse".to_string()]).unwrap();
        let ab = learned
            .iter()
            .find(|r| r.from == "a" && r.to == "b")
            .unwrap();
        assert_eq!(ab.about, ["fuse"]);
        assert!(
            (ab.weight - 0.45).abs() < 1e-9,
            "starts from the unscoped 0.9: {ab:?}"
        );
        let ba = learned
            .iter()
            .find(|r| r.from == "b" && r.to == "a")
            .unwrap();
        assert!((ba.weight - 1.0).abs() < 1e-9, "a was right: {ba:?}");

        // Rows read back keep scoped and unscoped apart, latest per scope.
        let atoms = vec![
            trust_atom(&everywhere, &[], "ws").unwrap(),
            trust_atom(&on_docs, &[], "ws").unwrap(),
        ];
        let mut back = trust_rows(&atoms);
        back.sort_by(|x, y| x.about.cmp(&y.about));
        assert_eq!(back, vec![everywhere, on_docs]);
    }

    /// A persona is a voter with an anchor; the latest atom per name wins and
    /// the anchors go to the settle as one object.
    #[test]
    fn personas_are_latest_per_name_and_anchor_the_settle() {
        let p = Persona {
            name: "reviewer".into(),
            anchor: 0.2,
            view: "Reads for what could break in production.".into(),
            entities: vec!["Release".into()],
        };
        let mut a = persona_atom(&p, "ws").unwrap();
        a["ts"] = Value::String("2026-01-01T00:00:00Z".into());
        let mut later = a.clone();
        later["anchor"] = serde_json::json!(0.4);
        later["ts"] = Value::String("2026-02-01T00:00:00Z".into());
        let got = personas_of(&[a, later]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].anchor, 0.4);
        assert_eq!(got[0].entities, ["release"]);
        assert_eq!(anchors_json(&got), r#"{"reviewer":0.4}"#);
        // A refuted persona listens more next time; a vindicated one does
        // not move; one that did not vote is untouched.
        let ballots = vec![
            ("reviewer".to_string(), "hold".to_string()),
            ("reader".to_string(), "ship".to_string()),
        ];
        let moved = learn_anchors(&got, &ballots, "ship", 0.5);
        assert_eq!(moved.len(), 1);
        assert!(
            (moved[0].anchor - 0.7).abs() < 1e-9,
            "0.4 + 0.6 * 0.5: {moved:?}"
        );
        assert!(learn_anchors(&got, &ballots, "hold", 0.5).is_empty());
        assert!(persona_atom(
            &Persona {
                anchor: 1.5,
                ..p.clone()
            },
            "ws"
        )
        .is_err());
        let steps = consensus_steps_anchored("x-1", true, true, &[], &got).unwrap();
        for step in &steps {
            assert!(
                step.args.contains(&"--susceptibility-of".to_string()),
                "{step:?}"
            );
        }
        // The kind of work sets the dynamics: a broad-audience issue runs
        // bounded confidence on the model crate, and the tracker verb, which
        // has no such model, is left as it was.
        let broad =
            consensus_steps_for("x-1", true, true, &[], &got, &["broad".to_string()]).unwrap();
        assert!(
            broad[0].args.contains(&"--epsilon".to_string()),
            "{:?}",
            broad[0]
        );
        assert!(
            !broad[1].args.contains(&"--epsilon".to_string()),
            "{:?}",
            broad[1]
        );
        assert!(settle_flags_for(&["feature".to_string()]).is_empty());
    }

    /// Playbooks are kind playbook, latest per name, unreviewed; sitting
    /// copies the full body; a second name on a live sitting is refused;
    /// the inbound floor is unscoped.
    #[test]
    fn playbooks_are_latest_per_name_and_stick_until_finish() {
        let _g = env_guard();
        let dir = std::env::temp_dir().join(format!("ljos-playbook-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let before = std::env::var_os("XDG_RUNTIME_DIR");
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
        }
        let shipped = shipped_playbooks();
        let names: Vec<&str> = shipped.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, SHIPPED_PLAYBOOK_NAMES);
        for p in shipped_playbooks() {
            assert!(!p.body.is_empty(), "{}", p.name);
            assert!(
                !p.body.contains("/poteto-mode") && !p.body.contains("poteto-agent"),
                "{}",
                p.name
            );
            let atom = playbook_atom(&p, "ws").unwrap();
            assert_eq!(atom["kind"], "playbook");
            assert_eq!(atom["name"], p.name);
            assert_eq!(atom["text"], p.body);
            assert!(!super::reviewable(&atom), "{}", p.name);
        }
        assert!(playbook_atom(
            &Playbook {
                name: "sit".into(),
                body: "  ".into(),
                models: vec![],
            },
            "ws"
        )
        .is_err());
        let mut a = playbook_atom(
            &Playbook {
                name: "sit".into(),
                body: "first body".into(),
                models: vec![],
            },
            "ws",
        )
        .unwrap();
        a["ts"] = Value::String("2026-01-01T00:00:00Z".into());
        let mut later = a.clone();
        later["text"] = Value::String("second body".into());
        later["ts"] = Value::String("2026-02-01T00:00:00Z".into());
        let got = playbooks_of(&[a, later]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "second body");
        let copy = copy_playbook("proj-1a2b", "sit").unwrap();
        assert!(copy.starts_with("sit\n"), "{copy}");
        assert!(copy.contains("Grade due claims"), "{copy}");
        assert_eq!(bound_playbook("proj-1a2b").as_deref(), Some("sit"));
        let err = bind_playbook("proj-1a2b", "arena").unwrap_err().to_string();
        assert!(err.contains("bound to sit"), "{err}");
        assert!(err.contains("new sitting"), "{err}");
        let again = playbook_opening("proj-1a2b", None).unwrap();
        assert!(again.contains("Grade due claims"), "{again}");
        let blocks = brief_playbook_blocks("proj-1a2b");
        assert!(blocks.contains("== playbook"), "{blocks}");
        assert!(blocks.contains("Grade due claims"), "{blocks}");
        assert!(blocks.contains("== principles"), "{blocks}");
        assert!(blocks.contains("split-fence"), "{blocks}");
        assert!(blocks.contains("== rubric"), "{blocks}");
        assert!(blocks.contains("Ledger intact"), "{blocks}");
        drop_playbook("proj-1a2b");
        assert_eq!(bound_playbook("proj-1a2b"), None);
        let none = playbook_opening("proj-1a2b", None).unwrap();
        assert!(none.contains("none bound"), "{none}");
        assert!(none.contains("panel is refused"), "{none}");
        let err = panel("proj-1a2b", &dir.join("panel"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no playbook bound"), "{err}");
        let p = Persona {
            name: "reviewer".into(),
            anchor: 0.2,
            view: "Reads for what could break.".into(),
            entities: vec!["docs".into()],
        };
        let floor = inbound_floor(&p, "seat").unwrap();
        assert_eq!(floor.from, "seat");
        assert_eq!(floor.to, "reviewer");
        assert!((floor.weight - 1.0).abs() < 1e-9);
        assert!(floor.about.is_empty());
        assert!(inbound_floor(&p, "reviewer").is_none());
        assert!(has_unscoped_inbound(
            std::slice::from_ref(&floor),
            "reviewer",
            "seat"
        ));
        let scoped = Trust {
            about: vec!["docs".into()],
            ..floor
        };
        assert!(!has_unscoped_inbound(
            std::slice::from_ref(&scoped),
            "reviewer",
            "seat"
        ));
        let other = Trust {
            from: "other".into(),
            to: "reviewer".into(),
            weight: 1.0,
            about: Vec::new(),
        };
        assert!(
            !has_unscoped_inbound(std::slice::from_ref(&other), "reviewer", "seat"),
            "a third-party unscoped row is not the seat floor"
        );
        let arena_pb = shipped_playbooks()
            .into_iter()
            .find(|p| p.name == "arena")
            .unwrap();
        let arena = format_playbook_copy(&arena_pb);
        assert!(
            arena.contains("spawn hints (optional): judgment, instruction, fast"),
            "{arena}"
        );
        assert!(arena.contains("ljos vote --as"), "{arena}");
        match before {
            Some(v) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", v) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn playbook_note_latest_wins_and_empty_rest_drops() {
        let v = serde_json::json!({
            "logbook": [
                {"note": "playbook: land", "timestamp": "2026-09-21"},
                {"note": "playbook: sit", "timestamp": "2026-09-20"},
                {"note": "progress", "timestamp": "2026-09-19"}
            ]
        });
        assert_eq!(playbook_name_from_issue(&v).as_deref(), Some("land"));
        let empty = serde_json::json!({"logbook": []});
        assert_eq!(playbook_name_from_issue(&empty), None);
        let dropped = serde_json::json!({
            "logbook": [
                {"note": "playbook:", "timestamp": "2026-09-22T00:00:00Z"},
                {"note": "playbook: sit", "timestamp": "2026-09-21T00:00:00Z"}
            ]
        });
        assert_eq!(playbook_name_from_issue(&dropped), None);
        let undated = serde_json::json!({
            "logbook": [
                {"note": "playbook:"},
                {"note": "playbook: sit"}
            ]
        });
        assert_eq!(
            playbook_name_from_issue(&undated),
            None,
            "newest-first empty rest drops without walking back"
        );
    }

    #[test]
    fn playbook_from_title_matches_a_closed_name_else_sit() {
        assert_eq!(playbook_from_title("Seat playbooks: routing"), "sit");
        assert_eq!(playbook_from_title("x5jz compose: land B"), "land");
        assert_eq!(
            playbook_from_title("Run the company-panel overnight"),
            "company-panel"
        );
        assert_eq!(playbook_from_title("sitting on a ticket"), "sit");
        assert_eq!(playbook_from_title("arena then compose"), "arena");
        assert_eq!(
            playbook_from_title("Benny and poteto-mode"),
            "sit",
            "title-match binds only closed-set tokens"
        );
    }

    #[test]
    fn playbook_among_pack_latest_wins_and_unknown_names_are_refused() {
        let rewritten = Playbook {
            name: "sit".into(),
            body: "rewritten sit body".into(),
            models: vec![],
        };
        let got = playbook_among("sit", std::slice::from_ref(&rewritten)).unwrap();
        assert_eq!(got.body, "rewritten sit body");
        let seed = playbook_among("sit", &[]).unwrap();
        assert!(
            seed.body.contains("Grade due claims"),
            "shipped seed when the pack has no live atom: {}",
            seed.body
        );
        let err = playbook_among("Benny", &[]).unwrap_err().to_string();
        assert!(err.contains("unknown"), "{err}");
        let sneaky = Playbook {
            name: "poteto-mode".into(),
            body: "second roster".into(),
            models: vec![],
        };
        let err = playbook_among("poteto-mode", std::slice::from_ref(&sneaky))
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown"), "{err}");
        assert!(playbook_atom(&sneaky, "ws").is_err());
        assert!(parse_playbook_name("overnight").is_ok());
        assert!(parse_playbook_name("company-panel").is_ok());
        let listed = playbooks_of(&[serde_json::json!({
            "kind": "playbook",
            "name": "Benny",
            "text": "no",
            "ts": "2026-01-01T00:00:00Z"
        })]);
        assert!(listed.is_empty(), "{listed:?}");
        let err = bind_playbook("proj-1a2b", "Benny").unwrap_err().to_string();
        assert!(err.contains("unknown"), "{err}");
    }

    #[test]
    fn sitting_resolves_asked_else_bound_else_title_else_sit() {
        let _g = env_guard();
        let dir =
            std::env::temp_dir().join(format!("ljos-playbook-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let before = std::env::var_os("XDG_RUNTIME_DIR");
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
        }
        assert_eq!(
            resolve_sitting_playbook("proj-1a2b", "Seat playbooks", Some("arena")).unwrap(),
            "arena"
        );
        assert_eq!(
            resolve_sitting_playbook("proj-1a2b", "x5jz compose: land B", None).unwrap(),
            "land"
        );
        assert_eq!(
            resolve_sitting_playbook("proj-1a2b", "Ship the fuse change?", None).unwrap(),
            "sit"
        );
        bind_playbook("proj-1a2b", "sit").unwrap();
        assert_eq!(
            resolve_sitting_playbook("proj-1a2b", "x5jz compose: land B", None).unwrap(),
            "sit",
            "sticky wins over title"
        );
        drop_playbook("proj-1a2b");
        assert_eq!(bound_playbook("proj-1a2b"), None);
        match before {
            Some(v) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", v) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A claim that never entered the clock is due now; a scheduled one is
    /// not; trust rows never are; and the summary says whether the clock runs.
    #[test]
    fn unreviewed_claims_are_due_and_the_summary_says_if_the_clock_runs() {
        let atoms = vec![
            serde_json::json!({"id": "a", "kind": "conclusion", "text": "old", "due_at": ""}),
            serde_json::json!({"id": "b", "kind": "conclusion", "text": "older"}),
            serde_json::json!({"id": "c", "kind": "conclusion", "text": "later",
                "due_at": "2030-01-01T00:00:00Z"}),
            serde_json::json!({"id": "d", "kind": "conclusion", "text": "past",
                "due_at": "2020-01-01T00:00:00Z"}),
            serde_json::json!({"id": "t", "kind": "trust", "text": "x weighs y"}),
            serde_json::json!({"id": "p", "kind": "playbook", "text": "sit recipe", "name": "sit"}),
        ];
        let now = "2026-01-01T00:00:00Z";
        let due: Vec<String> = super::due_of(&atoms, now)
            .iter()
            .map(|a| a["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            due,
            ["a", "b", "d"],
            "unreviewed first, then the past-due one"
        );
        assert_eq!(
            super::review_summary(&atoms, now),
            "3 due; 1 scheduled, next at 2030-01-01T00:00:00Z"
        );
        assert_eq!(
            super::review_summary(&[atoms[4].clone()], now),
            "0 due; nothing scheduled: this seat has remembered nothing yet"
        );
        assert!(super::format_due(&super::due_of(&atoms, now)).starts_with("unreviewed\t"));
    }

    #[test]
    fn bumping_mcp_generation_respawns_without_rewriting_the_entry() {
        let dir = std::env::temp_dir().join(format!("ljos-gen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tempdir");
        let config = dir.join("config.toml");
        std::fs::write(
            &config,
            "[mcp_servers.ljos.env]\nLJOS_MCP_GENERATION = \"0.12.8\"\n",
        )
        .expect("write");
        let bumped = super::bump_ljos_mcp_generation(&config, "0.13.1", false)
            .expect("bumps")
            .expect("changed");
        assert_eq!(bumped, "0.13.1");
        let text = std::fs::read_to_string(&config).expect("read");
        assert!(text.contains("LJOS_MCP_GENERATION = \"0.13.1\""), "{text}");
        assert!(!text.contains("0.12.8"), "{text}");
        assert!(
            super::bump_ljos_mcp_generation(&config, "0.13.1", false)
                .expect("second")
                .is_none(),
            "a matching generation is left alone"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_client_name_listed_on_a_harness_is_that_runners_seat() {
        let dir = std::env::temp_dir().join(format!("ljos-clients-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("harnesses.toml");
        std::fs::write(
            &file,
            "[[harness]]\nname = \"acme\"\nclients = [\"acme-mcp-client\"]\n\n[[harness]]\nname = \"brio\"\nclients = [\"brio-coding-agent\"]\n",
        )
        .unwrap();
        assert_eq!(
            runner_for_client(&file, "acme-mcp-client").as_deref(),
            Some("acme")
        );
        assert_eq!(
            runner_for_client(&file, &seat_slug("brio-coding-agent")).as_deref(),
            Some("brio")
        );
        assert!(runner_for_client(&file, "acme-cli").is_none());
        assert!(runner_for_client(&dir.join("absent.toml"), "acme-mcp-client").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_issues_tags_are_words_it_speaks_in() {
        let v: Value = serde_json::from_str(r#"{"tags":["Decision","sharing","memory"]}"#).unwrap();
        assert_eq!(tags_of(&v), vec!["decision", "sharing", "memory"]);
        assert!(tags_of(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn a_decision_is_a_tag_a_type_or_an_options_line() {
        let v = |j: &str| -> Value { serde_json::from_str(j).unwrap() };
        assert!(is_decision(&v(r#"{"tags":["seat","decision"]}"#)));
        assert!(is_decision(&v(r#"{"properties":{"TYPE":"decision"}}"#)));
        assert!(is_decision(&v(
            r#"{"body":"Evidence.\n\nOptions:\n- a\n- b"}"#
        )));
        assert!(!is_decision(&v(
            r#"{"tags":["bug"],"properties":{"TYPE":"task"},"body":"no options here"}"#
        )));
        assert!(!is_decision(&v(
            r#"{"body":"We weighed the Options: none"}"#
        )));
    }

    #[test]
    fn a_probe_passes_only_when_the_runner_lists_ljos() {
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        assert!(probe_lists_ljos(&s(&["sh", "-c", "echo '  ljos_sitting   Call this'"])).is_ok());
        assert!(probe_lists_ljos(&s(&["sh", "-c", "echo 'MCP SDK not installed'"])).is_err());
        assert!(probe_lists_ljos(&s(&["sh", "-c", "echo ljos_sitting; exit 3"])).is_err());
        assert!(probe_lists_ljos(&s(&["/nonexistent/runner"])).is_err());
        let all: super::Harnesses = toml::from_str(super::HARNESSES_EXAMPLE).expect("parses");
        let hermes = all.harness.iter().find(|h| h.name == "hermes").unwrap();
        assert_eq!(hermes.probe, s(&["hermes", "mcp", "test", "ljos"]));
    }

    #[test]
    fn a_plugin_runner_gets_its_bundled_plugin_with_ljos_filled() {
        let all: super::Harnesses = toml::from_str(super::HARNESSES_EXAMPLE).expect("parses");
        for name in ["opencode", "omp"] {
            let h = all.harness.iter().find(|h| h.name == name).expect(name);
            assert!(h.plugin.is_some(), "{name} names a plugin path");
            let text = super::plugin_text(h, Path::new("/opt/seat/bin/ljos")).expect(name);
            assert!(text.contains("\"/opt/seat/bin/ljos\""), "{name}");
            assert!(!text.contains("{ljos}"), "{name}");
            assert!(
                text.contains("PreToolUse") && text.contains("UserPromptSubmit"),
                "{name}"
            );
        }
        let unknown = super::Harness {
            name: "x".into(),
            plugin: Some("/tmp/x.ts".into()),
            plugin_template: Some("nobody".into()),
            ..Default::default()
        };
        assert!(super::plugin_text(&unknown, Path::new("/l")).is_none());
        let step = super::plugin_step(&unknown, Path::new("/tmp/x.ts"), true);
        assert!(!step.ok, "an unknown template writes nothing: {step:?}");
    }

    /// The example file parses, and onboarding a config-file runner from it
    /// appends the entry once and writes the skill once; a dry run writes
    /// nothing; an unnamed runner is refused with the names the file holds.
    #[test]
    fn onboarding_a_config_file_runner_writes_once() {
        let all: super::Harnesses = toml::from_str(super::HARNESSES_EXAMPLE).expect("parses");
        // Three shapes, then the four runners this seat has carried.
        assert_eq!(all.harness.len(), 7);
        assert!(all.harness[3..].iter().all(|h| h.register.len()
            + usize::from(h.config.is_some())
            + usize::from(h.config_json.is_some())
            > 0));
        assert_eq!(all.harness[1].marker.as_deref(), Some("[mcp_servers.ljos]"));
        assert_eq!(all.harness[2].json_pointer.as_deref(), Some("/mcp/ljos"));

        let dir = std::env::temp_dir().join(format!("ljos-onboard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tempdir");
        let config = dir.join("config.toml");
        let skills = dir.join("skills");
        let file = dir.join("harnesses.toml");
        std::fs::write(
            &file,
            format!(
                "[[harness]]\nname = \"r\"\nconfig = {config:?}\nmarker = \"[mcp_servers.ljos]\"\n\
                 snippet = \"\\n[mcp_servers.ljos]\\ncommand = \\\"{{server}}\\\"\\n\"\nskills = {skills:?}\n",
                config = config.display().to_string(),
                skills = skills.display().to_string(),
            ),
        )
        .expect("write");

        let refused = super::onboard_from(&file, "nobody", true)
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("no runner \"nobody\"") && refused.contains("names r"),
            "{refused}"
        );

        let steps = match super::onboard_from(&file, "r", true) {
            Ok(steps) => steps,
            // Without ljos-mcp on PATH there is nothing to register; the
            // refusal says so and the rest of the check needs the binary.
            Err(e) => {
                assert!(e.to_string().contains("ljos-mcp not on PATH"), "{e}");
                return;
            }
        };
        assert!(steps.iter().all(|s| s.ok), "{steps:?}");
        assert!(
            steps[0].detail.starts_with("would append"),
            "{}",
            steps[0].detail
        );
        assert!(!config.exists() && !skills.exists(), "a dry run wrote");

        let steps = super::onboard_from(&file, "r", false).expect("onboards");
        assert!(steps.iter().all(|s| s.ok), "{steps:?}");
        let written = std::fs::read_to_string(&config).expect("config written");
        assert_eq!(written.matches("[mcp_servers.ljos]").count(), 1);
        assert!(written.contains("ljos-mcp"), "{written}");
        let skill = std::fs::read_to_string(skills.join("ljos/SKILL.md")).expect("skill written");
        assert!(skill.starts_with("---\nname: ljos\n"));
        assert!(skill.contains("## Before the work"));

        let again = super::onboard_from(&file, "r", false).expect("onboards again");
        assert_eq!(again[0].detail, "ljos registered");
        assert!(
            again[1].detail.ends_with("is current"),
            "{}",
            again[1].detail
        );
        assert_eq!(
            std::fs::read_to_string(&config)
                .expect("config")
                .matches("[mcp_servers.ljos]")
                .count(),
            1,
            "the entry was appended twice"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grok_onboard_names_the_frozen_hook_file() {
        let file = std::env::temp_dir().join("ljos-missing-harnesses.toml");
        let steps = super::onboard_from(&file, "grok", true).expect("grok dry");
        assert!(steps[0].ok, "{steps:?}");
        assert!(
            steps[0].detail.contains(".grok/hooks/ljos.json"),
            "{}",
            steps[0].detail
        );
    }

    #[test]
    fn the_grok_hook_file_runs_ljos_by_absolute_path() {
        let text = super::grok_hooks_json(Path::new("/opt/seat/bin/ljos"));
        let v: Value = serde_json::from_str(&text).expect("the hook file is JSON");
        let pre = &v["hooks"]["PreToolUse"][0]["hooks"][0];
        assert_eq!(pre["command"], "/opt/seat/bin/ljos hook");
        assert_eq!(pre["timeout"], 10);
        assert!(!text.contains("{ljos}"), "{text}");
        assert!(!text.contains("\"ljos hook\""), "{text}");
    }

    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A non-zero exit is an error carrying what was said on stderr.
    #[test]
    fn a_refusal_is_an_error_not_an_answer() {
        let err = run_captured("false", &[] as &[&str]).unwrap_err();
        assert!(err.to_string().contains("false exited"), "{err}");
        let said = run_captured("sh", &["-c", "echo answered; echo aside >&2"]).unwrap();
        assert_eq!(said.stdout.trim(), "answered");
        assert_eq!(said.stderr.trim(), "aside");
        let said = run_captured("sh", &["-c", "echo reason >&2; exit 3"]).unwrap_err();
        assert!(said.to_string().contains("reason"), "{said}");
    }

    #[test]
    fn join_keeps_spaces() {
        assert_eq!(
            join(&["the default fuse".into(), "is CombMNZ".into()]),
            "the default fuse is CombMNZ"
        );
    }

    #[test]
    fn remember_is_lesson_prefer_is_preference() {
        assert_eq!(atom_kind("Remember").unwrap(), "lesson");
        assert_eq!(atom_kind("Prefer").unwrap(), "preference");
        assert!(atom_kind("extract").is_err());
    }

    #[test]
    fn a_sitting_lists_the_due_claims_its_island_holds_first() {
        let due = vec![
            serde_json::json!({"id": "old", "due_at": "2026-09-01"}),
            serde_json::json!({"id": "here", "due_at": "2026-09-05"}),
            serde_json::json!({"id": "older", "due_at": "2026-08-01"}),
        ];
        let island = serde_json::json!({"island": [{"id": "here"}, {"id": "absent"}]});
        let ids: Vec<String> = due_on_island_first(due, &island)
            .iter()
            .map(|a| a["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, ["here", "old", "older"]);
        let weak = serde_json::json!({"weak": true, "island": [{"id": "older"}]});
        let kept = due_on_island_first(
            vec![
                serde_json::json!({"id": "a"}),
                serde_json::json!({"id": "older"}),
            ],
            &weak,
        );
        assert_eq!(kept[0]["id"], "a", "a weak island does not reorder");
    }

    #[test]
    fn atom_body_is_explicit_and_unextracted() {
        let v = atom_body("lesson", "the default fuse is CombMNZ", "ws");
        assert_eq!(v["schema"], "inside.atom/v1");
        assert_eq!(v["kind"], "lesson");
        assert_eq!(v["level"], "explicit");
        assert_eq!(v["text"], "the default fuse is CombMNZ");
        assert_eq!(v["workspace"], "ws");
        // Every write names the seat that wrote it, and other entities join it.
        let seat = v["entities"][0].as_str().unwrap();
        assert!(seat.starts_with(SEAT_ENTITY), "{seat}");
        let mut more = v.clone();
        add_entities(
            &mut more,
            ["persona:reviewer".to_string(), seat.to_string()],
        );
        assert_eq!(more["entities"].as_array().unwrap().len(), 2, "{more}");
        // Never harvest a transcript: the text is the claim, not a prefix parse.
        let raw = atom_body("lesson", "Remember: pin the review set", "ws");
        assert_eq!(raw["text"], "Remember: pin the review set");
    }

    #[test]
    fn empty_claim_is_refused() {
        let client = PacksetClient::new("http://127.0.0.1:1");
        let err = post_claim(&client, "Remember", "   ", "ws").unwrap_err();
        assert!(err.to_string().contains("empty text"));
    }

    #[test]
    fn cards_are_the_two_named_files_only() {
        assert_eq!(CARD_NAMES, &["USER.md", "MEMORY.md"]);
        let dir = std::env::temp_dir().join(format!("ljos-cards-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("USER.md"), "user card\n").unwrap();
        std::fs::write(dir.join("MEMORY.md"), "memory card\n").unwrap();
        std::fs::write(dir.join("NOTES.md"), "must not appear\n").unwrap();
        let out = cards(&dir).unwrap();
        assert!(out.contains("user card"));
        assert!(out.contains("memory card"));
        assert!(!out.contains("must not appear"));
        assert!(!out.contains("NOTES.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn policy_prints_argv_and_does_not_reload() {
        assert!(policy_line(&[]).is_err());
        assert_eq!(policy_line(&["ls".into(), "-la".into()]).unwrap(), "ls -la");
        let note = POLICY_TCB.to_ascii_lowercase();
        assert!(note.contains("ljos-policyd"));
        assert!(note.contains("not a check"));
        assert!(!note.contains("grokos policy reload"));
        assert!(!note.contains("policy reload"));
    }

    #[test]
    fn consensus_is_ljos_then_vissue() {
        let steps = consensus_steps("vissue-1a5a", true, true, &[]).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].bin, "ljos-consensus");
        assert_eq!(steps[0].args, vec!["settle", "--issue", "vissue-1a5a"]);
        assert_eq!(steps[1].bin, "vissue");
        assert_eq!(steps[1].args, vec!["consensus", "vissue-1a5a"]);
    }

    #[test]
    fn consensus_carries_the_packs_trust() {
        let rows = vec![row("a", "b", 0.5)];
        let steps = consensus_steps("id", true, true, &rows).unwrap();
        assert_eq!(steps[0].args[3], "--trust");
        assert_eq!(steps[0].args[4], r#"[["a","b",0.5]]"#);
        assert_eq!(
            steps[1].args,
            vec!["consensus", "id", "--trust", r#"[["a","b",0.5]]"#]
        );
    }

    #[test]
    fn consensus_skips_a_missing_bin() {
        let only_v = consensus_steps("id", false, true, &[]).unwrap();
        assert_eq!(only_v.len(), 1);
        assert_eq!(only_v[0].bin, "vissue");
        let only_l = consensus_steps("id", true, false, &[]).unwrap();
        assert_eq!(only_l[0].bin, "ljos-consensus");
        assert!(consensus_steps("id", false, false, &[]).is_err());
    }

    fn row(from: &str, to: &str, weight: f64) -> Trust {
        Trust {
            about: Vec::new(),
            from: from.into(),
            to: to.into(),
            weight,
        }
    }

    #[test]
    fn a_trust_atom_is_one_edge_with_its_evidence() {
        let atom = trust_atom(&row("a", "b", 0.25), &["deed-x-y".into()], "ws").unwrap();
        assert_eq!(atom["kind"], "trust");
        assert_eq!(atom["from"], "a");
        assert_eq!(atom["to"], "b");
        assert_eq!(atom["weight"], 0.25);
        assert_eq!(atom["entities"], serde_json::json!(["deed-x-y"]));
        assert_eq!(atom["text"], "a weighs b at 0.250.");
        assert!(trust_atom(&row("a", "a", 0.5), &[], "ws").is_err());
        assert!(trust_atom(&row("a", "b", 0.0), &[], "ws").is_err());
        assert!(trust_atom(&row("a", "b", 1.5), &[], "ws").is_err());
        assert!(trust_atom(&row("", "b", 0.5), &[], "ws").is_err());
    }

    #[test]
    fn the_latest_row_per_pair_wins() {
        let atoms = vec![
            serde_json::json!({"kind": "trust", "from": "a", "to": "b", "weight": 0.9, "ts": "2026-01-01T00:00:00Z"}),
            serde_json::json!({"kind": "trust", "from": "a", "to": "b", "weight": 0.3, "ts": "2026-02-01T00:00:00Z"}),
            serde_json::json!({"kind": "trust", "from": "b", "to": "a", "weight": 0.7}),
            serde_json::json!({"kind": "lesson", "text": "not a row"}),
            serde_json::json!({"kind": "trust", "from": "b", "weight": 0.7}),
        ];
        let rows = trust_rows(&atoms);
        assert_eq!(rows, vec![row("a", "b", 0.3), row("b", "a", 0.7)]);
        assert_eq!(trust_json(&rows), r#"[["a","b",0.3],["b","a",0.7]]"#);
    }

    #[test]
    fn ballots_are_agent_and_choice() {
        let rows =
            ballots_from_json(r#"[{"agent":"a","choice":"ship","stamp":"[2026-01-01]"}]"#).unwrap();
        assert_eq!(rows, vec![("a".to_string(), "ship".to_string())]);
        assert!(ballots_from_json(r#"[{"agent":"a"}]"#).is_err());
        assert!(ballots_from_json("{}").is_err());
    }

    /// A refuted voter loses weight in every other voter's row; a vindicated
    /// one keeps it; the rows come back complete.
    #[test]
    fn learning_downweights_the_refuted_voter() {
        let ballots = vec![
            ("a".to_string(), "ship".to_string()),
            ("b".to_string(), "ship".to_string()),
            ("c".to_string(), "hold".to_string()),
        ];
        let rows = learn(&ballots, "ship", &[], 0.5).unwrap();
        assert_eq!(rows.len(), 6);
        let w = |from: &str, to: &str| {
            rows.iter()
                .find(|r| r.from == from && r.to == to)
                .unwrap()
                .weight
        };
        assert_eq!(w("a", "b"), 1.0);
        assert_eq!(w("a", "c"), 0.5);
        assert_eq!(w("b", "c"), 0.5);
        assert_eq!(w("c", "a"), 1.0);

        let again = learn(&ballots, "ship", &rows, 0.5).unwrap();
        let w2 = |from: &str, to: &str| {
            again
                .iter()
                .find(|r| r.from == from && r.to == to)
                .unwrap()
                .weight
        };
        assert_eq!(w2("a", "c"), 0.25);
        assert_eq!(w2("a", "b"), 1.0);

        let floored = learn(&ballots, "ship", &[row("a", "c", 0.015)], 0.5).unwrap();
        let low = floored
            .iter()
            .find(|r| r.from == "a" && r.to == "c")
            .unwrap();
        assert_eq!(low.weight, TRUST_FLOOR);

        assert!(learn(&ballots, "ship", &[], 1.0).is_err());
        assert!(learn(&ballots, "  ", &[], 0.5).is_err());
        assert!(learn(&ballots[..1], "ship", &[], 0.5).is_err());

        // A fixed share of recovery: the refuted row moves back toward one
        // by the share of the gap, the vindicated row stays at one.
        let shared = learn_shared(&ballots, "ship", &rows, 0.5, &[], 0.1).unwrap();
        let w3 = |from: &str, to: &str| {
            shared
                .iter()
                .find(|r| r.from == from && r.to == to)
                .unwrap()
                .weight
        };
        assert!((w3("a", "c") - (0.25 + 0.75 * 0.1)).abs() < 1e-12);
        assert_eq!(w3("a", "b"), 1.0);
        assert!(learn_shared(&ballots, "ship", &[], 0.5, &[], 1.0).is_err());
    }

    #[test]
    fn a_name_is_one_work_id_and_hex_passes_through() {
        let a = work_id("demo-riml");
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(a, work_id(" demo-riml "));
        assert_ne!(a, work_id("demo-rimm"));
        assert_eq!(work_id(&a.to_ascii_uppercase()), a);
        assert_ne!(work_id("seat"), work_id("reader"));
    }

    #[test]
    fn a_refusal_is_not_a_writer_that_is_down() {
        let refused = anyhow::Error::from(packset_client::Error::Bad("no".into()));
        assert!(!writer_unreachable(&refused));
    }

    #[test]
    fn a_stated_probability_has_a_brier_score_and_a_hard_vote_does_not() {
        let rows = vec![
            Forecast {
                agent: "a".into(),
                choice: "ship".into(),
                confidence: Some(0.8),
            },
            Forecast {
                agent: "b".into(),
                choice: "hold".into(),
                confidence: None,
            },
        ];
        assert!((brier("ship", "ship", 0.8) - 0.04).abs() < 1e-12);
        assert!((brier("hold", "ship", 0.8) - 0.64).abs() < 1e-12);
        let (mean, n) = mean_brier(&rows, "ship").unwrap();
        assert_eq!(n, 1);
        assert!((mean - 0.04).abs() < 1e-12);
        let said = learn_reading(2, 0, &rows, "ship", &std::collections::BTreeMap::new());
        assert!(said.contains("Brier 0.040"), "{said}");
        assert!(said.contains("not a trust weight"), "{said}");
        let silent = learn_reading(2, 0, &rows[1..], "ship", &std::collections::BTreeMap::new());
        assert!(silent.contains("No stated probability"), "{silent}");
        assert!(log_score("ship", "ship", 0.8).unwrap() > 0.0);
        assert!(log_score("hold", "ship", 1.0).is_none());
        let mut cal = Calibration::default();
        cal = observe(&cal, "ship", "ship", 0.8);
        cal = observe(&cal, "ship", "hold", 0.8);
        let part = murphy(&cal).unwrap();
        let mean_b = cal.sum_brier / f64::from(cal.n);
        assert!((part.reliability - part.resolution + part.uncertainty - mean_b).abs() < 1e-9);
        assert!((cal.sum_p / f64::from(cal.n) - 0.8).abs() < 1e-12);
        assert!((cal.sum_o / f64::from(cal.n) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn an_island_prints_one_memory_a_line() {
        let body = serde_json::json!({"island": [
            {"id": "a", "text": "one", "activation": 1.0, "seed": true, "ts": now_utc()},
            {"id": "b", "text": "two", "activation": 0.25, "seed": false}
        ]});
        let printed = format_island(&body);
        assert!(
            printed.contains("Seat island") && printed.contains("Not fired"),
            "{printed}"
        );
        assert!(
            printed.contains("1.000\tseed\ta\ttoday\tone\n"),
            "{printed}"
        );
        assert!(printed.contains("0.250\t    \tb\t\ttwo\n"), "{printed}");
        assert!(format_island(&serde_json::json!({})).is_empty());
        let persona = serde_json::json!({
            "as": "reviewer",
            "fired": 3,
            "island": [{"id": "a", "text": "one", "activation": 1.0, "seed": true, "ts": now_utc()}]
        });
        let walked = format_island(&persona);
        assert!(walked.contains("Persona reviewer"), "{walked}");
        assert!(walked.contains("Fired: 3"), "{walked}");
        assert!(!walked.contains("Seat island"), "{walked}");
    }

    #[test]
    fn a_fed_verb_reads_its_stdin() {
        let said = run_fed("cat", &[] as &[&str], "one\ntwo\n").unwrap();
        assert_eq!(said.stdout, "one\ntwo\n");
        assert!(run_fed("sh", &["-c", "exit 2"], "").is_err());
    }

    #[test]
    fn needs_and_cited_are_enclosed_once_each() {
        let needs = needs_of(r#"{"needs":["deed-b-2","deed-a-1"],"other":1}"#).unwrap();
        assert_eq!(needs, vec!["deed-b-2", "deed-a-1"]);
        assert_eq!(
            enclose(needs, "deed-a-1\n\ndeed-c-3\n"),
            vec!["deed-a-1", "deed-b-2", "deed-c-3"]
        );
        assert!(needs_of("{}").unwrap().is_empty());
        assert!(needs_of("not json").is_err());
    }

    #[test]
    fn a_json_config_takes_the_entry_by_pointer() {
        let dir = std::env::temp_dir().join(format!("ljos-onboard-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("runner.json");
        std::fs::write(&config, "{\"model\": \"x\"}\n").unwrap();
        let entry = serde_json::json!({"type": "local", "command": ["/bin/ljos-mcp"]});
        set_json_entry(&config, "/mcp/ljos", &entry).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert_eq!(doc["model"], "x", "the rest of the file stands");
        assert_eq!(doc["mcp"]["ljos"]["command"][0], "/bin/ljos-mcp");
        let h = Harness {
            name: "runner".into(),
            register: Vec::new(),
            registered: Vec::new(),
            config: None,
            marker: None,
            snippet: None,
            config_json: Some(config.display().to_string()),
            json_pointer: Some("/mcp/ljos".into()),
            json_entry: None,
            skills: None,
            hooks: None,
            hook_events: Vec::new(),
            plugin: None,
            plugin_template: None,
            probe: Vec::new(),
            clients: Vec::new(),
        };
        assert_eq!(is_registered(&h, Path::new("/bin/ljos-mcp")), Some(true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_persona_set_is_in_the_pack_alphabet() {
        assert_eq!(persona_set("Reviewer"), "persona-reviewer");
        assert_eq!(persona_set("first gpu:user"), "persona-first-gpu-user");
        assert!(persona_set("x".repeat(60).as_str()).len() <= 32);
    }

    #[test]
    fn the_roster_lists_each_persona_on_one_line() {
        assert!(format_personas(&[]).starts_with("no personas;"));
        let roster = format_personas(&[
            Persona {
                name: "reviewer".into(),
                anchor: 0.2,
                view: "Reads for what breaks.".into(),
                entities: vec!["docs".into(), "release".into()],
            },
            Persona {
                name: "reader".into(),
                anchor: 0.8,
                view: "Reads as a first-time user.".into(),
                entities: Vec::new(),
            },
        ]);
        let lines: Vec<&str> = roster.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].starts_with("reviewer  anchor 0.20  about docs, release  Reads"),
            "{}",
            lines[0]
        );
        assert!(lines[1].contains("about anything"), "{}", lines[1]);
    }

    #[test]
    fn the_sweep_line_counts_what_moved_and_is_silent_otherwise() {
        assert_eq!(format_sweep(None), "");
        assert_eq!(
            format_sweep(Some(&serde_json::json!({"lapsed": 0, "forgotten": 0}))),
            ""
        );
        let line = format_sweep(Some(&serde_json::json!({"lapsed": 2, "forgotten": 1})));
        assert!(line.contains("2 reviews lapsed"), "{line}");
        assert!(line.contains("1 never-recalled claim forgotten"), "{line}");
        let one = format_sweep(Some(&serde_json::json!({"lapsed": 1, "forgotten": 0})));
        assert!(
            one.contains("1 review lapsed past twice its interval"),
            "{one}"
        );
    }

    #[test]
    fn due_is_the_past_soonest_first() {
        let atoms = vec![
            serde_json::json!({"id": "late", "due_at": "2026-02-01T00:00:00.000Z"}),
            serde_json::json!({"id": "later", "due_at": "2026-03-01T00:00:00.000Z"}),
            serde_json::json!({"id": "future", "due_at": "2099-01-01T00:00:00.000Z"}),
            serde_json::json!({"id": "never"}),
            serde_json::json!({"id": "blank", "due_at": ""}),
        ];
        let due = due_of(&atoms, "2026-06-01T00:00:00.000Z");
        let ids: Vec<&str> = due.iter().map(|a| a["id"].as_str().unwrap()).collect();
        // A claim that never entered the clock is due now, ahead of the
        // past-due ones; the future one waits.
        assert_eq!(ids, ["never", "blank", "late", "later"]);
        assert!(now_utc().ends_with(".000Z"));
        assert!(now_utc().as_str() > "2026-01-01T00:00:00.000Z");
    }

    #[test]
    fn timeline_exposes_event_rows() {
        let src = include_str!("lib.rs");
        assert!(src.contains("pub fn timeline_events"));
        assert!(src.contains("Result<Vec<Event>>"));
        assert!(src.contains("pub fn pack_last_write_ts"));
        assert!(src.contains("GET /v1/status"));
        assert!(src.contains("vissue_core::agent::show_json"));
    }

    #[test]
    fn timeline_of_does_not_shell_vissue() {
        let src = include_str!("lib.rs");
        let start = src.find("fn timeline_of").expect("timeline_of");
        let end = src[start..]
            .find("\npub fn timeline(")
            .map(|i| start + i)
            .expect("timeline after timeline_of");
        let body = &src[start..end];
        assert!(
            !body.contains("run_captured(\"vissue\""),
            "timeline_of must not shell vissue"
        );
        assert!(
            !body.contains("Command::new(\"vissue\")"),
            "timeline_of must not Command::new vissue"
        );
        assert!(
            body.contains("tracker_show_json"),
            "timeline_of should call the tracker library"
        );
    }

    #[test]
    fn timeline_events_reads_the_tracker_without_shelling_vissue() {
        let _g = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Software/sample");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("issues.org"),
            "#+TITLE: sample issues\n#+VISSUE: 1\n#+CATEGORY: sample\n#+TODO: TODO STARTED BLOCKED | DONE CANCELLED\n\n* TODO [#B] Deed rail library show\n:PROPERTIES:\n:ID:         sample-k2p2\n:CREATED:    [2026-09-20 Sat]\n:END:\n",
        )
        .unwrap();
        let old_issue_root = std::env::var_os("ISSUE_ROOT");
        let old_vissue_root = std::env::var_os("VISSUE_ROOT");
        let old_no_route = std::env::var_os("VISSUE_NO_ROUTE");
        let old_path = std::env::var_os("PATH");
        unsafe {
            std::env::set_var("ISSUE_ROOT", dir.path());
            std::env::set_var("VISSUE_ROOT", dir.path());
            std::env::set_var("VISSUE_NO_ROUTE", "1");
            std::env::set_var("PATH", "/usr/bin");
        }
        let events = timeline_events("sample-k2p2", 12);
        unsafe {
            match old_issue_root {
                Some(v) => std::env::set_var("ISSUE_ROOT", v),
                None => std::env::remove_var("ISSUE_ROOT"),
            }
            match old_vissue_root {
                Some(v) => std::env::set_var("VISSUE_ROOT", v),
                None => std::env::remove_var("VISSUE_ROOT"),
            }
            match old_no_route {
                Some(v) => std::env::set_var("VISSUE_NO_ROUTE", v),
                None => std::env::remove_var("VISSUE_NO_ROUTE"),
            }
            match old_path {
                Some(v) => std::env::set_var("PATH", v),
                None => std::env::remove_var("PATH"),
            }
        }
        let events = events.expect("timeline_events should read the tracker library");
        assert!(
            events
                .iter()
                .any(|e| e.source == "tracker" && e.text == "created"),
            "{events:?}"
        );
    }

    const EVIDENCE: &str = "stdout:\n== building and installing GCCcore/15.2.0...\nstderr:\nERROR: Installation of GCCcore-15.2.0.eb failed: shell command 'make ...' failed with exit code 2 in build step for GCCcore-15.2.0.eb\nsrun: error: task 0 exited";

    #[test]
    fn a_bundle_becomes_rows_with_edges_and_steady_ids() {
        let dir = std::env::temp_dir().join(format!("ljos-bump-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("locks")).unwrap();
        std::fs::write(
            dir.join("locks/default.lock.json"),
            r#"{"package":"eOn","version":"2.17.10","toolchain":{"name":"foss","version":"2026.1"},"versionsuffix":"",
                "dependencies":[
                 {"name":"CMake","version":"4.2.1","toolchain":{"name":"GCCcore","version":"15.2.0"},"easyconfig_path":"c/CMake/CMake-4.2.1-GCCcore-15.2.0.eb","build":true},
                 {"name":"Eigen","version":"5.0.0","toolchain":{"name":"GCCcore","version":"15.2.0"},"easyconfig_path":"e/Eigen/Eigen-5.0.0-GCCcore-15.2.0.eb","build":true},
                 {"name":"Python","version":"3.14.2","toolchain":{"name":"GCCcore","version":"15.2.0"},"easyconfig_path":"p/Python/Python-3.14.2-GCCcore-15.2.0.eb","build":false}]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("package.sbom.cdx.json"),
            r#"{"components":[],"dependencies":[
                {"ref":"pkg:generic/eOn@2.17.10","dependsOn":["pkg:generic/CMake@==4.2.1","pkg:generic/Eigen@==5.0.0","pkg:generic/Python@==3.14.2"]},
                {"ref":"pkg:generic/Eigen@==5.0.0","dependsOn":["pkg:generic/CMake@==4.2.1"]},
                {"ref":"pkg:generic/CMake@==4.2.1"}]}"#,
        )
        .unwrap();
        let (generation, rows) = bump_rows(&dir, "ebstack", None).unwrap();
        assert_eq!(generation, "foss/2026.1");
        let modules: Vec<&str> = rows.iter().map(|r| r.module.as_str()).collect();
        assert_eq!(
            modules,
            [
                "eOn-2.17.10-foss-2026.1",
                "CMake-4.2.1-GCCcore-15.2.0",
                "Eigen-5.0.0-GCCcore-15.2.0",
                "Python-3.14.2-GCCcore-15.2.0"
            ],
            "the root first, then every module the lock names, build dependencies included"
        );
        let cmake = &rows[1];
        let eigen = &rows[2];
        let python = &rows[3];
        assert!(cmake.blockers.is_empty());
        assert_eq!(eigen.blockers, std::slice::from_ref(&cmake.id));
        assert_eq!(
            rows[0].blockers,
            [cmake.id.clone(), eigen.id.clone(), python.id.clone()],
            "the root is blocked by every module it depends on"
        );
        assert_eq!(
            rows[0].id,
            bump_issue_id("ebstack", "eOn-2.17.10-foss-2026.1", "foss/2026.1")
        );
        assert!(rows[0].id.starts_with("ebstack-") && rows[0].id.len() == "ebstack-".len() + 8);
        assert_ne!(
            rows[0].id,
            bump_issue_id("ebstack", "eOn-2.17.10-foss-2026.1", "foss/2027a")
        );
        assert!(rows.iter().all(|r| r.result == "would make"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_finding_lesson_is_two_short_sentences_about_the_recipe() {
        let campaign = Campaign {
            package: "eOn".into(),
            version: "2.17.10".into(),
            target: "terra".into(),
            status: "completed".into(),
            attempts: 29,
            findings: Vec::new(),
        };
        let f = Finding {
            id: "attempt:6:finding:6".into(),
            status: "resolved".into(),
            class: "compile".into(),
            disposition: "requires-judgment".into(),
            stage: "build".into(),
            recipe: recipe_stem("easyconfigs/e/eOn/eOn-2.17.10-foss-2026.1.eb"),
            module: failed_module(EVIDENCE).unwrap_or_default(),
            summary: "Compile failure from EasyBuild command (exit Some(1))".into(),
            error: error_line(EVIDENCE, "Compile failure"),
            action: "applied the GCC 14 libsanitizer kernel headers patch. Kept in the overlay"
                .into(),
            changes: vec!["overlay/g/GCCcore/GCCcore-15.2.0.eb".into()],
        };
        assert_eq!(f.module, "GCCcore-15.2.0");
        let lesson = finding_lesson(&campaign, &f);
        assert_eq!(
            lesson,
            "GCCcore-15.2.0 for eOn-2.17.10-foss-2026.1 on terra: compile failed in the build step \
             with shell command 'make' failed with exit code 2 in build. \
             Fix: applied the GCC 14 libsanitizer kernel headers patch, Kept in the overlay in GCCcore-15.2.0."
        );
        assert!(!lesson.contains("srun"));
        assert_eq!(
            finding_entities(&campaign, &f),
            [
                "GCCcore-15.2.0",
                "GCCcore",
                "eOn-2.17.10-foss-2026.1",
                "eOn",
                "compile"
            ]
        );
        let retry = Finding {
            action: "successful campaign retry superseded this finding".into(),
            ..f.clone()
        };
        assert!(superseded_by_retry(&retry));
        assert!(!superseded_by_retry(&f));
        assert!(finding_lesson(&campaign, &retry).ends_with("A later attempt got past it."));
        assert_eq!(
            failed_module("== building and installing gettext/0.26...\n== FAILED"),
            Some("gettext-0.26".into())
        );
    }

    #[test]
    fn tracker_decimal_confidence_remains_a_scored_forecast() {
        let forecasts = super::forecasts_from_json(
            r#"[{"agent":"alice","choice":"accept","confidence":"0.8"},
                {"agent":"bob","choice":"reject","confidence":0.6},
                {"agent":"carol","choice":"accept","confidence":null},
                {"agent":"dana","choice":"accept"}]"#,
        )
        .unwrap();
        assert_eq!(forecasts[0].confidence, Some(0.8));
        assert_eq!(forecasts[1].confidence, Some(0.6));
        assert_eq!(forecasts[2].confidence, None);
        assert_eq!(forecasts[3].confidence, None);
        let (score, count) = super::mean_brier(&forecasts, "accept").unwrap();
        assert_eq!(count, 2);
        assert!((score - 0.2).abs() < 1e-14);
    }

    #[test]
    fn invalid_tracker_confidence_is_not_silently_unscored() {
        for confidence in ["0", "-0.1", "1.1", "\"NaN\"", "\"oops\"", "true", "[]"] {
            let raw =
                format!(r#"[{{"agent":"alice","choice":"accept","confidence":{confidence}}}]"#);
            let error = super::forecasts_from_json(&raw).unwrap_err().to_string();
            assert!(error.contains("probability in (0, 1]"), "{error}");
        }
    }

    #[test]
    fn ahead_of_a_cached_registry_answer_is_said() {
        let cached = super::CrateVersion {
            version: "0.12.16".into(),
            cached: true,
        };
        let (state, ok) = super::bin_health("/bin/ljos", Some("0.13.5"), Some(&cached));
        assert!(ok, "{state}");
        assert!(
            state.contains("ahead of crates.io (cached) 0.12.16"),
            "{state}"
        );
        let (same, _) = super::bin_health("/bin/ljos", Some("0.12.16"), Some(&cached));
        assert!(same.ends_with("crates.io (cached) 0.12.16"), "{same}");
    }

    #[test]
    fn the_mcp_binary_tracks_the_ljos_crate() {
        let crate_name = super::SEAT_BINS
            .iter()
            .find(|(bin, _)| *bin == "ljos-mcp")
            .map(|(_, name)| *name);
        assert_eq!(crate_name, Some("ljos"));
    }

    #[test]
    fn a_behind_required_bin_still_answers() {
        let latest = super::CrateVersion {
            version: "0.9.5".into(),
            cached: false,
        };
        let (state, ok) = super::bin_health("/bin/packsetd", Some("0.9.2"), Some(&latest));
        assert!(ok, "{state}");
        assert!(state.contains("behind crates.io 0.9.5"), "{state}");
        let rows = vec![Habitat {
            name: "packsetd",
            state,
            ok,
        }];
        assert!(
            healthy(&rows),
            "sitting must not refuse a stale but answering bin"
        );
    }

    #[test]
    fn ballot_health_requires_both_evidence_and_confidence_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vissue");
        for (help, missing) in [
            ("--for OPTION --json", Some("--used, --confidence")),
            ("--for OPTION --used DEEDS", Some("--confidence")),
            ("--for OPTION --confidence P", Some("--used")),
            ("--for OPTION --used DEEDS --confidence P", None),
        ] {
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\n[ \"$*\" = 'vote --help' ] || exit 3\nprintf '%s\\n' '{help}'\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            let result = super::check_vissue_ballot_protocol(&path);
            if let Some(missing) = missing {
                let error = result.unwrap_err().to_string();
                assert!(error.contains(&format!("missing {missing};")), "{error}");
                let rows = vec![Habitat {
                    name: "vissue",
                    state: error,
                    ok: false,
                }];
                assert!(!healthy(&rows));
            } else {
                result.unwrap();
            }
        }
    }

    #[test]
    fn ballot_health_refuses_a_failed_help_command() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vissue");
        std::fs::write(
            &path,
            "#!/bin/sh\necho '--used DEEDS --confidence P'\nexit 2\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let error = super::check_vissue_ballot_protocol(&path)
            .unwrap_err()
            .to_string();
        assert!(error.contains("vote --help failed"), "{error}");
    }

    #[test]
    fn the_doctor_names_every_habitat_and_the_pack_gates_health() {
        let rows = doctor();
        let names: Vec<&str> = rows.iter().map(|h| h.name).collect();
        for want in [
            "ljos",
            "packset-embed",
            "vissue",
            "deedar",
            "packset",
            "pack",
            "encoder",
            "host key",
            "deed store",
            "tracker",
        ] {
            assert!(names.contains(&want), "{names:?}");
        }
        let table = format_doctor(&rows);
        assert_eq!(table.lines().count(), rows.len());
        let sick = vec![Habitat {
            name: "pack",
            state: "PACKSET_URL unset".into(),
            ok: false,
        }];
        assert!(!healthy(&sick));
        let fine = vec![Habitat {
            name: "landfold",
            state: "not on PATH".into(),
            ok: false,
        }];
        assert!(healthy(&fine));
        assert_eq!(
            super::format_write_ack(&serde_json::json!({
                "id": "ab",
                "kind": "lesson",
                "due_at": "2026-09-15T00:00:00Z",
                "text": "The encoder sits beside packsetd."
            })),
            "ab\tlesson\tdue 2026-09-15T00:00:00Z\tThe encoder sits beside packsetd."
        );
        assert_eq!(super::parse_semver("ljos 0.12.8"), Some("0.12.8"));
        assert_eq!(
            super::cmp_semver("0.4.1", "0.5.3"),
            Some(std::cmp::Ordering::Less)
        );
    }

    #[test]
    fn enclosed_atoms_are_read_from_every_jsonl_in_the_bag() {
        let dir = std::env::temp_dir().join(format!("ljos-bag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let atoms = dir.join("data").join("atoms");
        std::fs::create_dir_all(&atoms).unwrap();
        std::fs::write(
            atoms.join("a.jsonl"),
            "{\"kind\":\"lesson\",\"text\":\"one\"}\n\n{\"kind\":\"trust\",\"from\":\"a\",\"to\":\"b\",\"weight\":0.5}\n",
        )
        .unwrap();
        std::fs::write(
            atoms.join("b.jsonl"),
            "{\"kind\":\"preference\",\"text\":\"two\"}\n",
        )
        .unwrap();
        let read = enclosed_atoms(&dir).unwrap();
        assert_eq!(read.len(), 3);
        assert_eq!(trust_rows(&read).len(), 1);
        assert!(enclosed_atoms(&dir.join("nowhere")).unwrap().is_empty());
        std::fs::write(atoms.join("c.jsonl"), "not json\n").unwrap();
        assert!(enclosed_atoms(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);

        let table = format_due(&[serde_json::json!({
            "id": "x", "kind": "lesson", "text": "t", "due_at": "2026-01-01T00:00:00.000Z"
        })]);
        assert_eq!(table, "2026-01-01T00:00:00.000Z\tlesson\tx\tt\n");
    }

    fn read_http(s: &mut impl Read) -> String {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            let n = s.read(&mut tmp).unwrap_or(0);
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = &buf[..at];
                let mut need = 0usize;
                for line in headers.split(|b| *b == b'\n') {
                    let line = std::str::from_utf8(line).unwrap_or("").trim();
                    if let Some(v) = line
                        .split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim())
                    {
                        need = v.parse().unwrap_or(0);
                    }
                }
                let have = buf.len().saturating_sub(at + 4);
                if have >= need {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    fn serve_capture() -> (String, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let captured = Arc::new(Mutex::new(String::new()));
        let slot = captured.clone();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                *slot.lock().unwrap() = read_http(&mut s);
                let body =
                    r#"{"id":"atom-1","kind":"lesson","text":"the default fuse is CombMNZ"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
            }
        });
        (format!("http://{addr}"), captured)
    }

    #[test]
    fn remember_posts_v1_atoms() {
        let (url, captured) = serve_capture();
        let client = PacksetClient::new(&url);
        let body = post_claim(&client, "Remember", "the default fuse is CombMNZ", "ws").unwrap();
        assert_eq!(body["id"], "atom-1");
        let req = captured.lock().unwrap().clone();
        assert!(req.contains("POST"), "{req}");
        assert!(req.contains("/v1/atoms"), "{req}");
        assert!(req.contains("\"kind\":\"lesson\""), "{req}");
        assert!(req.contains("the default fuse is CombMNZ"), "{req}");
        assert!(req.contains("\"level\":\"explicit\""), "{req}");
        assert!(!req.contains("extract"), "{req}");
    }

    #[test]
    fn forget_posts_the_id_and_workspace() {
        let (url, captured) = serve_capture();
        let client = PacksetClient::new(&url);
        let body = client.delete_atom("ws", "atom-1", None).unwrap();
        assert_eq!(body["id"], "atom-1");
        let req = captured.lock().unwrap().clone();
        assert!(req.contains("POST"), "{req}");
        assert!(req.contains("/v1/atoms/delete"), "{req}");
        assert!(req.contains("\"id\":\"atom-1\""), "{req}");
        assert!(req.contains("\"workspace\":\"ws\""), "{req}");
        // No deed named, no field: the pack should not have to tell an absent
        // citation from an empty one.
        assert!(!req.contains("\"why\""), "{req}");
    }

    /// The deed rides with the retraction, so the pack can write it onto the
    /// tombstone in the same step the atom leaves the live set.
    #[test]
    fn forget_carries_the_deed_that_withdrew_the_claim() {
        let (url, captured) = serve_capture();
        let client = PacksetClient::new(&url);
        client
            .delete_atom("ws", "atom-1", Some("deed-patch-overlay"))
            .unwrap();
        let req = captured.lock().unwrap().clone();
        assert!(req.contains("\"why\":\"deed-patch-overlay\""), "{req}");
    }

    /// An id is the whole of the request, so an empty one is a mistake worth
    /// naming rather than a delete of whatever the server decides that means.
    #[test]
    fn forget_refuses_an_empty_id() {
        let err = packset_forget("   ", None).unwrap_err();
        assert!(err.to_string().contains("atom id is required"), "{err}");
    }

    /// A fake tracker on PATH: `show` answers as told, `claim` logs its
    /// argv and the identity it was given.
    fn fake_vissue(dir: &std::path::Path, show_ok: bool, claim_ok: bool) -> std::path::PathBuf {
        let log = dir.join("calls.log");
        let script = format!(
            "#!/bin/sh\necho \"$* VISSUE_AGENT=${{VISSUE_AGENT:-}}\" >> '{}'\ncase \"$1\" in\n  show) {} ;;\n  claim) {} ;;\nesac\nexit 0\n",
            log.display(),
            if show_ok { "echo '{}'" } else { "exit 1" },
            if claim_ok { "echo claimed" } else { "echo refused >&2; exit 1" },
        );
        let path = dir.join("vissue");
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        log
    }

    /// Run `f` with `dir` first on PATH, then put PATH back.
    fn with_fake_on_path<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
        let old = std::env::var_os("PATH").unwrap_or_default();
        let mut new = std::ffi::OsString::from(dir.as_os_str());
        new.push(":");
        new.push(&old);
        unsafe {
            std::env::set_var("PATH", &new);
        }
        let out = f();
        unsafe {
            std::env::set_var("PATH", old);
        }
        out
    }

    #[test]
    fn a_claim_stamps_the_tracker_under_the_assignee() {
        let _g = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let log = fake_vissue(dir.path(), true, true);
        let said = with_fake_on_path(dir.path(), || stamp_tracker("proj-1a2b", "alice")).unwrap();
        assert_eq!(
            said.as_deref(),
            Some("tracker: proj-1a2b STARTED under alice")
        );
        let calls = std::fs::read_to_string(log).unwrap();
        assert!(
            calls.contains("claim proj-1a2b VISSUE_AGENT=alice"),
            "{calls}"
        );
    }

    #[test]
    fn a_node_the_tracker_does_not_know_stamps_nothing() {
        let _g = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let log = fake_vissue(dir.path(), false, true);
        let said = with_fake_on_path(dir.path(), || stamp_tracker("deadbeef", "alice")).unwrap();
        assert_eq!(said, None);
        let calls = std::fs::read_to_string(log).unwrap();
        assert!(
            !calls.contains("claim"),
            "asked to claim a non-issue: {calls}"
        );
    }

    #[test]
    fn a_closed_tracker_heading_is_reopened_when_the_graph_takes_it() {
        let _g = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("calls.log");
        let script = format!(
            "#!/bin/sh\necho \"$* VISSUE_AGENT=${{VISSUE_AGENT:-}}\" >> '{log}'\ncase \"$1\" in\n  show) echo '{{}}'; exit 0 ;;\n  update) echo updated; exit 0 ;;\n  claim)\n    echo \"$*\" | grep -q -- '--force' && {{ echo claimed; exit 0; }}\n    if grep -q '^update ' '{log}'; then echo 'vissue: proj-1a2b is claimed by you since [2026-01-01]; pass --force to take it over' >&2; exit 1; fi\n    echo 'vissue: proj-1a2b is already DONE; cannot claim' >&2\n    exit 1\n    ;;\nesac\nexit 1\n",
            log = log.display()
        );
        let path = dir.path().join("vissue");
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let said = with_fake_on_path(dir.path(), || stamp_tracker("proj-1a2b", "alice")).unwrap();
        assert_eq!(
            said.as_deref(),
            Some("tracker: proj-1a2b STARTED under alice")
        );
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(
            calls.contains("update proj-1a2b -s STARTED"),
            "reopen the heading: {calls}"
        );
        assert!(
            calls.contains("claim proj-1a2b --force VISSUE_AGENT=alice"),
            "{calls}"
        );
    }

    #[test]
    fn a_tracker_refusal_names_the_way_out() {
        let _g = env_guard();
        let dir = tempfile::tempdir().unwrap();
        let _log = fake_vissue(dir.path(), true, false);
        let err =
            with_fake_on_path(dir.path(), || stamp_tracker("proj-1a2b", "alice")).unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("ljos release proj-1a2b"), "{text}");
        assert!(text.contains("refused"), "{text}");
    }
}
