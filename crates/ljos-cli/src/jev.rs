//! The prompt hook's judgments through Jev, TypeSafe's decision model on
//! OpenRouter's Decisions API: which candidate claims bear on a prompt,
//! whether the prompt corrects the agent, and whether it puts a choice.
//!
//! One request answers all three: the prompt and the candidates are the
//! state, and each judgment is a `noul` question, a probability that the
//! statement is true. Opt-in per machine through `~/.config/ljos/jev.toml`,
//! because the call sends the prompt and the candidate claims off the
//! machine; with no file, no key, a failure or a spent budget, the hook
//! keeps its local path.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

/// `~/.config/ljos/jev.toml`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Config {
    /// Off unless set: the call sends the prompt and claims off the machine.
    #[serde(default)]
    pub enabled: bool,
    /// A file holding the OpenRouter key, one line, mode 0600.
    pub key_file: String,
    /// The model the Decisions API routes to.
    #[serde(default = "default_model")]
    pub model: String,
    /// How long the hook waits for the answer.
    #[serde(default = "default_budget")]
    pub budget_ms: u64,
    /// The Decisions endpoint.
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
}

fn default_model() -> String {
    "typesafe/jev-1.13".into()
}
fn default_budget() -> u64 {
    2000
}
fn default_endpoint() -> String {
    "https://openrouter.ai/api/alpha/decisions".into()
}

fn config_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("ljos")
        .join("jev.toml")
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map_or_else(|| PathBuf::from(path), |h| PathBuf::from(h).join(rest)),
        None => PathBuf::from(path),
    }
}

/// The machine's Jev setting, when it turned Jev on and its key is there.
#[must_use]
pub fn config() -> Option<(Config, String)> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    let cfg: Config = toml::from_str(&text).ok()?;
    if !cfg.enabled {
        return None;
    }
    let key = std::fs::read_to_string(expand(&cfg.key_file)).ok()?;
    let key = key.trim().to_string();
    (!key.is_empty()).then_some((cfg, key))
}

/// What Jev said about one prompt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Judgment {
    /// Probability that each candidate, by its index, bears on the prompt.
    pub bears: Vec<f64>,
    /// Probability the prompt corrects something the agent did or forgot.
    pub correction: f64,
    /// Probability the prompt puts a choice between options to the agent.
    pub choice: f64,
    /// What the call cost, in US dollars, as the API reported it.
    pub cost: f64,
}

/// The request body: the prompt and numbered candidates as state, one
/// `noul` per candidate and one each for a correction and a choice.
#[must_use]
pub fn request(model: &str, prompt: &str, candidates: &[&str]) -> Value {
    let mut state = format!("Prompt from the person to the agent:\n{prompt}\n\nStored claims:\n");
    for (i, text) in candidates.iter().enumerate() {
        state.push_str(&format!("[{i}] {text}\n"));
    }
    let mut questions = serde_json::Map::new();
    for i in 0..candidates.len() {
        questions.insert(
            format!("bears_{i}"),
            serde_json::json!({
                "type": "noul",
                "instructions": format!("Does stored claim [{i}] bear on what the prompt asks the agent to do now?"),
                "criteria": {
                    "true": "The claim changes or informs how the agent should act on this prompt",
                    "false": "The claim is about something else, or only shares words with the prompt"
                }
            }),
        );
    }
    questions.insert(
        "correction".into(),
        serde_json::json!({
            "type": "noul",
            "instructions": "Does the person correct the agent for something it did, forgot or was already told?",
            "criteria": {
                "true": "The prompt tells the agent it was wrong or should already know",
                "false": "The prompt asks for work or information without correcting the agent"
            }
        }),
    );
    questions.insert(
        "choice".into(),
        serde_json::json!({
            "type": "noul",
            "instructions": "Does the prompt put to the agent a choice between two or more defensible options?",
            "criteria": {
                "true": "The person asks which of several ways to take, or weighs options",
                "false": "The person names one thing to do, or asks a factual question"
            }
        }),
    );
    serde_json::json!({ "model": model, "state": state, "questions": questions })
}

/// Read the answers into a judgment; `None` when a question went
/// unanswered, so the caller falls back rather than trusting half an answer.
#[must_use]
pub fn parse(body: &Value, candidates: usize) -> Option<Judgment> {
    let answers = body.get("answers")?.as_object()?;
    let noul = |key: &str| answers.get(key)?.get("noul")?.as_f64();
    let bears: Vec<f64> = (0..candidates)
        .map(|i| noul(&format!("bears_{i}")))
        .collect::<Option<_>>()?;
    Some(Judgment {
        bears,
        correction: noul("correction")?,
        choice: noul("choice")?,
        cost: body["usage"]["cost"].as_f64().unwrap_or(0.0),
    })
}

/// Ask Jev about one prompt, inside the configured budget.
#[must_use]
pub fn judge(prompt: &str, candidates: &[&str]) -> Option<Judgment> {
    let (cfg, key) = config()?;
    let body = request(&cfg.model, prompt, candidates);
    let reply: Value = ureq::post(&cfg.endpoint)
        .timeout(Duration::from_millis(cfg.budget_ms))
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .send_json(body)
        .ok()?
        .into_json()
        .ok()?;
    let judged = parse(&reply, candidates.len())?;
    record_cost(judged.cost);
    Some(judged)
}

/// Add a call's cost to this month's running total in the state directory,
/// so `ljos doctor` can say what Jev has cost.
fn record_cost(cost: f64) {
    let dir = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
        .join("ljos");
    let _ = std::fs::create_dir_all(&dir);
    let month = crate::now_utc().chars().take(7).collect::<String>();
    let path = dir.join("jev-cost.toml");
    let mut totals: BTreeMap<String, f64> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default();
    *totals.entry(month).or_default() += cost;
    if let Ok(text) = toml::to_string(&totals) {
        let _ = std::fs::write(path, text);
    }
}

/// This month's recorded Jev spend, in US dollars.
#[must_use]
pub fn month_cost() -> Option<f64> {
    let dir = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?
        .join("ljos");
    let text = std::fs::read_to_string(dir.join("jev-cost.toml")).ok()?;
    let totals: BTreeMap<String, f64> = toml::from_str(&text).ok()?;
    let month = crate::now_utc().chars().take(7).collect::<String>();
    totals.get(&month).copied()
}

/// The `jev` row in `ljos doctor`, only on a machine with a Jev file: off,
/// on without its key, or on with this month's spend. A setting that does
/// not parse is not ok, since the hook then keeps its local path silently.
#[must_use]
pub fn doctor_row() -> Option<crate::Habitat> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    let (state, ok) = match toml::from_str::<Config>(&text) {
        Err(e) => (format!("{}: {e}", config_path().display()), false),
        Ok(cfg) if !cfg.enabled => ("off".to_string(), true),
        Ok(cfg) => match config() {
            None => (format!("on, but no key in {}", cfg.key_file), false),
            Some(_) => (
                format!(
                    "on  {}  {} ms  ${:.4} this month",
                    cfg.model,
                    cfg.budget_ms,
                    month_cost().unwrap_or(0.0)
                ),
                true,
            ),
        },
    };
    Some(crate::Habitat { name: "jev", state, ok })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_request_asks_about_every_candidate_and_both_cues() {
        let body = request("typesafe/jev-1.13", "fix the ci", &["alpha claim", "beta claim"]);
        let q = body["questions"].as_object().unwrap();
        assert_eq!(q.len(), 4);
        assert_eq!(q["bears_1"]["type"], "noul");
        assert!(body["state"].as_str().unwrap().contains("[1] beta claim"));
    }

    #[test]
    fn a_full_answer_is_read_and_a_partial_one_is_refused() {
        let full = serde_json::json!({
            "answers": {
                "bears_0": {"type": "noul", "noul": 0.9},
                "bears_1": {"type": "noul", "noul": 0.1},
                "correction": {"type": "noul", "noul": 0.2},
                "choice": {"type": "noul", "noul": 0.7}
            },
            "usage": {"input_tokens": 900, "output_tokens": 40, "cost": 0.0000378}
        });
        let j = parse(&full, 2).unwrap();
        assert_eq!(j.bears, vec![0.9, 0.1]);
        assert!((j.choice - 0.7).abs() < 1e-9);
        assert!((j.cost - 0.0000378).abs() < 1e-12);
        let partial = serde_json::json!({"answers": {"bears_0": {"noul": 0.9}}});
        assert!(parse(&partial, 2).is_none());
    }

    #[test]
    fn jev_is_off_without_a_file_and_off_when_the_file_says_so() {
        let dir = tempfile::tempdir().unwrap();
        // Safety: the test sets and clears this for itself.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", dir.path()) };
        assert!(config().is_none(), "no file, no call");
        std::fs::create_dir_all(dir.path().join("ljos")).unwrap();
        let key = dir.path().join("key");
        std::fs::write(&key, "sk-or-test\n").unwrap();
        std::fs::write(
            dir.path().join("ljos/jev.toml"),
            format!("enabled = false\nkey_file = \"{}\"\n", key.display()),
        )
        .unwrap();
        assert!(config().is_none(), "a file that says off is off");
        std::fs::write(
            dir.path().join("ljos/jev.toml"),
            format!("enabled = true\nkey_file = \"{}\"\n", key.display()),
        )
        .unwrap();
        let (cfg, k) = config().unwrap();
        assert_eq!(k, "sk-or-test");
        assert_eq!(cfg.budget_ms, 2000);
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
    }
}
