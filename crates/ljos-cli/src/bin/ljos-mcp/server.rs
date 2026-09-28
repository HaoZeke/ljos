//! The seat as an agent surface.
//!
//! The command line's verbs, none owned here: a pack write is an HTTP
//! call, everything else execs the habitat's own binary and hands back what
//! it said. Every tool says whether it writes; a habitat that refused is an
//! error; the cards are read-only resources under `ljos://cards/`; the
//! sequences that cross habitats are prompts. The pack is written by
//! Remember, Prefer and Forget, and the text is the claim.

use std::path::{Path, PathBuf};

use anyhow::Context as _;

use ljos_cli::{
    age_of, announce_seat, brief, bump_plan, calibrate, cards, claim, complete, conflicts,
    consensus_steps_for, copy_playbook, doctor, due_page, finish, forecasts_from_json,
    format_change, format_consolidation, graded, habit, habits, handover, identity_or_seat,
    island_entities, issue_words, learn_and_write, learn_reading, now_utc, on_path, other_seat,
    pack, packset_consolidate, packset_forget, packset_island_as, packset_search_as_of,
    packset_write_as, panel_steps, parse_every, personas_from_pack, personas_speaking_to,
    playbooks_from_pack, policy_with_memory, predictions_of, read_campaign, receive, release,
    remember_findings, resolve_assignee, rows_about, run_captured, runner_pid, search_reading,
    seat_name, sitting_gated, timeline, topic_words, tracker_show_json, trust_from_pack,
    write_persona, write_prediction, write_rule, write_trust, Persona, Rule, Trust, CARD_NAMES,
    LEARN_BETA, POLICY_TCB, PROTOCOL,
};
use rmcp::{
    handler::server::wrapper::Json, handler::server::wrapper::Parameters,
    handler::server::ServerHandler, model::*, prompt, prompt_handler, prompt_router, tool,
    tool_handler, tool_router, ErrorData as McpError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The scheme the cards are addressable under.
const SCHEME: &str = "ljos";
/// Where the sitting protocol is read from.
const PROTOCOL_URI: &str = "ljos://protocol";

#[derive(Clone)]
pub struct LjosServer {
    /// Where the cards are read from. The command line's `--dir`.
    cards_dir: PathBuf,
}

// ---- arguments -------------------------------------------------------------

/// One explicit claim.
#[derive(Deserialize, JsonSchema)]
pub struct ClaimArgs {
    /// The claim, as it will be stored. Two short sentences at most, in
    /// plain words: the pack refuses a third sentence and prose above
    /// readability grade 14. Name the thing (a recipe, a version, a file,
    /// an error line), not the procedure. Not a transcript, not a summary
    /// of one.
    pub text: String,
    /// Remember as this persona: the claim comes back to it first in its
    /// next brief. Absent, the seat's own.
    #[serde(rename = "as")]
    pub as_persona: Option<String>,
    /// `true` when this claim is about one review or one run. `false` keeps
    /// a rule that happens to name an artifact. Absent, the writer decides:
    /// a numbered pull request, a ticket id, or a commit is transient.
    #[serde(default)]
    pub transient: Option<bool>,
}

/// A question for the pack.
#[derive(Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// What to ask the seat's standing knowledge.
    pub query: String,
    /// Ask the pack as it stood at this time (`YYYY-MM-DD` or RFC 3339):
    /// what the seat knew then, memories withdrawn since included, memories
    /// learnt since left out. Omit for now.
    #[serde(default)]
    pub as_of: Option<String>,
}

/// A reading of a habit, or the question which readings stand.
#[derive(Deserialize, JsonSchema)]
pub struct HabitArgs {
    /// The habit's name (`mab cr all`, `hook p50 10k`). Absent, every habit.
    #[serde(default)]
    pub name: Option<String>,
    /// The reading. With it the tool writes; without it the tool lists.
    #[serde(default)]
    pub value: Option<f64>,
    /// The unit the reading is in, for the reader.
    #[serde(default)]
    pub unit: Option<String>,
    /// How often a reading is taken: `7d`, `24h`, `2w`. Absent, a week.
    #[serde(default)]
    pub every: Option<String>,
    /// Where the reading came from: a job id, a run, a file.
    #[serde(default)]
    pub source: Option<String>,
}

/// One habit as it stands.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ReadingRow {
    pub name: String,
    pub value: f64,
    pub unit: String,
    /// The change since the reading before, in words; `first reading` when none.
    pub change: String,
    /// When this reading was taken, in words.
    pub age: String,
    /// When the next reading is due, RFC 3339.
    pub due_at: Option<String>,
    pub source: String,
    pub id: Option<String>,
}

/// How many rows to print.
#[derive(Deserialize, JsonSchema)]
pub struct LimitArgs {
    /// Most rows; twelve when absent.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Whether a consolidation writes.
#[derive(Deserialize, JsonSchema)]
pub struct ConsolidateArgs {
    /// Write the closures. Absent or false: report the pairs, change nothing.
    #[serde(default)]
    pub apply: Option<bool>,
}

/// One atom to retire, and what withdrew it.
#[derive(Deserialize, JsonSchema)]
pub struct ForgetArgs {
    /// The atom's id, as `ljos_search` returned it. Not its text.
    pub id: String,
    /// The deed accession the retraction stands on, when the work minted one.
    /// Free text is refused: the point of writing it is that `ljos_evidence`
    /// can be asked about it later.
    #[serde(default)]
    pub why: Option<String>,
}

/// One deed accession.
#[derive(Deserialize, JsonSchema)]
pub struct AccessionArgs {
    /// `deed-<kind>-<slug>`, or `sha256:` of the deed or of a product path.
    pub accession: String,
}

/// A tracker node, and optionally a deed to cite on it.
#[derive(Deserialize, JsonSchema)]
pub struct DeedArgs {
    /// The issue id.
    pub issue: String,
    /// An accession to cite. Absent, the tool lists what the issue cites.
    pub add: Option<String>,
}

/// A tracker node.
#[derive(Deserialize, JsonSchema)]
pub struct IssueArgs {
    /// The issue id.
    pub issue: String,
}

/// A forecast of the others' ballots.
#[derive(Deserialize, JsonSchema)]
pub struct PredictArgs {
    /// The issue id.
    pub issue: String,
    /// The option you expect to win, or an object of option to share.
    pub expect: serde_json::Value,
    /// Forecast as this persona; absent, the seat's identity.
    #[serde(rename = "as")]
    pub as_persona: Option<String>,
}

/// A rule: argv law kept in the pack.
#[derive(Deserialize, JsonSchema)]
pub struct RuleArgs {
    /// A glob over the whole command line, such as `*--force*`.
    pub pattern: String,
    /// `deny` stops the action at the runner; `ask` hands it to the person.
    pub verdict: String,
    /// The reason a stopped reader sees.
    pub why: String,
}

/// An option name, or a JSON object of option to share.
/// Empty and null are absent, so a ballot without a forecast still stands.
fn expect_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        }
        other => Some(other.to_string()),
    }
}

/// A ballot, or a request for the tally.
#[derive(Deserialize, JsonSchema)]
pub struct VoteArgs {
    /// The issue id.
    pub issue: String,
    /// `accept` or `reject`. Absent, the tool shows the tally.
    pub choice: Option<String>,
    /// Probability in (0, 1] that `choice` is the outcome. Absent, the
    /// ballot is not a forecast and will not be scored.
    pub confidence: Option<f64>,
    /// Deed accessions this ballot used, comma-separated, or `none`.
    /// Required when `choice` is set.
    pub used: Option<String>,
    /// The option you expect the others to pick, or an object of option to
    /// share. Recorded with the ballot, so the surprisingly popular reading
    /// does not depend on a second call. Absent, the ballot stands alone.
    #[serde(default)]
    pub expect: Option<serde_json::Value>,
    /// Cast as this persona (a name written with `ljos_persona`) instead of
    /// the seat's own identity.
    #[serde(rename = "as")]
    pub as_persona: Option<String>,
}

/// A persona and the issue it will read.
#[derive(Deserialize, JsonSchema)]
pub struct BriefArgs {
    /// The persona's name, as written with `ljos_persona`.
    pub name: String,
    /// The tracker id of the issue.
    pub issue: String,
}

/// A voter with a view.
#[derive(Deserialize, JsonSchema)]
pub struct PersonaArgs {
    /// The persona's name; ballots cast as it carry this name.
    pub name: String,
    /// How far it moves off its ballot in a settle, in [0, 1]: 0 never
    /// moves, 1 is a plain voter. Half when absent.
    pub anchor: Option<f64>,
    /// How this persona reads the work, in a sentence or two.
    pub view: String,
    /// Domains it speaks to; a trust row scoped to one applies when the
    /// issue is about it.
    #[serde(default)]
    pub about: Vec<String>,
}

/// A writer's answer as the object a structured result has to be: the
/// daemon's own object, or a non-object carried under `value`.
fn object(value: serde_json::Value) -> Json<serde_json::Map<String, serde_json::Value>> {
    Json(match value {
        serde_json::Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".into(), other);
            map
        }
    })
}

/// Rows a tool answers with. The protocol wants a structured result to be
/// an object, so a list comes back under one key.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Rows<T: JsonSchema> {
    /// The rows, in the order the tool ranks them.
    pub rows: Vec<T>,
}

/// Wrap a list as the object a structured result has to be.
fn as_rows<T: JsonSchema>(rows: Vec<T>) -> Json<Rows<T>> {
    Json(Rows { rows })
}

/// One persona of the roster.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PersonaRow {
    /// The name its ballots carry.
    pub name: String,
    /// How far it moves off its ballot in a settle, in [0, 1].
    pub anchor: f64,
    /// The domains it speaks to; empty speaks to every issue.
    pub about: Vec<String>,
    /// How it reads the work.
    pub view: String,
}

/// A session node to take or hand back.
#[derive(Deserialize, JsonSchema)]
pub struct TakeArgs {
    /// The tracker id of the issue (`proj-1a2b`), or a 32-hex claim-graph id.
    pub node: String,
    /// Your name; a named worker occupies one slot per issue. Absent, this
    /// conversation's holder: the session the runner stamped, else the seat
    /// named after the client that connected, tagged with the runner's process.
    #[serde(default)]
    pub assignee: Option<String>,
}

/// A session node to finish.
#[derive(Deserialize, JsonSchema)]
pub struct FinishArgs {
    /// The tracker id of the issue, or a 32-hex claim-graph id.
    pub node: String,
    /// `done` (the default), `failed`, or `cancelled`. To stop without
    /// finishing, use `ljos_release` instead.
    pub status: Option<String>,
    /// Generation from the sitting's claim. Absent: the live one. A stale gen is refused.
    #[serde(default)]
    pub gen: Option<u64>,
    /// The name that holds it. Absent: the runner's seat name.
    #[serde(default)]
    pub assignee: Option<String>,
}

/// An argv to check.
#[derive(Deserialize, JsonSchema)]
pub struct ArgvArgs {
    /// The command line, one element per argument.
    pub argv: Vec<String>,
}

/// No arguments.
#[derive(Deserialize, JsonSchema)]
pub struct NoArgs {}

/// One trust row.
#[derive(Deserialize, JsonSchema)]
pub struct TrustArgs {
    /// The agent doing the weighing.
    pub from: String,
    /// The agent being weighed.
    pub to: String,
    /// In (0, 1].
    pub weight: f64,
    /// Deed accessions the row stands on.
    #[serde(default)]
    pub why: Vec<String>,
    /// Domains this row is scoped to; none means it applies everywhere.
    #[serde(default)]
    pub about: Vec<String>,
}

/// An issue and what turned out right on it.
#[derive(Deserialize, JsonSchema)]
pub struct LearnArgs {
    /// The tracker id.
    pub issue: String,
    /// The option that turned out right.
    pub outcome: String,
    /// The factor a refuted persona's anchor moves by; the seat's default
    /// when absent. The rows themselves follow each voter's record.
    pub beta: Option<f64>,
}

/// A slice of the seat to pack.
#[derive(Deserialize, JsonSchema)]
pub struct PackArgs {
    /// Where to write the satchel.
    pub out: String,
    /// Projects to take whole.
    #[serde(default)]
    pub projects: Vec<String>,
    /// Issues to take, with what they stand on.
    #[serde(default)]
    pub issues: Vec<String>,
}

/// A satchel that arrived.
#[derive(Deserialize, JsonSchema)]
pub struct ReceiveArgs {
    /// The satchel directory.
    pub dir: String,
    /// A bridge file from the last handover by the same sender.
    pub since: Option<String>,
    /// Put the enclosed atoms into this seat's pack.
    pub import: Option<bool>,
}

/// One review graded.
#[derive(Deserialize, JsonSchema)]
pub struct GradeArgs {
    /// The atom id.
    pub id: String,
    /// False when the claim had to be looked up again.
    pub recalled: Option<bool>,
}

/// A sitting to open.
#[derive(Deserialize, JsonSchema)]
pub struct SittingArgs {
    /// The tracker id of the issue (`proj-1a2b`).
    pub issue: String,
    /// Your name; a named worker occupies one slot per issue. Absent, this
    /// conversation's holder: the session the runner stamped, else the seat
    /// named after the client that connected, tagged with the runner's process.
    #[serde(default)]
    pub assignee: Option<String>,
    /// Sit even when the issue's blockers are open. Without it a blocked
    /// issue is refused before anything is claimed.
    #[serde(default)]
    pub anyway: bool,
    /// The recipe this sitting copies before recall (`sit`, `arena`, `land`,
    /// `company-panel`, `overnight`). Absent, a closed-set token in the
    /// title else sit. Held until finish or release.
    #[serde(default)]
    pub playbook: Option<String>,
}

/// Bind a playbook to an issue.
#[derive(Deserialize, JsonSchema)]
pub struct PlaybookBindArgs {
    /// The tracker id of the issue.
    pub issue: String,
    /// The recipe name: sit, arena, land, company-panel, overnight.
    pub name: String,
}

/// One closed-set playbook: pack latest, else the shipped seed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PlaybookRow {
    pub name: String,
    /// Optional model-family spawn hints; empty means none.
    pub models: Vec<String>,
    /// The full recipe body.
    pub body: String,
}

/// A sitting to close.
#[derive(Deserialize, JsonSchema)]
pub struct FinishSittingArgs {
    /// The tracker id of the issue.
    pub issue: String,
    /// `done` (the default), `failed`, or `cancelled`.
    pub status: Option<String>,
    /// The lesson this sitting taught, two sentences at most. Omit only
    /// when there was none; the report says so.
    pub lesson: Option<String>,
    /// The option that turned out right, when the ballots are in and the
    /// world has said. Omit when nobody knows yet.
    pub outcome: Option<String>,
    /// Generation from the sitting's claim. Absent: the live one. A stale gen is refused.
    #[serde(default)]
    pub gen: Option<u64>,
    /// The name that holds it. Absent: the runner's seat name.
    #[serde(default)]
    pub assignee: Option<String>,
    /// Also close the tracker ticket. Only when the work is accepted; a
    /// sitting ending is not that, and a blocker on the ticket must hold.
    #[serde(default)]
    pub close: bool,
}

/// A project whose history calibrates the voters.
#[derive(Deserialize, JsonSchema)]
pub struct CalibrateArgs {
    /// The tracker project.
    pub project: String,
    /// Expectation-maximisation rounds; twenty when absent.
    pub rounds: Option<usize>,
}

/// A cue: the task or question at hand.
#[derive(Deserialize, JsonSchema)]
pub struct CueArgs {
    /// What the seat is about to work on, in its own words.
    pub cue: String,
    /// Fire the strongest eight together so their links gain weight; say
    /// true when the island is the one you go on to use.
    pub fire: Option<bool>,
    /// Walk the island as this persona: its own weights lead the spread,
    /// and a fire moves its weights and not the seat's. A panel member
    /// passes its own name; the seat passes nothing.
    #[serde(default, rename = "as")]
    pub lens: Option<String>,
}

/// One memory an island holds.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct IslandRow {
    pub id: Option<String>,
    pub kind: String,
    pub text: String,
    /// Relative to the strongest, so the top is 1.
    pub activation: f64,
    /// Whether search found it, or activation reached it.
    pub seed: bool,
    /// How long ago it was written, in words; empty when unstamped.
    pub age: String,
}

/// One habitat and whether it answers.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct HabitatRow {
    pub name: String,
    pub state: String,
    pub ok: bool,
}

/// One atom due for review.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DueRow {
    pub id: Option<String>,
    pub kind: String,
    pub text: String,
    pub due_at: String,
}

/// The review clock as an agent reads it: the soonest claims, and how many
/// are due in all. The full list is `ljos due`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DuePage {
    /// The soonest claims. Eight, or fewer when fewer are due.
    pub rows: Vec<DueRow>,
    /// How many claims are due, including the ones `rows` does not carry.
    pub due: usize,
    /// The clock line: how many are due, how many are scheduled, when the next comes up.
    pub summary: String,
}

/// One row of the influence graph.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TrustRow {
    pub from: String,
    pub to: String,
    pub weight: f64,
}

// ---- answers ---------------------------------------------------------------

/// What a habitat printed.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Said {
    /// The habitat's answer, as it printed it.
    pub text: String,
    /// What it said aside, when anything.
    pub aside: Option<String>,
}

/// One remembered thing.
/// What learn wrote, and the rows the next settle uses.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LearnPage {
    pub reading: String,
    pub rows: Vec<TrustRow>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchPage {
    /// What the score is, and what an empty list means.
    pub reading: String,
    pub rows: Vec<HitRow>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct HitRow {
    /// The atom's id, when it has one.
    pub id: Option<String>,
    /// `lesson` or `preference`, or another kind the pack holds.
    pub kind: String,
    /// The claim as it was stored.
    pub text: String,
    /// The pack's score for it.
    pub score: f64,
    /// When it was written, RFC 3339; absent on a card paragraph.
    pub ts: Option<String>,
    /// How long ago that was, in words: `today`, `3 weeks ago`. Read the
    /// hits as a timeline: a later lesson revises an earlier one.
    pub age: String,
    /// The seat that wrote it, when it was not this one. Many seats share
    /// a pack.
    pub from: Option<String>,
}

/// The argv law's answer.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PolicyRow {
    /// The line as it would run.
    pub argv: String,
    /// What `ljos policy` prints under the line: the TCB's verdict when
    /// `ljos-policyd` is on PATH, then the rule that fired and the
    /// memories the line activates.
    pub ruling: String,
    /// What this process is not: a check. Reloading a pack is not one either.
    pub note: String,
}

/// The thread a call names in its `_meta`: a `thread_id` at the top, or in
/// any object the runner keys its own metadata under.
fn thread_in_meta(meta: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let id = |v: &serde_json::Value| {
        v.get("thread_id")
            .and_then(|t| t.as_str())
            .map(String::from)
    };
    id(&serde_json::Value::Object(meta.clone()))
        .or_else(|| meta.values().filter(|v| v.is_object()).find_map(id))
}

fn said(out: ljos_cli::Said) -> Json<Said> {
    let aside = out.stderr.trim();
    Json(Said {
        text: out.stdout,
        aside: (!aside.is_empty()).then(|| aside.to_string()),
    })
}

/// A refusal, as the protocol carries one.
fn refused(e: anyhow::Error) -> McpError {
    let text = format!("{e:#}");
    // The pack's prose rule refuses a hard sentence; the seat that hit it
    // is told what passes, so the second try is not a guess.
    let text = if text.contains("readability grade") || text.contains("sentences; one claim") {
        format!("{text}. What passes: two sentences of plain words, each under twenty, naming the thing (a recipe, a version, an error line) rather than the procedure")
    } else {
        text
    };
    McpError::internal_error(text, None)
}

/// Run a habitat's verb and hand back what it said.
fn habitat(bin: &str, args: &[&str]) -> Result<Json<Said>, McpError> {
    run_captured(bin, args).map(said).map_err(refused)
}

/// [`habitat`] with `VISSUE_AGENT` set, so a ballot is recorded under a
/// persona's name.
fn habitat_as(bin: &str, args: &[&str], identity: Option<&str>) -> Result<Json<Said>, McpError> {
    let Some(who) = identity_or_seat(identity) else {
        return habitat(bin, args);
    };
    use std::process::{Command, Stdio};
    let path = which::which(bin).map_err(|_| refused(anyhow::anyhow!("{bin} not on PATH")))?;
    let out = Command::new(path)
        .env("VISSUE_AGENT", who)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| refused(anyhow::anyhow!("{bin}: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        return Err(refused(anyhow::anyhow!(
            "{bin} exited {}: {stderr}",
            out.status
        )));
    }
    Ok(said(ljos_cli::Said { stdout, stderr }))
}

#[tool_router]
impl LjosServer {
    /// Read the cards directory from `LJOS_CARDS_DIR`, or the working directory,
    /// the same default the command line has.
    #[must_use]
    pub fn from_env() -> Self {
        let cards_dir = std::env::var_os("LJOS_CARDS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self { cards_dir }
    }

    /// Open on a named cards directory, for a test.
    #[cfg(test)]
    #[must_use]
    pub fn at(cards_dir: PathBuf) -> Self {
        Self { cards_dir }
    }

    // ---- the pack --------------------------------------------------------

    #[tool(
        description = "Call this when the work taught something. One lesson, two short sentences at most, stored as an episode: it is kept and it is not a refresher until a recalled review or a consolidation promotes it. Pass transient false to store a standing rule now. Pass transient true to force an episode. Never a transcript. Progress notes go on the issue.",
        annotations(
            title = "Remember",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_remember(
        &self,
        Parameters(args): Parameters<ClaimArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        packset_write_as(
            "Remember",
            &args.text,
            args.as_persona.as_deref(),
            args.transient,
        )
            .map(object)
            .map_err(refused)
    }

    #[tool(
        description = "Call this when a choice between two ways was settled and should hold from now on: one standing preference, stored as given. A lesson that is not a choice is ljos_remember.",
        annotations(
            title = "Prefer",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_prefer(
        &self,
        Parameters(args): Parameters<ClaimArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        packset_write_as("Prefer", &args.text, args.as_persona.as_deref(), Some(false))
            .map(object)
            .map_err(refused)
    }

    #[tool(
        description = "Call this when reviewing the seat's memory for contradictions the words do not show: the lowest passes between single memories in the pack's embedding landscape, read by the optional `landscape` habitat. On a record of planted contradictions the lowest passes were the contradictions nine times in ten. Nothing is written; judge each pair, then ljos_forget the one that turned out wrong or ljos_remember the rewrite. Absent the habitat, the tool says so.",
        annotations(
            title = "Contradiction candidates",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_conflicts(
        &self,
        Parameters(args): Parameters<LimitArgs>,
    ) -> Result<Json<Said>, McpError> {
        let text = conflicts(args.limit.unwrap_or(12)).map_err(refused)?;
        Ok(Json(Said { text, aside: None }))
    }

    #[tool(
        description = "Call this after a run of lessons on one matter, or after importing a handover: consolidate the seat's memory. Every claim that rewrites an earlier one of the same kind (same opening words, a new object; a correction; an explicit supersedes) closes the earlier one's validity window and names it, the rule a write applies on arrival run over what is already held. With apply false (the default) it reports the pairs and writes nothing; read them, then call again with apply true.",
        annotations(
            title = "Consolidate",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_consolidate(
        &self,
        Parameters(args): Parameters<ConsolidateArgs>,
    ) -> Result<Json<Said>, McpError> {
        let body = packset_consolidate(args.apply.unwrap_or(false)).map_err(refused)?;
        Ok(Json(Said {
            text: format_consolidation(&body),
            aside: None,
        }))
    }

    #[tool(
        description = "Call this when the work showed a standing claim wrong, with the deed that showed it: retire the atom by id, so the seat stops recalling it. The pack tombstones rather than erases and keeps the record, and `why` names the deed the retraction stands on. Forget a claim that turned out wrong; do not forget one merely because this sitting disagrees with it.",
        annotations(
            title = "Forget",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_forget(
        &self,
        Parameters(args): Parameters<ForgetArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        packset_forget(&args.id, args.why.as_deref())
            .map(object)
            .map_err(refused)
    }

    #[tool(
        description = "Call this at the start of any task, before reading code or files, with the topic in a few words. The reading says what the score is: a rank from the scorers, not whether the claim is true. The fraction is how many scorers named the hit. An empty list means the pack holds nothing on it; a failure means the writer is down, which is a different thing: `packset ensure` starts one. Pass as_of to ask what the seat knew at an earlier time. Follow with ljos_island for the cluster the task touches.",
        annotations(
            title = "Search the pack",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<Json<SearchPage>, McpError> {
        let hits =
            packset_search_as_of(&args.query, 10, args.as_of.as_deref(), false).map_err(refused)?;
        let now = args.as_of.clone().unwrap_or_else(now_utc);
        let mine = seat_name();
        let reading = search_reading(hits.len()).to_string();
        Ok(Json(SearchPage {
            reading,
            rows: hits
                .into_iter()
                .map(|h| HitRow {
                    id: h.id,
                    kind: h.kind,
                    text: h.text,
                    score: h.score,
                    age: age_of(h.ts.as_deref(), &now),
                    from: other_seat(&h.entities, &mine),
                    ts: h.ts,
                })
                .collect(),
        }))
    }

    #[tool(
        description = "Call this when a number the seat tracks has been measured again: a benchmark score, a latency, a count. With a value it takes the reading, closes the habit's earlier reading (kept as `was`), and sets the next reading due one cadence on, so ljos_due and the hook say when it is late. Without a value it lists the habits as they stand, each with the change since the last reading and when the next is due. ljos_search with as_of answers what a habit stood at then.",
        annotations(
            title = "Take or read a habit's reading",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_habit(
        &self,
        Parameters(args): Parameters<HabitArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        let now = now_utc();
        if let (Some(name), Some(value)) = (&args.name, args.value) {
            let every_s = parse_every(args.every.as_deref().unwrap_or("7d")).map_err(refused)?;
            let (body, prev) = habit(
                name,
                value,
                args.unit.as_deref().unwrap_or(""),
                every_s,
                args.source.as_deref().unwrap_or(""),
            )
            .map_err(refused)?;
            return Ok(object(serde_json::json!({
                "reading": body,
                "was": prev.map(|p| serde_json::json!({"value": p.value, "ts": p.ts, "id": p.id})),
            })));
        }
        let rows: Vec<ReadingRow> = habits()
            .map_err(refused)?
            .into_iter()
            .filter(|r| args.name.as_deref().is_none_or(|n| r.name == n.trim()))
            .map(|r| ReadingRow {
                change: format_change(&r, &now),
                age: age_of(r.ts.as_deref(), &now),
                name: r.name,
                value: r.value,
                unit: r.unit,
                due_at: r.due_at,
                source: r.source,
                id: r.id,
            })
            .collect();
        Ok(object(serde_json::json!(rows)))
    }

    // ---- the deed store ----------------------------------------------------

    #[tool(
        description = "Call this before standing on a deed somebody cited: whether its bytes are intact and its sources are too. The deed store answers; an accession it does not hold is a failure, not an empty answer.",
        annotations(
            title = "Evidence a deed",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_evidence(
        &self,
        Parameters(args): Parameters<AccessionArgs>,
    ) -> Result<Json<Said>, McpError> {
        habitat("deedar", &["evidence", &args.accession])
    }

    #[tool(
        description = "Call this on every deed a handover or an issue names: whether it is still the tip or a later take superseded it. A citation that resolves and is stale is worse than one that fails, because nothing complains.",
        annotations(
            title = "Is a deed current",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_current(
        &self,
        Parameters(args): Parameters<AccessionArgs>,
    ) -> Result<Json<Said>, McpError> {
        habitat("deedar", &["current", &args.accession])
    }

    // ---- the tracker ---------------------------------------------------------

    #[tool(
        description = "Call this after the work produced something and the deed store minted it (deedar create): cite the accession on the issue. Omit the accession to list what the issue cites. A citation names the accession; the bytes stay in deedar.",
        annotations(
            title = "Cite a deed",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_deed(
        &self,
        Parameters(args): Parameters<DeedArgs>,
    ) -> Result<Json<Said>, McpError> {
        match &args.add {
            Some(a) => {
                let mut said = habitat("vissue", &["deed", &args.issue, "--add", a])?;
                said.0
                    .text
                    .push_str(&ljos_cli::persist_tracker(&args.issue, "cited a deed"));
                Ok(said)
            }
            None => habitat("vissue", &["deed", &args.issue]),
        }
    }

    #[tool(
        description = "Call this before claiming an issue: its plan, what its inputs produced, and the deeds it has cited so far. The issue id is the tracker id (proj-1a2b).",
        annotations(
            title = "Recall a node",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_recall(
        &self,
        Parameters(args): Parameters<IssueArgs>,
    ) -> Result<Json<Said>, McpError> {
        habitat("vissue", &["recall", &args.issue])
    }

    #[tool(
        description = "Call this after ljos_recall when the order of events matters, or when a question is about when: the issue's timeline, one dated list oldest first across the three stores, the tracker's logbook (creation, state changes, claims, notes), the deeds it cites with the time each was produced, and the memories its title activates with the time each was written. Each line carries its age and the gap since the line before; a later line supersedes an earlier one on the same matter. Read-only.",
        annotations(
            title = "Timeline of an issue",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_timeline(
        &self,
        Parameters(args): Parameters<IssueArgs>,
    ) -> Result<Json<Said>, McpError> {
        let text = timeline(&args.issue, 60).map_err(refused)?;
        Ok(Json(Said { text, aside: None }))
    }

    #[tool(
        description = "Call this when a decision has more than one defensible answer: cast one ballot, or omit the option to read the count. Pass `confidence` in (0, 1], the probability that the choice is the outcome (DeGroot 1974, doi:10.1080/01621459.1974.10480137). Omit it only when you are not forecasting; a hard vote is not scored. Pass `expect` as the option you think the others will pick, or an object of option to share: that private forecast is what the surprisingly popular reading needs, and it belongs on this ballot rather than a later call. Pass `used` as comma-separated deed accessions, or `none` when the ballot used no deed (doi:10.1007/3-540-44503-X_20). The text that comes back is a count, not the settle. Call ljos_consensus before acting. Pass `as` for a persona.",
        annotations(
            title = "Vote",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_vote(
        &self,
        Parameters(args): Parameters<VoteArgs>,
    ) -> Result<Json<Said>, McpError> {
        match &args.choice {
            Some(c) => {
                let used = args.used.as_deref().unwrap_or("");
                if used.is_empty() {
                    return Err(refused(anyhow::anyhow!(
                        "a ballot records what it used (doi:10.1007/3-540-44503-X_20); pass used as deed accessions or none"
                    )));
                }
                let mut argv = vec![
                    "vote".to_string(),
                    args.issue.clone(),
                    "--for".into(),
                    c.clone(),
                    "--used".into(),
                    used.to_string(),
                ];
                if let Some(p) = args.confidence {
                    argv.push("--confidence".into());
                    argv.push(format!("{p}"));
                }
                let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                let mut said = habitat_as("vissue", &refs, args.as_persona.as_deref())?;
                if let Some(text) = args.expect.as_ref().and_then(expect_text) {
                    let who = identity_or_seat(args.as_persona.as_deref())
                        .unwrap_or_else(|| "seat".to_string());
                    write_prediction(&args.issue, &who, &text).map_err(refused)?;
                    said.0
                        .text
                        .push_str(&format!("forecast recorded for {who}\n"));
                }
                said.0
                    .text
                    .push_str(&ljos_cli::persist_tracker(&args.issue, "ballot cast"));
                Ok(said)
            }
            None => habitat("vissue", &["vote", &args.issue]),
        }
    }

    // ---- the session graph --------------------------------------------------

    #[tool(
        description = "Call this before starting work on an issue, after ljos_recall: it takes the session node for that tracker id under your name. A named worker occupies one slot (busy names the held node and ljos_complete / ljos_release). A harness seat occupies per issue. Completing a node later does not close the ticket.",
        annotations(
            title = "Claim a node",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_claim(
        &self,
        Parameters(args): Parameters<TakeArgs>,
    ) -> Result<Json<Said>, McpError> {
        let text =
            claim(&args.node, &resolve_assignee(args.assignee.as_deref())).map_err(refused)?;
        Ok(Json(Said { text, aside: None }))
    }

    #[tool(
        description = "Call this to begin work on an issue; it is the whole opening of a sitting in the protocol's order and stops at the first store that does not answer: doctor, cards, the review clock, the island the issue's title activates, the playbook copied before recall, the working set, and the claim. Pass playbook to name the recipe (sit, arena, land, company-panel, overnight); absent, a closed-set token in the title else sit. Sitting always binds one of the five before claim. The name is a tracker playbook: note until finish or release. Occupancy is the runner session (`*_SESSION_ID`) then `{name}:{issue}`: two conversations hold two tickets. Omit assignee.",
        annotations(
            title = "Open a sitting",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_sitting(
        &self,
        Parameters(args): Parameters<SittingArgs>,
    ) -> Result<Json<Said>, McpError> {
        let text = sitting_gated(
            &args.issue,
            &resolve_assignee(args.assignee.as_deref()),
            &self.cards_dir,
            args.anyway,
            args.playbook.as_deref(),
        )
        .map_err(refused)?;
        Ok(Json(Said { text, aside: None }))
    }

    #[tool(
        description = "Call this when the work on an issue ends; it is the whole closing of a sitting: remember the lesson, fire the island so its links gain weight, complete the session node, and learn from the outcome when one is named. Pass the lesson: a sitting that taught nothing worth two sentences is rare, and the report says so when none is given.",
        annotations(
            title = "Close a sitting",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_finish(
        &self,
        Parameters(args): Parameters<FinishSittingArgs>,
    ) -> Result<Json<Said>, McpError> {
        let text = finish(
            &args.issue,
            args.status.as_deref().unwrap_or("done"),
            args.lesson.as_deref(),
            args.outcome.as_deref(),
            LEARN_BETA,
            &resolve_assignee(args.assignee.as_deref()),
            args.gen,
            args.close,
        )
        .map_err(refused)?;
        Ok(Json(Said { text, aside: None }))
    }

    #[tool(
        description = "Call this once a project has a few voted issues, and again when it has many more: estimate each voter's accuracy from the project's voting history with no truth labels (Dawid and Skene) and write the accuracies back as trust rows, so a consensus stops being a count even when nobody named an outcome.",
        annotations(
            title = "Calibrate the voters",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_calibrate(
        &self,
        Parameters(args): Parameters<CalibrateArgs>,
    ) -> Result<Json<Rows<TrustRow>>, McpError> {
        let rows = calibrate(&args.project, args.rounds.unwrap_or(20)).map_err(refused)?;
        Ok(as_rows(
            rows.into_iter()
                .map(|r| TrustRow {
                    from: r.from,
                    to: r.to,
                    weight: r.weight,
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Call this when you stop working on an issue without finishing it: the session node goes back to ready, your name is free to claim again, and the generation moves. Not for finished work; that is ljos_complete.",
        annotations(
            title = "Hand a node back",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_release(
        &self,
        Parameters(args): Parameters<TakeArgs>,
    ) -> Result<Json<Said>, McpError> {
        let mut text =
            release(&args.node, &resolve_assignee(args.assignee.as_deref())).map_err(refused)?;
        text.push_str(&ljos_cli::persist_tracker(&args.node, "released"));
        Ok(Json(Said { text, aside: None }))
    }

    #[tool(
        description = "Call this when the work on an issue is finished, failed, or cancelled: the session node goes terminal. Completing is not closing; the ticket stays open until the tracker changes its state. To stop without finishing, ljos_release.",
        annotations(
            title = "Complete a node",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_complete(
        &self,
        Parameters(args): Parameters<FinishArgs>,
    ) -> Result<Json<Said>, McpError> {
        let text = complete(
            &args.node,
            args.status.as_deref(),
            &resolve_assignee(args.assignee.as_deref()),
            args.gen,
        )
        .map_err(refused)?;
        Ok(Json(Said { text, aside: None }))
    }

    // ---- cards, policy, consensus -------------------------------------------

    #[tool(
        description = "Call this second in a sitting, after ljos_doctor: the cards, what the human froze. USER.md and MEMORY.md from the cards directory, read-only. A missing card prints nothing. Nothing here writes one.",
        annotations(
            title = "Read the cards",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_cards(&self, Parameters(_): Parameters<NoArgs>) -> Result<Json<Said>, McpError> {
        cards(&self.cards_dir)
            .map(|text| Json(Said { text, aside: None }))
            .map_err(refused)
    }

    #[tool(
        description = "Argv law: print the line as it would run. This is not a check, and reloading a pack is not one either; ljos-policyd is the trusted base when it is on PATH or POLICYD_BIN.",
        annotations(
            title = "Print an argv",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_policy(
        &self,
        Parameters(args): Parameters<ArgvArgs>,
    ) -> Result<Json<PolicyRow>, McpError> {
        let said = policy_with_memory(&args.argv).map_err(refused)?;
        let (argv, ruling) = said.split_once('\n').unwrap_or((said.as_str(), ""));
        Ok(Json(PolicyRow {
            argv: argv.to_string(),
            ruling: ruling.trim_end().to_string(),
            note: POLICY_TCB.to_string(),
        }))
    }

    #[tool(
        description = "Call this after the ballots are in. The first lines are the reading: act on those, not on a later count. Polarization is how far voters still sit from the mean after listening. Disagreement is how far neighbors still sit from each other. Both zero with one option means there was one option, not that a split closed. Act on the shares when polarization is about zero and two or more options were named. When polarization is away from zero, the mean is not a position the group reached. Equal weights with nobody anchored repeat the count. The JSON and the tracker text follow the reading.",
        annotations(title = "Consensus", read_only_hint = true, open_world_hint = false)
    )]
    async fn ljos_consensus(
        &self,
        Parameters(args): Parameters<IssueArgs>,
    ) -> Result<Json<Rows<Said>>, McpError> {
        // Rows scoped to a domain apply when the issue is about it; the
        // personas' anchors go to both settles.
        let shown = tracker_show_json(&args.issue).ok();
        let topic = shown
            .as_ref()
            .and_then(|v| v.get("title").and_then(|t| t.as_str()).map(topic_words))
            .unwrap_or_default();
        let tags: Vec<String> = shown
            .as_ref()
            .and_then(|v| v.get("org_tags").and_then(|t| t.as_array()).cloned())
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_str().map(str::to_lowercase))
            .collect();
        let trust = rows_about(&trust_from_pack().unwrap_or_default(), &topic);
        let personas = personas_from_pack().unwrap_or_default();
        let steps = consensus_steps_for(
            &args.issue,
            on_path("ljos-consensus"),
            on_path("vissue"),
            &trust,
            &personas,
            &tags,
        )
        .map_err(refused)?;
        let mut out = Vec::new();
        for step in steps {
            let args: Vec<&str> = step.args.iter().map(String::as_str).collect();
            out.push(habitat(step.bin, &args)?.0);
        }
        // Beside the settle, as the CLI: the surprisingly popular answer
        // when two or more voters forecast, and the voters' standing when
        // trust rows exist.
        let predictions = pack()
            .and_then(|c| {
                ljos_cli::atoms_lean(&c, &c.workspace()).context("consensus: GET /v1/atoms failed")
            })
            .map(|atoms| predictions_of(&atoms, &args.issue))
            .unwrap_or_default();
        for step in panel_steps(&args.issue, on_path("ljos-consensus"), &trust, &predictions) {
            let args: Vec<&str> = step.args.iter().map(String::as_str).collect();
            out.push(habitat(step.bin, &args)?.0);
        }
        Ok(as_rows(out))
    }

    #[tool(
        description = "Call this when a person or a checked outcome says how much one voter should weigh another: write one trust row to the pack, from weighs to at weight in (0, 1], citing the deeds it stands on. Trust is memory: the row has a validity window and a later row for the same pair supersedes it.",
        annotations(
            title = "Trust",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_trust(
        &self,
        Parameters(args): Parameters<TrustArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        let row = Trust {
            from: args.from,
            to: args.to,
            weight: args.weight,
            about: args.about,
        };
        write_trust(&row, &args.why).map(object).map_err(refused)
    }

    #[tool(
        description = "Call this beside a ballot when the question is hard and the majority may be wrong: forecast what share the others give each option. With two or more forecasts, ljos_consensus also names the surprisingly popular answer, the option whose actual share most exceeds its forecast share.",
        annotations(
            title = "Forecast the others",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_predict(
        &self,
        Parameters(args): Parameters<PredictArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        let who =
            identity_or_seat(args.as_persona.as_deref()).unwrap_or_else(|| "seat".to_string());
        let expect = match &args.expect {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        write_prediction(&args.issue, &who, &expect)
            .map(object)
            .map_err(refused)
    }

    #[tool(
        description = "Call this when the work has shown that a kind of command must never run, or must be asked about first: writes a rule to the pack, a glob over the command line with a verdict. The memory hook and `ljos policy` enforce it at the point of action, and it travels in handovers like any memory.",
        annotations(
            title = "Write a rule",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_rule(
        &self,
        Parameters(args): Parameters<RuleArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        write_rule(&Rule {
            pattern: args.pattern,
            verdict: args.verdict,
            reason: args.why,
        })
        .map(object)
        .map_err(refused)
    }

    #[tool(
        description = "Call this to start a subagent that plays a persona on an issue: the text it should begin from. The persona's view and anchor, the bound playbook's full recipe, the five named principles, the arena rubric, what the seat knows on its domains (preferences first), the issue's working set, and the one ballot it must end with. Read-only.",
        annotations(
            title = "Brief a persona",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_brief(
        &self,
        Parameters(args): Parameters<BriefArgs>,
    ) -> Result<Json<Said>, McpError> {
        let text = brief(&args.name, &args.issue).map_err(refused)?;
        Ok(Json(Said { text, aside: None }))
    }

    #[tool(
        description = "Call this when the work wants a voter with a view of its own, such as a reviewer for a broad audience or a domain expert: write a persona with a name, an anchor in [0, 1] for how far it moves off its ballot in a settle (0 never moves), a sentence or two on how it reads the work, and the domains it speaks to. Then vote with `as` set to its name; ljos_consensus reads its anchor.",
        annotations(
            title = "Write a persona",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_persona(
        &self,
        Parameters(args): Parameters<PersonaArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        let persona = Persona {
            name: args.name,
            anchor: args.anchor.unwrap_or(0.5),
            view: args.view,
            entities: args.about,
        };
        write_persona(&persona).map(object).map_err(refused)
    }

    #[tool(
        description = "Call this when the world has said which option on an issue was right. The reading gives the trust rows for the next settle, this outcome's Brier score and logarithmic score, and each voter's running calibration: mean stated probability against how often the event occurred (Dawid 1982). From the second forecast it also gives Murphy's reliability, resolution, and uncertainty. None of those scores is a trust weight. A ballot that stated no probability is not scored. A probability of 1 on an event that did not occur has an unbounded logarithmic score.",
        annotations(
            title = "Learn",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_learn(
        &self,
        Parameters(args): Parameters<LearnArgs>,
    ) -> Result<Json<LearnPage>, McpError> {
        let said = run_captured("vissue", &["vote", &args.issue, "--json"]).map_err(refused)?;
        let forecasts = forecasts_from_json(&said.stdout).map_err(refused)?;
        let ballots: Vec<(String, String)> = forecasts
            .iter()
            .map(|f| (f.agent.clone(), f.choice.clone()))
            .collect();
        // Scoped to what the issue's island is about, so a voter wrong here
        // keeps its standing elsewhere; a refuted persona listens more.
        let about = island_entities(&args.issue).unwrap_or_default();
        let (rows, moved, calibration) = learn_and_write(
            &ballots,
            &args.outcome,
            args.beta.unwrap_or(LEARN_BETA),
            &about,
            &forecasts,
        )
        .map_err(refused)?;
        let reading = learn_reading(
            rows.len(),
            moved.len(),
            &forecasts,
            &args.outcome,
            &calibration,
        );
        Ok(Json(LearnPage {
            reading,
            rows: rows
                .into_iter()
                .map(|r| TrustRow {
                    from: r.from,
                    to: r.to,
                    weight: r.weight,
                })
                .collect(),
        }))
    }

    #[tool(
        description = "Call this first in a sitting, and again whenever another tool fails: which habitats answer (binaries, pack, host key, deed store, tracker, claim graph) and whether this harness is onboarded. A tool failing is a habitat down or refusing, never an empty answer.",
        annotations(title = "Doctor", read_only_hint = true, open_world_hint = false)
    )]
    async fn ljos_doctor(&self) -> Result<Json<Rows<HabitatRow>>, McpError> {
        Ok(as_rows(
            doctor()
                .into_iter()
                .map(|h| HabitatRow {
                    name: h.name.to_string(),
                    state: h.state,
                    ok: h.ok,
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Call this after eb_package_bump on a generation bump, before anybody builds: put the bundle's modules on the tracker as child issues of the bump ticket, blockers along the dependency edges, the same ids on every run. vissue_ready then lists the modules a seat can build now, and ljos_sitting refuses the rest until their blockers close. With dry_run the rows come back and nothing is written.",
        annotations(
            title = "Plan a bump on the tracker",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_bump_plan(
        &self,
        Parameters(args): Parameters<BumpPlanArgs>,
    ) -> Result<Json<Rows<BumpRow>>, McpError> {
        let (_generation, rows) = bump_plan(
            Path::new(&args.bundle),
            &args.project,
            &args.parent,
            args.generation.as_deref(),
            args.dry_run,
        )
        .map_err(refused)?;
        Ok(as_rows(
            rows.into_iter()
                .map(|r| BumpRow {
                    id: r.id,
                    module: r.module,
                    recipe: r.recipe,
                    blockers: r.blockers,
                    result: r.result,
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Call this after eb_campaign_run or eb_campaign_status on a bump: the campaign's typed findings, one row each with its class, stage, recipe, the error line and the fix. With remember, one lesson per finding a person or a seat resolved goes to the pack under the recipe's name, so the next bump of that recipe recalls it; with issue, the state file is cited as a deed on the issue. Findings a later attempt merely got past are skipped unless all.",
        annotations(
            title = "Campaign findings",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ljos_findings(
        &self,
        Parameters(args): Parameters<FindingsArgs>,
    ) -> Result<Json<Rows<FindingRow>>, McpError> {
        let state = Path::new(&args.state);
        let campaign = read_campaign(state).map_err(refused)?;
        let remembered = if args.remember || args.issue.is_some() {
            remember_findings(state, args.issue.as_deref(), args.all).map_err(refused)?
        } else {
            Vec::new()
        };
        Ok(as_rows(
            campaign
                .findings
                .iter()
                .map(|f| {
                    let done = remembered.iter().find(|r| r.id == f.id);
                    FindingRow {
                        id: f.id.clone(),
                        status: f.status.clone(),
                        class: f.class.clone(),
                        stage: f.stage.clone(),
                        recipe: f.recipe.clone(),
                        module: f.module.clone(),
                        summary: f.summary.clone(),
                        error: f.error.clone(),
                        action: f.action.clone(),
                        lesson: done.map(|r| r.lesson.clone()).filter(|l| !l.is_empty()),
                        remembered: done.map(|r| r.result.clone()),
                    }
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Call this when another seat takes the work over: pack a slice of this seat for it, the tracker's satchel for the projects and issues named, the pack's atoms (trust rows included), the deeds both cite with their receipts, sealed, and signed when the host has a key.",
        annotations(
            title = "Handover",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_handover(
        &self,
        Parameters(args): Parameters<PackArgs>,
    ) -> Result<Json<Rows<String>>, McpError> {
        handover(Path::new(&args.out), &args.projects, &args.issues)
            .map(as_rows)
            .map_err(refused)
    }

    #[tool(
        description = "Call this when a handover directory arrived from another seat, before anything in it is trusted: check the satchel, manifest, deed receipts against the head in the bag (and the bridge from a kept head when given), the signature, and what the atoms hold. With import, the atoms go into this seat's pack.",
        annotations(
            title = "Receive",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_receive(
        &self,
        Parameters(args): Parameters<ReceiveArgs>,
    ) -> Result<Json<Rows<String>>, McpError> {
        receive(
            Path::new(&args.dir),
            args.since.as_deref().map(Path::new),
            args.import.unwrap_or(false),
        )
        .map(as_rows)
        .map_err(refused)
    }

    #[tool(
        description = "Call this after ljos_search with the task in your own words. The first lines say whose island this is. Activation is spread from search seeds along links, two hops. It is not a relevance rank. Pass `as` for a persona: the spread follows the weights that persona fired, and a fire rewrites those weights, not the seat's. Pass fire only after the island was used. A weak island does not fire. A cue that already fired inside the hour is left as it was.",
        annotations(
            title = "Island",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_island(
        &self,
        Parameters(args): Parameters<CueArgs>,
    ) -> Result<Json<Rows<IslandRow>>, McpError> {
        let now = now_utc();
        let body = packset_island_as(&args.cue, args.fire.unwrap_or(false), args.lens.as_deref())
            .map_err(refused)?;
        Ok(as_rows(
            body["island"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| IslandRow {
                    id: a["id"].as_str().map(str::to_string),
                    kind: a["kind"].as_str().unwrap_or("").to_string(),
                    text: a["text"].as_str().unwrap_or("").to_string(),
                    activation: a["activation"].as_f64().unwrap_or(0.0),
                    seed: a["seed"].as_bool().unwrap_or(false),
                    age: age_of(a["ts"].as_str(), &now),
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Call this before a panel or a persona vote: the personas the pack holds, each with its name, anchor, the domains it speaks to and its view. Write a missing one with ljos_persona.",
        annotations(title = "Personas", read_only_hint = true, open_world_hint = false)
    )]
    async fn ljos_personas(&self) -> Result<Json<Rows<PersonaRow>>, McpError> {
        Ok(as_rows(
            personas_from_pack()
                .map_err(refused)?
                .into_iter()
                .map(|p| PersonaRow {
                    name: p.name,
                    anchor: p.anchor,
                    about: p.entities,
                    view: p.view,
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Call this to name the recipe a sitting copies before personas enter: bind NAME to ISSUE and print the full body. The name is a tracker playbook: note until finish or release. Mid-sitting turns re-read it. A new task is a new sitting. A panel is refused until one is bound.",
        annotations(
            title = "Bind a playbook",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_playbook(
        &self,
        Parameters(args): Parameters<PlaybookBindArgs>,
    ) -> Result<Json<Said>, McpError> {
        let mut text = copy_playbook(&args.issue, &args.name).map_err(refused)?;
        text.push_str(&ljos_cli::persist_tracker(&args.issue, "bound a playbook"));
        Ok(Json(Said { text, aside: None }))
    }

    #[tool(
        description = "Call this to see the playbooks the pack holds: the closed set sit, arena, land, company-panel, overnight. Kind playbook, weighed not recalled. Pack latest per name, shipped seed only when the pack has no live atom. Model names on a recipe are optional spawn hints; every panel still ends in ljos_vote --as then ljos_consensus.",
        annotations(title = "Playbooks", read_only_hint = true, open_world_hint = false)
    )]
    async fn ljos_playbooks(&self) -> Result<Json<Rows<PlaybookRow>>, McpError> {
        Ok(as_rows(
            playbooks_from_pack()
                .map_err(refused)?
                .into_iter()
                .map(|p| PlaybookRow {
                    name: p.name,
                    models: p.models,
                    body: p.body,
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Call this at the start of a sitting, after the cards: the soonest eight claims whose review is due, plus how many are due in all. Read each shown row, then ljos_graded it recalled or lapsed. The full list and the sweep are `ljos due`. The review clock moves only when you grade.",
        annotations(title = "Due", read_only_hint = true, open_world_hint = false)
    )]
    async fn ljos_due(&self) -> Result<Json<DuePage>, McpError> {
        let (shown, due_n, summary) = due_page().map_err(refused)?;
        let rows = shown
            .into_iter()
            .map(|a| DueRow {
                id: a["id"].as_str().map(str::to_string),
                kind: a["kind"].as_str().unwrap_or("").to_string(),
                text: a["text"].as_str().unwrap_or("").to_string(),
                due_at: a["due_at"].as_str().unwrap_or("").to_string(),
            })
            .collect();
        Ok(Json(DuePage {
            rows,
            due: due_n,
            summary,
        }))
    }

    #[tool(
        description = "Call this for each claim ljos_due listed once you have read it: recalled (default) pushes the next review out, lapsed brings it back sooner. Returns the atom with its new due_at.",
        annotations(
            title = "Graded",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ljos_graded(
        &self,
        Parameters(args): Parameters<GradeArgs>,
    ) -> Result<Json<serde_json::Map<String, serde_json::Value>>, McpError> {
        graded(&args.id, args.recalled.unwrap_or(true))
            .map(object)
            .map_err(refused)
    }
}

// ---- prompts ---------------------------------------------------------------

/// A node to pick up.
#[derive(Deserialize, JsonSchema)]
pub struct PickUpArgs {
    /// The tracker id of the work, when known.
    pub issue: Option<String>,
}

/// An eb-stack campaign state to read or remember.
#[derive(Deserialize, JsonSchema)]
pub struct FindingsArgs {
    /// The campaign state file (`campaign.json`) `eb_campaign_run` keeps.
    pub state: String,
    /// Write one lesson per finding a person or a seat resolved.
    #[serde(default)]
    pub remember: bool,
    /// With remember, every finding, the ones a later attempt superseded too.
    #[serde(default)]
    pub all: bool,
    /// Cite the state file as a deed on this issue.
    #[serde(default)]
    pub issue: Option<String>,
}

/// A bundle to put on the tracker.
#[derive(Deserialize, JsonSchema)]
pub struct BumpPlanArgs {
    /// The bundle directory `eb_package_bump` wrote (`out_dir`).
    pub bundle: String,
    /// The tracker project the module issues go in.
    pub project: String,
    /// The bump ticket every module issue is a child of.
    pub parent: String,
    /// The generation named in titles and ids; default the lock's toolchain.
    #[serde(default)]
    pub generation: Option<String>,
    /// Print the rows and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// One module of a bump as the tracker holds it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BumpRow {
    /// The issue id, the same on every run.
    pub id: String,
    /// The module as EasyBuild names it.
    pub module: String,
    /// The recipe path the lock names, when it does.
    pub recipe: String,
    /// The issues this one is blocked by: the modules built before it.
    pub blockers: Vec<String>,
    /// `made`, `held` (it existed), or `would make`.
    pub result: String,
}

/// One typed finding of a campaign, and what the seat did with it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FindingRow {
    /// `attempt:N:finding:M`, stable across runs.
    pub id: String,
    /// `open`, `in-progress`, `resolved`, `superseded`.
    pub status: String,
    /// `compile`, `configure`, `dependency-missing`, `resource`, ...
    pub class: String,
    /// The build stage that failed.
    pub stage: String,
    /// The recipe the campaign drives, as its file stem.
    pub recipe: String,
    /// The module whose build failed, when the evidence names one; a
    /// campaign fails in its dependencies more often than in its recipe.
    pub module: String,
    /// One line to triage from.
    pub summary: String,
    /// The last error line the evidence carries.
    pub error: String,
    /// The resolution's action, when resolved.
    pub action: String,
    /// The lesson written for it, when remember was asked.
    pub lesson: Option<String>,
    /// The pack's answer to that lesson: an atom id, or the refusal.
    pub remembered: Option<String>,
}

/// A handover to check.
#[derive(Deserialize, JsonSchema)]
pub struct HandoverArgs {
    /// The satchel directory that arrived.
    pub dir: String,
}

fn asked(text: String) -> Vec<PromptMessage> {
    vec![PromptMessage::new_text(Role::User, text)]
}

#[prompt_router(vis = "pub(crate)")]
impl LjosServer {
    /// Start a sitting: read the cards, ask the pack, then take the work.
    #[prompt(name = "start_a_sitting")]
    pub async fn start_a_sitting_prompt(
        &self,
        Parameters(args): Parameters<PickUpArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let work = args.issue.filter(|i| !i.trim().is_empty()).map_or_else(
            || "what the tracker says is ready".to_string(),
            |i| format!("tracker node {i}"),
        );
        Ok(asked(format!(
            "Begin a sitting on {work}.\n\
             \n\
             Prefer `ljos_sitting` for the whole opening and `ljos_finish` for\n\
             the close. Neither completing a session node nor `ljos_finish`\n\
             closes a ticket; `ljos_finish` with close does, when the work is\n\
             accepted.\n\
             \n\
             By hand, in order:\n\
             \n\
             1. `ljos_doctor`. A habitat that refuses is the answer.\n\
             2. `ljos_cards`. What the human froze. Read them and leave them.\n\
             3. `ljos_due` returns the soonest eight claims and the total due.\n\
                Read each shown row and `ljos_graded` it.\n\
             4. `ljos_search` then `ljos_island` with the task in your own words.\n\
             5. `ljos_playbook` on the node with a recipe name (sit, arena, land,\n\
                company-panel, overnight) unless this sitting already holds one.\n\
                Absent a name, the sitting matches the title or binds sit. The sitting\n\
                copies the full body before recall; a panel is refused until one is bound.\n\
             6. `ljos_recall` on the node. What it stands on and what it cited.\n\
             7. `ljos_timeline` the last twelve dated events across the stores.\n\
             8. `ljos_claim` a session node for it. A harness seat occupies per\n\
                issue; a named worker occupies one slot.\n\
             \n\
             When something is learned that will still be true next sitting, say it\n\
             with `ljos_remember` in two short sentences. When the work shows a\n\
             standing claim was wrong, `ljos_forget` it and name the deed that\n\
             showed it; disagreeing with a claim is not showing it wrong. When the\n\
             work produces something, mint the deed in the deed store and\n\
             `ljos_deed` it on the node. Completing the session node does not\n\
             close the ticket. A tool that fails is a habitat refusing or down:\n\
             `ljos_doctor` says which."
        )))
    }

    /// Run a panel: one subagent per persona in the pack, each voting as
    /// itself, then the settle under the trust rows.
    #[prompt(name = "run_a_panel")]
    pub async fn run_a_panel_prompt(
        &self,
        Parameters(args): Parameters<IssueArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let issue = args.issue;
        // Only the personas whose domains the issue speaks to sit; a seat
        // that runs every persona on every issue is a count, not a panel.
        let all = personas_from_pack().unwrap_or_default();
        let personas = personas_speaking_to(&all, &issue_words(&issue));
        let roster = if all.is_empty() {
            "The pack holds no personas yet. Write two or three with `ljos_persona` first: a \
             name, an anchor in [0, 1] (0 never moves off its ballot), a sentence on how it \
             reads the work, and the domains it speaks to."
                .to_string()
        } else if personas.is_empty() {
            "No persona's domains are among this issue's words. Write one with `ljos_persona` \
             whose `--about` is one of those words, or a persona with no domains. Specialists \
             stay seated out."
                .to_string()
        } else {
            personas
                .iter()
                .map(|p| {
                    format!(
                        "- {} (anchor {:.2}{}): {}",
                        p.name,
                        p.anchor,
                        if p.entities.is_empty() {
                            String::new()
                        } else {
                            format!(", about {}", p.entities.join(", "))
                        },
                        p.view
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let seated = if personas.len() < all.len() {
            format!(
                "The personas whose domains this issue speaks to ({} of {} in the pack):",
                personas.len(),
                all.len()
            )
        } else {
            "The personas in this seat's pack:".to_string()
        };
        Ok(asked(format!(
            "Run a panel on {issue}.\n\n\
             A panel is refused until a playbook is bound: `ljos_playbook` with {issue} and a \
             recipe name, or `ljos_sitting` with playbook set. Mid-sitting turns re-read that \
             recipe; a new task is a new sitting. Model names on the recipe are optional spawn \
             hints; every member still ends with vote --as then consensus.\n\n\
             {seated}\n{roster}\n\n\
             1. `ljos_recall` on {issue}, and `ljos_search` for what the seat knows about it.\n\
             2. For each persona, `ljos_brief` with its name and {issue}, and start one \
                subagent with that text as its whole brief: the persona's view, the bound \
                playbook's full recipe, the five named principles, the arena rubric, what the \
                seat knows on its domains, the working set. Each subagent reads the work in its \
                own way: `ljos_island` on {issue} with `as` set to its name walks the pack through \
                its own weights. The number on a row is spread along those links, not a rank. \
                Pass `fire` true only after it has used that island: fire rewrites its weights, \
                not the seat's, and the next walk follows them. `ljos_remember` with `as` writes what it \
                concluded into its own set. It ends by casting exactly one ballot: `ljos_vote` \
                on {issue} with `as` set to the persona's name, for the option it would defend. \
                The ballot carries `confidence` in (0, 1], `expect` as what it thinks the others \
                will pick, and `used` as the deeds it drew on, or `none`. \
                The line that comes back is a count. The settle is the next step. \
                Subagents run in parallel and do not see each other's ballots. One playbook \
                step per subagent; do not resume across phases.\n\
             3. `ljos_consensus` on {issue}. Read the first lines and act on them. \
                Polarization is how far voters still sit from the mean. Disagreement is \
                how far neighbors still sit from each other. Both zero with one option \
                means there was one option. Act on the shares when polarization is about \
                zero and two or more options were named. When polarization is away from \
                zero, the mean is not a position the group reached. A count printed later \
                is who voted. Act on the settle, not on the count.\n\
             4. When the world later says which option \
                was right, `ljos_learn` on {issue} with that outcome, and the personas that \
                were wrong lose weight on this topic.\n\n\
             Equal weights with nobody anchored repeat the count. Anchors still hold each \
             voter to its ballot. Run `ljos_calibrate` on the project once it \
             holds a few voted issues, so the rows carry what the personas' history says."
        )))
    }

    /// Check a handover somebody sent, in the order the questions come.
    #[prompt(name = "check_a_handover")]
    pub async fn check_a_handover_prompt(
        &self,
        Parameters(args): Parameters<HandoverArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let dir = args.dir;
        Ok(asked(format!(
            "Check the handover at {dir}.\n\
             \n\
             `ljos_receive` on {dir} answers three questions in order, each\n\
             unanswered by the one before it: did it arrive as written (the\n\
             manifest), do the deeds predate the asking (every receipt to the head\n\
             in the bag, and the bridge from a kept head when `since` is given),\n\
             and who wrote it (the signature, when there is one). Pass `since` when\n\
             this sender has handed over before.\n\
             \n\
             Then, for each deed the bag names, `ljos_current`: a deed that arrived\n\
             intact and is no longer the tip is a different finding from one that\n\
             failed. Report which questions passed; a bag that fails one has not\n\
             mostly checked out. Only when all pass, `ljos_receive` again with\n\
             `import` so the atoms, trust rows included, join this seat's pack."
        )))
    }
}

/// Set on the one-call child a stale server forwards to, so the child
/// answers itself rather than forwarding again. Its seat record is keyed
/// on the stale server's process, which holds none of the runner's.
pub const FORWARDED: &str = "LJOS_MCP_FORWARDED";

/// The installed binary, when the one this server runs from has been
/// replaced on disk since it started. A runner keeps its server for the
/// whole conversation, so without this an install reached no running agent.
fn replaced_binary() -> Option<PathBuf> {
    if std::env::var_os(FORWARDED).is_some() {
        return None;
    }
    let exe = std::fs::read_link("/proc/self/exe").ok()?;
    let path = exe.to_str()?.strip_suffix(" (deleted)")?;
    let path = PathBuf::from(path);
    path.is_file().then_some(path)
}

/// Answer one tool call with the installed binary, typed for the server.
fn forward_call(
    fresh: &Path,
    init: Option<serde_json::Value>,
    request: rmcp::model::CallToolRequestParams,
    meta: serde_json::Map<String, serde_json::Value>,
) -> Result<rmcp::model::CallToolResponse, McpError> {
    let fail = |e: &dyn std::fmt::Display| {
        McpError::internal_error(format!("forward to {}: {e}", fresh.display()), None)
    };
    let mut params = serde_json::to_value(&request).map_err(|e| fail(&e))?;
    if !meta.is_empty() {
        params["_meta"] = serde_json::Value::Object(meta);
    }
    let answer = ljos_cli::mcp_forward(fresh, FORWARDED, init, params).map_err(|e| fail(&e))?;
    if let Some(err) = answer.get("error") {
        return Err(serde_json::from_value(err.clone())
            .unwrap_or_else(|_| McpError::internal_error(err.to_string(), None)));
    }
    serde_json::from_value::<rmcp::model::CallToolResult>(answer["result"].clone())
        .map(Into::into)
        .map_err(|e| fail(&e))
}

#[tool_handler]
#[prompt_handler(router = Self::prompt_router())]
impl ServerHandler for LjosServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new("ljos", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "One seat over five habitats: tracker, pack, deed store, claim graph, \
             consensus. Read ljos://protocol first; it says which store answers \
             which question and the order of tools in a sitting. ljos_sitting \
             runs the whole opening (doctor, cards, due, island, playbook, recall, timeline, claim) \
             and ljos_finish the whole closing (remember, fire, complete, learn); \
             prefer them. By hand: ljos_doctor, ljos_cards, ljos_due then \
             ljos_graded, ljos_search then ljos_island, ljos_playbook, ljos_recall, \
             ljos_timeline, ljos_claim; \
             during the work ljos_deed, ljos_remember, ljos_vote; after it \
             ljos_island with fire, ljos_complete, ljos_learn. ljos_calibrate \
             moves the trust rows from a project's history when nobody names an \
             outcome. \
             The pack is written by remember, prefer, trust, learn, graded and an \
             imported handover; the text is the claim. Cards are read at \
             ljos://cards/ and never written. Citing a deed on a node names an \
             accession and does not paste the product. Neither completing a session \
             node nor ljos_finish closes a ticket; ljos_finish with close does, \
             when the work is accepted, so a blocker on it holds until then; \
             ljos_release hands one back unfinished. A \
             tool that fails means a habitat refused or is not running; it is not \
             an empty answer. The seat is named after the client that connected. \
             The holder is any `*_SESSION_ID` the runner stamped, the whole value, \
             else that name tagged with the runner's process; a shell the runner \
             opens finds the same names, so no field here needs an assignee.",
        )
    }

    /// One tool call, held as the thread the call names in its `_meta`
    /// when the runner names one there: a runner that serves many
    /// conversations from one server puts no conversation id in the
    /// server's environment.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        if let Some(fresh) = replaced_binary() {
            let init = context
                .peer
                .peer_info()
                .and_then(|c| serde_json::to_value(&*c).ok());
            let meta = match serde_json::to_value(&context.meta.0) {
                Ok(serde_json::Value::Object(map)) => map,
                _ => serde_json::Map::new(),
            };
            return tokio::task::spawn_blocking(move || forward_call(&fresh, init, request, meta))
                .await
                .map_err(|e| McpError::internal_error(format!("forward: {e}"), None))?;
        }
        let thread = thread_in_meta(&context.meta.0);
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        ljos_cli::as_thread(thread, Self::tool_router().call(tcc)).await
    }

    /// The client has said who it is: name the seat after it and leave the
    /// record a shell below the same runner reads.
    async fn on_initialized(&self, context: rmcp::service::NotificationContext<rmcp::RoleServer>) {
        let client = context
            .peer
            .peer_info()
            .map(|info| info.client_info.name.clone())
            .unwrap_or_else(|| "runner".to_string());
        announce_seat(&client, runner_pid());
    }

    /// The two cards and the protocol, and nothing else.
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut protocol = Resource::new(PROTOCOL_URI, "protocol".to_string());
        protocol.title = Some("The seat protocol".into());
        protocol.description = Some(
            "Which store answers which question, the order of tools before, during and \
             after the work, and the refusals worth knowing. Read this first."
                .into(),
        );
        protocol.mime_type = Some("text/markdown".to_string());
        Ok(ListResourcesResult::with_all_items(
            std::iter::once(protocol)
                .chain(CARD_NAMES.iter().map(|name| {
                    let mut resource =
                        Resource::new(format!("{SCHEME}://cards/{name}"), (*name).to_string());
                    resource.title = Some(format!("{name}, frozen by the human"));
                    resource.description = Some(
                        "A card. Read-only; overflow is an error, not a prompt to write more."
                            .into(),
                    );
                    resource.mime_type = Some("text/markdown".to_string());
                    resource
                }))
                .collect(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri.clone();
        if uri == PROTOCOL_URI {
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(PROTOCOL, uri)]).into());
        }
        let name = card_named(&uri).ok_or_else(|| {
            McpError::resource_not_found(
                format!(
                    "{uri}: the resources are {PROTOCOL_URI}, {SCHEME}://cards/USER.md and \
                     {SCHEME}://cards/MEMORY.md"
                ),
                None,
            )
        })?;
        let path = self.cards_dir.join(name);
        // A missing card is an empty card; nothing is created.
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri)]).into())
    }
}

/// The card a resource uri names; anything else, including a third file in
/// the same directory, is nothing.
fn card_named(uri: &str) -> Option<&'static str> {
    let name = uri.strip_prefix(&format!("{SCHEME}://cards/"))?;
    CARD_NAMES.iter().copied().find(|card| *card == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_names_its_thread_in_the_meta() {
        let meta = |v: serde_json::Value| v.as_object().unwrap().clone();
        assert_eq!(
            thread_in_meta(&meta(serde_json::json!({
                "progressToken": 1,
                "x-runner-turn-metadata": {"session_id": "s-1", "thread_id": "0199a1b2-thread"}
            }))),
            Some("0199a1b2-thread".to_string())
        );
        assert_eq!(
            thread_in_meta(&meta(serde_json::json!({"thread_id": "top-level-thread"}))),
            Some("top-level-thread".to_string())
        );
        assert_eq!(
            thread_in_meta(&meta(serde_json::json!({"progressToken": 1}))),
            None
        );
    }

    /// Every tool is annotated, and the writers are the ones the contract names.
    #[test]
    fn the_writers_are_the_ones_the_contract_names() {
        let tools = LjosServer::tool_router().list_all();
        assert_eq!(
            tools.len(),
            39,
            "{:?}",
            tools.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
        let mut writers: Vec<String> = Vec::new();
        for tool in &tools {
            let hints = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} carries no annotations", tool.name));
            assert_eq!(hints.open_world_hint, Some(false), "{}", tool.name);
            match hints.read_only_hint {
                Some(true) => {}
                Some(false) => {
                    // Forget is the one verb that takes something away. Every
                    // other writer adds, so saying so is the whole annotation.
                    let expected = Some(tool.name == "ljos_forget");
                    assert_eq!(
                        hints.destructive_hint, expected,
                        "{} misreports whether it destroys",
                        tool.name
                    );
                    writers.push(tool.name.to_string());
                }
                None => panic!("{} does not say whether it writes", tool.name),
            }
        }
        writers.sort();
        assert_eq!(
            writers,
            [
                "ljos_bump_plan",
                "ljos_calibrate",
                "ljos_claim",
                "ljos_complete",
                "ljos_consolidate",
                "ljos_deed",
                "ljos_findings",
                "ljos_finish",
                "ljos_forget",
                "ljos_graded",
                "ljos_habit",
                "ljos_handover",
                "ljos_island",
                "ljos_learn",
                "ljos_persona",
                "ljos_playbook",
                "ljos_predict",
                "ljos_prefer",
                "ljos_receive",
                "ljos_release",
                "ljos_remember",
                "ljos_rule",
                "ljos_sitting",
                "ljos_trust",
                "ljos_vote"
            ]
        );
    }

    /// A refusal is an error, not a success carrying an error message.
    #[tokio::test]
    async fn a_refusal_is_an_error() {
        let server = LjosServer::at(std::env::temp_dir());
        let Err(err) = server
            .ljos_evidence(Parameters(AccessionArgs {
                accession: "deed-file-nobody-minted-this".into(),
            }))
            .await
        else {
            panic!("a missing deed came back as an answer");
        };
        let said = format!("{err:?}");
        assert!(
            said.contains("not on PATH") || said.contains("exited") || said.contains("not found"),
            "{said}"
        );
    }

    /// Cards are the two named files; nothing is created; nothing else reads.
    #[tokio::test]
    async fn cards_are_read_and_never_made() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("USER.md"), "the user\n").expect("write");
        let server = LjosServer::at(dir.path().to_path_buf());
        let out = server
            .ljos_cards(Parameters(NoArgs {}))
            .await
            .expect("reads");
        assert!(out.0.text.contains("the user"));
        assert!(
            !dir.path().join("MEMORY.md").exists(),
            "a missing card was created"
        );

        assert_eq!(card_named("ljos://cards/USER.md"), Some("USER.md"));
        assert_eq!(card_named("ljos://cards/MEMORY.md"), Some("MEMORY.md"));
        assert_eq!(card_named("ljos://cards/NOTES.md"), None);
        assert_eq!(card_named("ljos://cards/../USER.md"), None);
        assert_eq!(card_named("file:///etc/passwd"), None);
    }

    /// The protocol is a resource every harness can read, and it names the
    /// tools in the order a sitting calls them.
    #[test]
    fn the_protocol_is_served_and_orders_the_sitting() {
        assert_eq!(card_named(PROTOCOL_URI), None, "the protocol is not a card");
        for verb in [
            "ljos doctor",
            "ljos cards",
            "ljos due",
            "ljos search",
            "ljos island",
            "ljos recall",
            "ljos claim",
        ] {
            assert!(PROTOCOL.contains(verb), "{verb} missing from the protocol");
        }
        let order: Vec<usize> = [
            "## Before the work",
            "## During the work",
            "## After the work",
        ]
        .iter()
        .map(|h| PROTOCOL.find(h).unwrap_or_else(|| panic!("{h} missing")))
        .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]));
        // Every tool the server declares is named in the protocol, so a reader
        // of the protocol has heard of everything the server can do.
        for tool in LjosServer::tool_router().list_all() {
            let verb = tool.name.trim_start_matches("ljos_").to_string();
            assert!(
                PROTOCOL.contains(&format!("`{verb}`"))
                    || PROTOCOL.contains(&format!("ljos {verb}")),
                "tool {verb} is not in the protocol"
            );
        }
    }

    /// Every prompt renders from what it declares.
    #[tokio::test]
    async fn every_prompt_renders() {
        fn text(message: &PromptMessage) -> &str {
            &message.content.as_text().expect("a text prompt").text
        }
        fn ordered(said: &str, verbs: &[&str]) {
            let at: Vec<usize> = verbs
                .iter()
                .map(|v| {
                    said.find(v)
                        .unwrap_or_else(|| panic!("{v} missing: {said}"))
                })
                .collect();
            assert!(
                at.windows(2).all(|w| w[0] < w[1]),
                "{verbs:?} out of order: {said}"
            );
        }
        let declared = LjosServer::prompt_router().list_all();
        let mut names: Vec<&str> = declared.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["check_a_handover", "run_a_panel", "start_a_sitting"]
        );
        let server = LjosServer::at(std::env::temp_dir());
        let panel = server
            .run_a_panel_prompt(Parameters(IssueArgs {
                issue: "proj-1a2b".into(),
            }))
            .await
            .expect("renders");
        let said = text(&panel[0]);
        assert!(said.contains("proj-1a2b"), "{said}");
        ordered(
            said,
            &[
                "`ljos_recall`",
                "`ljos_brief`",
                "`ljos_vote`",
                "`ljos_consensus`",
                "`ljos_learn`",
                "`ljos_calibrate`",
            ],
        );
        let server = LjosServer::at(std::env::temp_dir());
        let begun = server
            .start_a_sitting_prompt(Parameters(PickUpArgs {
                issue: Some("proj-1a2b".into()),
            }))
            .await
            .expect("renders");
        let said = text(&begun[0]);
        assert!(said.contains("proj-1a2b"), "{said}");
        ordered(
            said,
            &[
                "`ljos_sitting`",
                "`ljos_finish`",
                "`ljos_doctor`",
                "`ljos_cards`",
                "`ljos_due`",
                "`ljos_graded`",
                "`ljos_search`",
                "`ljos_island`",
                "`ljos_playbook`",
                "`ljos_recall`",
                "`ljos_claim`",
                "`ljos_remember`",
                "`ljos_forget`",
                "`ljos_deed`",
            ],
        );
        let checked = server
            .check_a_handover_prompt(Parameters(HandoverArgs {
                dir: "/tmp/bag".into(),
            }))
            .await
            .expect("renders");
        let said = text(&checked[0]);
        assert!(said.contains("/tmp/bag"), "{said}");
        ordered(said, &["`ljos_receive`", "`ljos_current`", "`import`"]);
    }
}
