//! The prompt hook's judgments through Jev, TypeSafe's decision model, on
//! TypeSafe's own API or OpenRouter's Decisions API: which candidate claims
//! bear on a prompt, whether the prompt corrects the agent, and whether it
//! puts a choice.
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
    /// A file holding the key, one line, mode 0600.
    #[serde(default)]
    pub key_file: Option<String>,
    /// A command that prints the key on its first line, such as
    /// `["pass", "show", "api/typesafe/jev"]`; asked once per login and held
    /// in the runtime directory, mode 0600.
    #[serde(default)]
    pub key_cmd: Option<Vec<String>>,
    /// The model: `jev-1.13.0` on TypeSafe's API, `typesafe/jev-1.13` on
    /// OpenRouter's.
    #[serde(default = "default_model")]
    pub model: String,
    /// How long the hook waits for the answer.
    #[serde(default = "default_budget")]
    pub budget_ms: u64,
    /// TypeSafe's endpoint, or `https://openrouter.ai/api/alpha/decisions`.
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    /// The month's spend, in US dollars, past which the hook stops asking.
    #[serde(default = "default_monthly")]
    pub monthly_usd: f64,
    /// A prompt with fewer words is an acknowledgement ("yes", "keep
    /// going"), with too little in it for a judgment to add anything.
    #[serde(default = "default_min_words")]
    pub min_words: usize,
    /// Fewer candidates than this is nothing to choose between; the local
    /// filters answer.
    #[serde(default = "default_min_candidates")]
    pub min_candidates: usize,
    /// US dollars per million input tokens, for an API whose answer does
    /// not carry its cost. Output is not charged.
    #[serde(default = "default_price_in")]
    pub usd_per_mtok_in: f64,
    /// The probability at which a candidate counts as bearing on the
    /// prompt; higher lets fewer off-topic claims through.
    #[serde(default = "default_cut")]
    pub bears_at: f64,
    /// The probability at which the prompt counts as a correction or a
    /// choice.
    #[serde(default = "default_cut")]
    pub cue_at: f64,
}

fn default_model() -> String {
    "jev-1.13.0".into()
}
fn default_budget() -> u64 {
    2000
}
fn default_endpoint() -> String {
    "https://api.typesafe.ai/v1/systemone".into()
}
fn default_monthly() -> f64 {
    4.0
}
fn default_min_words() -> usize {
    4
}
fn default_min_candidates() -> usize {
    2
}
fn default_cut() -> f64 {
    0.5
}
fn default_price_in() -> f64 {
    0.042
}

/// What a call cost: the API's own figure when it sends one (OpenRouter
/// does), else the input tokens at the configured price.
fn cost_of(body: &Value, usd_per_mtok_in: f64) -> f64 {
    body["usage"]["cost"].as_f64().unwrap_or_else(|| {
        body["usage"]["input_tokens"].as_f64().unwrap_or(0.0) * usd_per_mtok_in / 1e6
    })
}

/// The longest prompt the state carries; a pasted log past it adds cost
/// and no judgment.
const PROMPT_CHARS: usize = 2000;

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

fn read_config() -> Option<Config> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    toml::from_str(&text).ok()
}

/// Whether this machine turned Jev on, key or not. The hook's local path
/// then skips the cross-encoder, which is what Jev stands in for.
#[must_use]
pub fn enabled() -> bool {
    read_config().is_some_and(|c| c.enabled)
}

/// The key from the first line of what a key file or command holds. A
/// `name: value` or `name=value` line gives its value.
fn key_from(text: &str) -> Option<String> {
    let line = text.lines().next()?.trim();
    let value = line
        .rsplit(|c: char| c == ':' || c == '=' || c.is_whitespace())
        .next()
        .unwrap_or(line)
        .trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn key_cache() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(dir).join("ljos").join("jev-key"))
}

/// Run the key command once, with no terminal to prompt on and three
/// seconds to answer, and hold what it printed for the rest of the login.
fn key_by_command(argv: &[String]) -> Option<String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let cache = key_cache();
    if let Some(key) = cache
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| key_from(&t))
    {
        return Some(key);
    }
    let (prog, args) = argv.split_first()?;
    let out = std::process::Command::new("timeout")
        .arg("3")
        .arg(prog)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let key = key_from(&String::from_utf8_lossy(&out.stdout))?;
    if let Some(path) = cache {
        let _ = std::fs::create_dir_all(path.parent()?);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
        {
            let _ = writeln!(f, "{key}");
        }
    }
    Some(key)
}

/// The machine's Jev setting, when it turned Jev on, its key is there and
/// the month's spend is under its cap.
#[must_use]
pub fn config() -> Option<(Config, String)> {
    let cfg = read_config()?;
    if !cfg.enabled || month_cost().unwrap_or(0.0) >= cfg.monthly_usd {
        return None;
    }
    let key = match (&cfg.key_cmd, &cfg.key_file) {
        (Some(argv), _) => key_by_command(argv)?,
        (None, Some(file)) => key_from(&std::fs::read_to_string(expand(file)).ok()?)?,
        (None, None) => return None,
    };
    Some((cfg, key))
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
    /// What the call cost, in US dollars.
    pub cost: f64,
    /// The machine's cut for `bears`.
    pub bears_at: f64,
    /// The machine's cut for `correction` and `choice`.
    pub cue_at: f64,
}

impl Judgment {
    /// Whether candidate `i` bears on the prompt at the machine's cut.
    #[must_use]
    pub fn bears(&self, i: usize) -> bool {
        self.bears.get(i).is_some_and(|p| *p >= self.bears_at)
    }
}

/// The request body: the prompt and numbered candidates as state, one
/// `noul` per candidate and one each for a correction and a choice.
#[must_use]
pub fn request(model: &str, prompt: &str, candidates: &[&str]) -> Value {
    let prompt: String = prompt.chars().take(PROMPT_CHARS).collect();
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
        cost: 0.0,
        bears_at: 0.5,
        cue_at: 0.5,
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
    let mut judged = parse(&reply, candidates.len())?;
    judged.cost = cost_of(&reply, cfg.usd_per_mtok_in);
    judged.bears_at = cfg.bears_at;
    judged.cue_at = cfg.cue_at;
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
    *totals.entry(format!("{month}-calls")).or_default() += 1.0;
    *totals.entry(month).or_default() += cost;
    if let Ok(text) = toml::to_string(&totals) {
        let _ = std::fs::write(path, text);
    }
}

fn month_totals() -> Option<BTreeMap<String, f64>> {
    let dir = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?
        .join("ljos");
    let text = std::fs::read_to_string(dir.join("jev-cost.toml")).ok()?;
    toml::from_str(&text).ok()
}

fn this_month() -> String {
    crate::now_utc().chars().take(7).collect()
}

/// This month's recorded Jev spend, in US dollars.
#[must_use]
pub fn month_cost() -> Option<f64> {
    month_totals()?.get(&this_month()).copied()
}

/// How many calls this month made.
#[must_use]
pub fn month_calls() -> u64 {
    month_totals()
        .and_then(|t| t.get(&format!("{}-calls", this_month())).copied())
        .unwrap_or(0.0) as u64
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
        Ok(cfg) => {
            let spent = month_cost().unwrap_or(0.0);
            let head = format!(
                "{}  {} calls  ${spent:.4} of ${:.2} this month",
                cfg.model,
                month_calls(),
                cfg.monthly_usd
            );
            if spent >= cfg.monthly_usd {
                (format!("capped  {head}"), true)
            } else if config().is_none() {
                ("on, but the key file or command gave no key".to_string(), false)
            } else {
                (format!("on  {head}"), true)
            }
        }
    };
    Some(crate::Habitat { name: "jev", state, ok })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_request_asks_about_every_candidate_and_both_cues() {
        let body = request("jev-1.13.0", "fix the ci", &["alpha claim", "beta claim"]);
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
        assert!(j.bears(0) && !j.bears(1));
        let strict = Judgment { bears_at: 0.95, ..j.clone() };
        assert!(!strict.bears(0), "a higher cut drops the 0.9");
        assert!((j.choice - 0.7).abs() < 1e-9);
        assert!((cost_of(&full, 0.042) - 0.0000378).abs() < 1e-12, "the API's figure");
        let direct = serde_json::json!({"usage": {"input_tokens": 1000, "output_tokens": 60}});
        assert!((cost_of(&direct, 0.042) - 0.000042).abs() < 1e-12, "tokens at the price");
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
        assert_eq!(cfg.min_candidates, 2);
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
    }

    #[test]
    fn a_key_line_gives_its_value() {
        assert_eq!(key_from("sk-or-v1-abc\n").as_deref(), Some("sk-or-v1-abc"));
        assert_eq!(key_from("apikey: sk-or-v1-abc\nurl: x\n").as_deref(), Some("sk-or-v1-abc"));
        assert_eq!(key_from("apikey=sk-or-v1-abc").as_deref(), Some("sk-or-v1-abc"));
        assert_eq!(key_from("\n"), None);
    }
}
