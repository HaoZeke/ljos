//! A persona's session: the runner that thinks as the persona, in a pane
//! the person can watch and talk to, kept across hand-offs.
//!
//! Each persona has a home directory. Its runner starts there, as the seat
//! named after the persona, so its memories, ballots and trust rows are
//! the persona's. A runner's `[[harness]]` table names how it starts and
//! how it resumes the latest session of the directory it starts in, so
//! the home is the session key: the second
//! hand-off resumes the first conversation, and no id is stored. A pane
//! that is still open is handed the next task in place. A task is written
//! to the persona's inbox and the pane is told one line naming the file,
//! since a long text typed into a runner's prompt submits at its first
//! line break.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// The tmux session persona windows open in when herdr is not running.
pub const PERSONA_SESSION: &str = "ljos-personas";

/// The argv that starts the runner named `runner` in a persona's home,
/// from its `[[harness]]` table: `start` the first time (the runner's name
/// alone when unset), `resume` after, which continues the latest session
/// of the directory it starts in. A runner with no `resume` starts fresh
/// each time. `None` when the table names no such runner.
#[must_use]
pub fn runner_argv_in(all: &crate::Harnesses, runner: &str, resume: bool) -> Option<Vec<String>> {
    let h = all.harness.iter().find(|h| h.name == runner)?;
    let start = if h.start.is_empty() {
        vec![h.name.clone()]
    } else {
        h.start.clone()
    };
    Some(if resume && !h.resume.is_empty() {
        h.resume.clone()
    } else {
        start
    })
}

/// [`runner_argv_in`] over this machine's runners file.
#[must_use]
pub fn runner_argv(runner: &str, resume: bool) -> Option<Vec<String>> {
    runner_argv_in(
        &crate::harnesses_from(&crate::harnesses_path()).ok()?,
        runner,
        resume,
    )
}

/// The runners this machine names.
#[must_use]
pub fn runner_names() -> Vec<String> {
    crate::harnesses_from(&crate::harnesses_path())
        .map(|all| all.harness.into_iter().map(|h| h.name).collect())
        .unwrap_or_default()
}

/// `$XDG_STATE_HOME/ljos/personas/NAME`: where the persona's runner works
/// and keeps its session.
#[must_use]
pub fn home(name: &str) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
        .join("ljos")
        .join("personas")
        .join(name)
}

/// A word quoted for `sh`.
fn sq(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// The script a persona's pane runs: the runner as the seat named after
/// the persona, in its home, then a shell left open for the person.
#[must_use]
pub fn pane_script(name: &str, argv: &[String], home: &Path) -> String {
    let cmd: Vec<String> = argv.iter().map(|a| sq(a)).collect();
    format!(
        "#!/bin/sh\nprintf '\\033]2;%s\\007' {n}\ncd {h} || exit 1\nLJOS_SEAT={n} {cmd}\n\
         echo \"persona {name}: the runner exited; this pane stays for reading\"\n\
         exec \"${{SHELL:-/bin/sh}}\" -i\n",
        n = sq(name),
        h = sq(&home.display().to_string()),
        cmd = cmd.join(" "),
    )
}

fn quiet(c: &mut std::process::Command) -> bool {
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|st| st.success())
}

fn herdr_up() -> bool {
    which::which("herdr").is_ok()
        && quiet(std::process::Command::new("herdr").args(["status", "server"]))
}

fn herdr_name(name: &str) -> String {
    format!("persona-{name}")
}

/// Where a persona's pane is open now: herdr's agent or tmux's window.
#[must_use]
pub fn live_pane(name: &str) -> Option<String> {
    if herdr_up()
        && quiet(std::process::Command::new("herdr").args(["agent", "get", &herdr_name(name)]))
    {
        return Some(format!("herdr agent {}", herdr_name(name)));
    }
    let out = std::process::Command::new("tmux")
        .args(["list-windows", "-t", PERSONA_SESSION, "-F", "#W"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|w| w == name)
        .then(|| format!("tmux {PERSONA_SESSION}:{name}"))
}

/// Open the persona's pane running `script`.
fn open_pane(name: &str, home: &Path, script: &Path) -> Result<String> {
    let script = script.display().to_string();
    if herdr_up() {
        let ok = quiet(
            std::process::Command::new("herdr")
                .args(["agent", "start", &herdr_name(name), "--no-focus", "--cwd"])
                .arg(home)
                .args(["--", "sh", &script]),
        );
        if ok {
            return Ok(format!("herdr agent {}", herdr_name(name)));
        }
    }
    if which::which("tmux").is_err() {
        bail!("persona {name}: neither herdr nor tmux is here to open its pane in");
    }
    let tmux = |args: &[&str]| quiet(std::process::Command::new("tmux").args(args));
    let ok = if tmux(&["has-session", "-t", PERSONA_SESSION]) {
        tmux(&[
            "new-window",
            "-d",
            "-t",
            PERSONA_SESSION,
            "-n",
            name,
            "sh",
            &script,
        ])
    } else {
        tmux(&[
            "new-session",
            "-d",
            "-s",
            PERSONA_SESSION,
            "-n",
            name,
            "sh",
            &script,
        ])
    };
    if !ok {
        bail!("persona {name}: tmux would not open its window");
    }
    Ok(format!("tmux {PERSONA_SESSION}:{name}"))
}

/// Type one line into the persona's pane and press enter.
fn send_line(name: &str, pane: &str, line: &str) -> Result<()> {
    let ok = if pane.starts_with("herdr") {
        let target = herdr_name(name);
        quiet(std::process::Command::new("herdr").args(["agent", "send", &target, line]))
            && pane_enter_herdr(&target)
    } else {
        let target = format!("{PERSONA_SESSION}:{name}");
        quiet(std::process::Command::new("tmux").args(["send-keys", "-t", &target, "-l", line]))
            && quiet(std::process::Command::new("tmux").args(["send-keys", "-t", &target, "Enter"]))
    };
    if !ok {
        bail!("persona {name}: the line did not reach {pane}");
    }
    Ok(())
}

/// Press enter in the pane herdr runs an agent in.
fn pane_enter_herdr(target: &str) -> bool {
    let Ok(out) = std::process::Command::new("herdr")
        .args(["agent", "get", target])
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return false;
    };
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let pane = v["result"]["agent"]["pane_id"]
        .as_str()
        .or_else(|| v["result"]["pane_id"].as_str())
        .or_else(|| v["pane_id"].as_str());
    pane.is_some_and(|p| {
        quiet(std::process::Command::new("herdr").args(["pane", "send-keys", p, "Enter"]))
    })
}

/// Wait until a fresh pane's runner can take a line: herdr says when its
/// agent is idle; tmux gets a few seconds.
fn wait_ready(name: &str, pane: &str) {
    if pane.starts_with("herdr") {
        let _ = quiet(std::process::Command::new("herdr").args([
            "agent",
            "wait",
            &herdr_name(name),
            "--status",
            "idle",
            "--timeout",
            "60000",
        ]));
    } else {
        std::thread::sleep(std::time::Duration::from_secs(8));
    }
}

/// Hand `task` to the persona `name`, whose runner is `runner`: into its
/// open pane, or a new pane that continues its session (or starts one).
/// Returns where it runs.
///
/// # Errors
///
/// An unknown runner, no pane system, or the line not reaching the pane.
pub fn hand(name: &str, runner: &str, task: &str) -> Result<String> {
    let home = home(name);
    let inbox = home.join("inbox");
    std::fs::create_dir_all(&inbox)
        .with_context(|| format!("persona {name}: {}", inbox.display()))?;
    // Two tasks in one second must not share a file.
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let file = inbox.join(format!("{millis}-{}.md", std::process::id()));
    std::fs::write(&file, task)?;
    let line = format!(
        "Read {} and do what it asks, through ljos; it is your next task as {name}.",
        file.display()
    );
    if let Some(pane) = live_pane(name) {
        send_line(name, &pane, &line)?;
        return Ok(pane);
    }
    let started = home.join(".started");
    let argv = runner_argv(runner, started.exists()).with_context(|| {
        format!(
            "persona {name}: runner {runner:?} is not a [[harness]] in {}",
            crate::harnesses_path().display()
        )
    })?;
    let script = home.join("pane.sh");
    std::fs::write(&script, pane_script(name, &argv, &home))?;
    let pane = open_pane(name, &home, &script)?;
    let _ = std::fs::write(&started, crate::now_utc());
    wait_ready(name, &pane);
    send_line(name, &pane, &line)?;
    Ok(pane)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runner_starts_fresh_then_resumes_its_home_session() {
        let all: crate::Harnesses = toml::from_str(concat!(
            "[[harness]]\nname = \"grok\"\nresume = [\"grok\", \"--continue\"]\n",
            "[[harness]]\nname = \"plain\"\nstart = [\"plain-cli\", \"--tui\"]\n",
        ))
        .unwrap();
        assert_eq!(runner_argv_in(&all, "grok", false).unwrap(), ["grok"]);
        assert_eq!(
            runner_argv_in(&all, "grok", true).unwrap(),
            ["grok", "--continue"]
        );
        assert_eq!(
            runner_argv_in(&all, "plain", true).unwrap(),
            ["plain-cli", "--tui"],
            "no resume, a fresh start"
        );
        assert!(runner_argv_in(&all, "nobody", false).is_none());
    }

    #[test]
    fn the_pane_runs_the_runner_as_the_persona_and_stays_open() {
        let s = pane_script(
            "buildengineer",
            &["grok".into(), "--continue".into()],
            Path::new("/s/personas/buildengineer"),
        );
        assert!(s.contains("cd '/s/personas/buildengineer'"));
        assert!(s.contains("LJOS_SEAT='buildengineer' 'grok' '--continue'"));
        assert!(s.trim_end().ends_with("-i"));
    }
}
