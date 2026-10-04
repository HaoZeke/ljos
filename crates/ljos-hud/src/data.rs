//! One snapshot of the habitats the pane shows. Read-only.

use anyhow::{Context, Result};
use claimdag::{WorkGraph, WorkId, WorkStatus};
use serde_json::Value;

use crate::graph::GraphLayout;

/// One due atom, with the review clock and last grade as display fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueRow {
    pub id: String,
    pub kind: String,
    pub text: String,
    /// Review clock instant, empty when the atom has never been graded.
    pub due_at: String,
    /// `overdue`, `later`, or `ungraded`. Display only. No exact-equality `due`.
    pub clock: String,
    /// `recalled`, `lapsed`, or `ungraded`. Display only; never a POST.
    pub grade: String,
}

/// One live claim-graph node, with lease and occupancy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRow {
    pub id: String,
    pub status: String,
    pub summary: String,
    /// First twelve hex chars, or `-` when nobody holds it.
    pub assignee: String,
    pub cas_gen: u64,
    /// Live Claimed|Running nodes with this assignee.
    pub occupancy: u32,
    pub updated_unix: u64,
}

/// One memory the cue activated. `fire` was false.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IslandRow {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub activation: String,
    pub seed: bool,
}

/// One search hit for the same cue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitRow {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub score: String,
}

/// Filesystem plus pack stamp. Tick is chrome; a change here reloads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchStamp {
    pub work_mtime: u128,
    pub pack_ts: String,
}

/// What the pane paints. Habitat errors become empty columns plus a banner.
/// Trust is a graph, not a list of from→to floats.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub due: Vec<DueRow>,
    pub claims: Vec<ClaimRow>,
    pub graph: GraphLayout,
    /// False when the pack GET failed. Distinct from an honest empty graph.
    pub pack_ok: bool,
    pub banner: String,
    /// `review_summary` on the same atoms as `due_of`.
    pub review_summary: String,
    pub island: Vec<IslandRow>,
    pub hits: Vec<HitRow>,
    pub events: Vec<ljos_cli::Event>,
    pub cue: String,
    pub issue: String,
    /// Weak/dense lines from [`ljos_cli::format_island`]. Empty when idle.
    pub island_notices: Vec<String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            due: Vec::new(),
            claims: Vec::new(),
            graph: GraphLayout::empty(),
            pack_ok: true,
            banner: String::new(),
            review_summary: String::new(),
            island: Vec::new(),
            hits: Vec::new(),
            events: Vec::new(),
            cue: String::new(),
            issue: String::new(),
            island_notices: Vec::new(),
        }
    }
}

impl Snapshot {
    /// Load due, live claims, trust, island, and the deed rail. Never mutates.
    ///
    /// One pack GET, then [`ljos_cli::due_of`], [`ljos_cli::review_summary`],
    /// [`ljos_cli::trust_rows`], and [`ljos_cli::personas_of`] on the same
    /// atoms. Claims are [`WorkGraph::open_dir`]. Island is
    /// [`ljos_cli::packset_search`] plus [`ljos_cli::packset_island`] with
    /// `fire` false. The rail is [`ljos_cli::timeline_events`].
    pub fn load() -> Self {
        Self::load_with_cue(None)
    }

    /// [`Self::load`] around an explicit cue. Empty cue leaves the island
    /// idle: no search, no activate. The deed rail stays idle until the
    /// cue names an issue. The operator types a cue and enters.
    pub fn load_with_cue(cue: Option<&str>) -> Self {
        let mut banner = Vec::new();
        let (due, graph, pack_ok, review_summary) = match load_pack_panes() {
            Ok(pair) => pair,
            Err(err) => {
                banner.push(format!("pack: {err}"));
                (Vec::new(), GraphLayout::empty(), false, String::new())
            }
        };
        let claims = match load_claims() {
            Ok(rows) => rows,
            Err(err) => {
                banner.push(format!("claims: {err}"));
                Vec::new()
            }
        };
        let cue = cue
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_default();
        let issue = issue_of(&cue);
        let (island, island_notices, hits) = if cue.is_empty() {
            (Vec::new(), Vec::new(), Vec::new())
        } else {
            let hits = match load_hits(&cue) {
                Ok(rows) => rows,
                Err(err) => {
                    banner.push(format!("search: {err}"));
                    Vec::new()
                }
            };
            match load_island(&cue) {
                Ok((rows, notices)) => (rows, notices, hits),
                Err(err) => {
                    banner.push(format!("island: {err}"));
                    (Vec::new(), Vec::new(), hits)
                }
            }
        };
        let events = if issue.is_empty() {
            Vec::new()
        } else {
            match ljos_cli::timeline_events(&issue, ljos_cli::SITTING_TIMELINE) {
                Ok(rows) => rows,
                Err(err) => {
                    banner.push(format!("deeds: {err}"));
                    Vec::new()
                }
            }
        };
        Self {
            due,
            claims,
            graph,
            pack_ok,
            banner: banner.join(" · "),
            review_summary,
            island,
            hits,
            events,
            cue,
            issue,
            island_notices,
        }
    }

    /// A banner-only snapshot when the off-thread load cannot join.
    /// The canvas stays empty; this must not invent personas or edges.
    /// An empty message is first paint, not a fake pack-down.
    pub fn banner_only(msg: String) -> Self {
        Self {
            pack_ok: msg.is_empty(),
            banner: msg,
            ..Self::default()
        }
    }
}

impl WatchStamp {
    /// Stat `work.bin` and GET pack `last_write_ts`. Not a habitat snapshot.
    pub fn read() -> Self {
        Self {
            work_mtime: work_bin_mtime(),
            pack_ts: ljos_cli::pack_last_write_ts()
                .ok()
                .flatten()
                .unwrap_or_default(),
        }
    }

    /// Whether `next` is a habitat write that needs a snapshot.
    ///
    /// An empty `pack_ts` on `self` is a boot fingerprint: the first load
    /// already painted, so a later GET filling the stamp is not a write.
    #[must_use]
    pub fn needs_load(&self, next: &Self) -> bool {
        if self == next {
            return false;
        }
        if self.pack_ts.is_empty() && self.work_mtime == next.work_mtime {
            return false;
        }
        true
    }
}

/// Seconds since the epoch, for lease chrome.
#[must_use]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// mtime of the claim graph snapshot, nanos since epoch. Zero if absent.
#[must_use]
pub fn work_bin_mtime() -> u128 {
    let dir = claimdag::resolve_dir(None);
    std::fs::metadata(dir.join(claimdag::SNAP_BIN))
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_nanos())
        .unwrap_or(0)
}

/// Lease seconds left. Negative when the quiet window has run out.
#[must_use]
pub fn lease_remaining_secs(updated_unix: u64, now_unix: u64) -> i64 {
    claimdag::DEFAULT_LEASE_SECS as i64 - now_unix.saturating_sub(updated_unix) as i64
}

/// `12m03s`, `expired`, or `-` when the node is not held.
#[must_use]
pub fn format_lease(status: &str, updated_unix: u64, now_unix: u64) -> String {
    if status != "claimed" && status != "running" {
        return "-".into();
    }
    let left = lease_remaining_secs(updated_unix, now_unix);
    if left <= 0 {
        "expired".into()
    } else {
        let m = left / 60;
        let s = left % 60;
        if m == 0 {
            format!("{s}s")
        } else {
            format!("{m}m{s:02}s")
        }
    }
}

/// Tracker id `proj-xxxx`.
#[must_use]
pub fn looks_like_issue(s: &str) -> bool {
    let s = s.trim().trim_end_matches([',', '.', ';', ':']);
    let Some((proj, rest)) = s.split_once('-') else {
        return false;
    };
    !proj.is_empty()
        && proj.chars().all(|c| c.is_ascii_lowercase())
        && rest.len() == 4
        && rest.chars().all(|c| c.is_ascii_alphanumeric())
}

fn issue_of(cue: &str) -> String {
    if looks_like_issue(cue) {
        return cue
            .trim()
            .trim_end_matches([',', '.', ';', ':'])
            .to_string();
    }
    cue.split_whitespace()
        .find(|w| looks_like_issue(w))
        .map(|s| s.trim().trim_end_matches([',', '.', ';', ':']).to_string())
        .unwrap_or_default()
}

fn load_pack_panes() -> Result<(Vec<DueRow>, GraphLayout, bool, String)> {
    let client = ljos_cli::pack()?;
    let atoms =
        ljos_cli::atoms_lean(&client, &client.workspace()).context("pack: GET /v1/atoms failed")?;
    let now = ljos_cli::now_utc();
    let due = ljos_cli::due_of(&atoms, &now)
        .iter()
        .map(|a| due_row(a, &now))
        .collect();
    let summary = ljos_cli::review_summary(&atoms, &now);
    let graph = GraphLayout::from_pack(
        &ljos_cli::personas_of(&atoms),
        &ljos_cli::trust_rows(&atoms),
    );
    Ok((due, graph, true, summary))
}

fn due_row(atom: &Value, now: &str) -> DueRow {
    let due_at = atom
        .get("due_at")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    DueRow {
        id: atom
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .chars()
            .take(12)
            .collect(),
        kind: atom
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("claim")
            .to_string(),
        text: atom
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(96)
            .collect(),
        clock: clock_label(&due_at, now).to_string(),
        grade: grade_label(atom).to_string(),
        due_at,
    }
}

fn clock_label(due_at: &str, now: &str) -> &'static str {
    if due_at.is_empty() {
        "ungraded"
    } else if due_at < now {
        "overdue"
    } else {
        "later"
    }
}

fn grade_label(atom: &Value) -> &'static str {
    let review = atom.get("review");
    let reps = review
        .and_then(|r| r.get("reps"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let recalls = review
        .and_then(|r| r.get("recalls"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if reps > 0 {
        "recalled"
    } else if recalls > 0 {
        "lapsed"
    } else {
        "ungraded"
    }
}

fn load_claims() -> Result<Vec<ClaimRow>> {
    let dir = claimdag::resolve_dir(None);
    let graph = WorkGraph::open_dir(&dir)?;
    let live: Vec<&claimdag::WorkNode> = graph
        .list_view(false, false)
        .into_iter()
        .filter(|n| {
            matches!(
                n.status,
                WorkStatus::Claimed | WorkStatus::Running | WorkStatus::Ready
            )
        })
        .collect();
    Ok(live
        .iter()
        .map(|n| ClaimRow {
            id: n.id.to_hex().chars().take(12).collect(),
            status: n.status.as_str().to_string(),
            summary: n.summary.chars().take(96).collect(),
            assignee: assignee_hex(n.assignee),
            cas_gen: n.cas_gen,
            occupancy: occupancy_of(&live, n.assignee),
            updated_unix: n.updated_unix,
        })
        .collect())
}

fn assignee_hex(id: WorkId) -> String {
    if id.is_zero() {
        "-".into()
    } else {
        id.to_hex().chars().take(12).collect()
    }
}

fn occupancy_of(nodes: &[&claimdag::WorkNode], assignee: WorkId) -> u32 {
    if assignee.is_zero() {
        return 0;
    }
    nodes
        .iter()
        .filter(|n| {
            matches!(n.status, WorkStatus::Claimed | WorkStatus::Running) && n.assignee == assignee
        })
        .count() as u32
}

fn load_hits(cue: &str) -> Result<Vec<HitRow>> {
    Ok(ljos_cli::packset_search(cue)?
        .into_iter()
        .map(|h| HitRow {
            id: h
                .id
                .unwrap_or_else(|| "?".into())
                .chars()
                .take(12)
                .collect(),
            kind: h.kind,
            text: h.text.chars().take(96).collect(),
            score: format!("{:.3}", h.score),
        })
        .collect())
}

fn load_island(cue: &str) -> Result<(Vec<IslandRow>, Vec<String>)> {
    let body = ljos_cli::packset_island(cue, false).context("island: activate failed")?;
    let notices = island_notices_of(&body);
    let rows = body
        .get("island")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(island_row)
        .collect();
    Ok((rows, notices))
}

/// Weak/dense banners, wording from [`ljos_cli::format_island`].
fn island_notices_of(body: &Value) -> Vec<String> {
    let formatted = ljos_cli::format_island(body);
    let mut notices: Vec<String> = formatted
        .lines()
        .filter(|line| line.starts_with("weak island:"))
        .map(str::to_string)
        .collect();
    if !body["dense"].as_bool().unwrap_or(true)
        && !notices.iter().any(|n| n.contains("encoder is down"))
    {
        notices.push("the encoder is down, ranking is lexical only".into());
    }
    notices
}

fn island_row(atom: &Value) -> IslandRow {
    IslandRow {
        id: atom
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .chars()
            .take(12)
            .collect(),
        kind: atom
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("claim")
            .to_string(),
        text: atom
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(96)
            .collect(),
        activation: format!(
            "{:.3}",
            atom.get("activation")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        ),
        seed: atom.get("seed").and_then(Value::as_bool).unwrap_or(false),
    }
}

/// One deed-rail line from a library [`ljos_cli::Event`].
#[must_use]
pub fn event_line(e: &ljos_cli::Event) -> String {
    format!("{}  {}  {}", e.source, e.clock, e.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_row_truncates_text() {
        let atom =
            serde_json::json!({"id":"abcdefghijklmnop","kind":"lesson","text":"x".repeat(200)});
        let row = due_row(&atom, "2026-09-20T10:00:00.000Z");
        assert_eq!(row.id.len(), 12);
        assert_eq!(row.kind, "lesson");
        assert_eq!(row.text.len(), 96);
        assert!(row.due_at.is_empty());
        assert_eq!(row.clock, "ungraded");
        assert_eq!(row.grade, "ungraded");
    }

    #[test]
    fn due_row_keeps_due_at_and_display_grade() {
        let atom = serde_json::json!({
            "id":"abcdefghijklmnop",
            "kind":"lesson",
            "text":"x".repeat(200),
            "due_at":"2026-09-20T10:00:00.000Z",
            "review":{"reps":2,"recalls":2}
        });
        let row = due_row(&atom, "2026-09-20T12:00:00.000Z");
        assert_eq!(row.id.len(), 12);
        assert_eq!(row.kind, "lesson");
        assert_eq!(row.text.len(), 96);
        assert_eq!(row.due_at, "2026-09-20T10:00:00.000Z");
        assert_eq!(row.clock, "overdue");
        assert_eq!(row.grade, "recalled");
        let later = due_row(&atom, "2026-09-20T09:00:00.000Z");
        assert_eq!(later.clock, "later");
        let exact = due_row(&atom, "2026-09-20T10:00:00.000Z");
        assert_eq!(exact.clock, "later");
        let lapsed = due_row(
            &serde_json::json!({"id":"a","review":{"reps":0,"recalls":1},"due_at":"2026-01-01T00:00:00.000Z"}),
            "2026-09-20T00:00:00.000Z",
        );
        assert_eq!(lapsed.grade, "lapsed");
    }

    #[test]
    fn clock_label_names_overdue() {
        let now = "2026-09-20T12:00:00.000Z";
        assert_eq!(clock_label("", now), "ungraded");
        assert_eq!(clock_label(now, now), "later");
        assert_eq!(clock_label("2026-09-20T11:59:59.000Z", now), "overdue");
        assert_eq!(clock_label("2026-09-20T12:00:00.001Z", now), "later");
        assert_ne!(
            clock_label("2026-09-19T00:00:00.000Z", now),
            "due",
            "due_at < now is overdue, not due"
        );
        let src = include_str!("data.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(
            !prod.contains("due_at == now"),
            "exact RFC3339 equality is not a due chip"
        );
    }

    #[test]
    fn load_with_cue_none_does_not_steal_due_text() {
        let src = include_str!("data.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(
            !prod.contains("due.first()"),
            "boot cue must not steal first due text"
        );
        assert!(
            !prod.contains("claims.first()"),
            "boot cue must not steal first claim summary"
        );
        assert!(
            !prod.contains("c.summary"),
            "issue_of must not steal a claim summary as the issue id"
        );
        assert!(prod.contains("fn issue_of(cue: &str)"));
        assert!(prod.contains("search: {err}"));
        assert!(prod.contains("format_island"));
        assert!(
            !prod.contains(".unwrap_or_default()\n        .into_iter()"),
            "packset_search Err must not become silent empty hits"
        );
    }

    #[test]
    fn island_notices_come_from_format_island() {
        let weak = serde_json::json!({
            "weak": true,
            "dense": true,
            "agreed_seeds": 1,
            "island": []
        });
        let formatted = ljos_cli::format_island(&weak);
        let weak_line = formatted
            .lines()
            .find(|line| line.starts_with("weak island:"))
            .expect("format_island names a weak island");
        let notices = island_notices_of(&weak);
        assert_eq!(notices, vec![weak_line.to_string()]);
        let dense_down = serde_json::json!({
            "weak": false,
            "dense": false,
            "island": []
        });
        let notices = island_notices_of(&dense_down);
        assert!(notices.iter().any(|n| n.contains("encoder is down")));
        let both = serde_json::json!({
            "weak": true,
            "dense": false,
            "agreed_seeds": 2,
            "island": []
        });
        let notices = island_notices_of(&both);
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("encoder is down"));
    }

    #[test]
    fn lease_remaining_uses_default_window() {
        assert_eq!(
            lease_remaining_secs(100, 100),
            claimdag::DEFAULT_LEASE_SECS as i64
        );
        assert_eq!(
            lease_remaining_secs(100, 100 + claimdag::DEFAULT_LEASE_SECS),
            0
        );
        assert!(lease_remaining_secs(100, 100 + claimdag::DEFAULT_LEASE_SECS + 1) < 0);
        assert_eq!(format_lease("ready", 100, 200), "-");
        assert_eq!(
            format_lease("claimed", 100, 100 + claimdag::DEFAULT_LEASE_SECS + 5),
            "expired"
        );
        assert_eq!(format_lease("running", 100, 160), "14m00s");
        assert_eq!(format_lease("claimed", 100, 130), "14m30s");
        assert_eq!(
            format_lease("claimed", 0, claimdag::DEFAULT_LEASE_SECS - 45),
            "45s"
        );
    }

    #[test]
    fn looks_like_issue_is_proj_xxxx() {
        assert!(looks_like_issue("ljos-w8kb"));
        assert!(looks_like_issue("demo-n7ly"));
        assert!(!looks_like_issue("not-an-issue"));
        assert!(!looks_like_issue("ljos"));
        assert_eq!(issue_of("ljos-w8kb"), "ljos-w8kb");
        assert_eq!(issue_of(""), "");
        assert_eq!(issue_of("work"), "");
        assert_eq!(issue_of("fix ljos-9ptd leftover"), "ljos-9ptd");
    }

    #[test]
    fn watchstamp_empty_pack_ts_is_boot_not_a_load() {
        let boot = WatchStamp {
            work_mtime: 1,
            pack_ts: String::new(),
        };
        let filled = WatchStamp {
            work_mtime: 1,
            pack_ts: "2026-09-20T12:00:00Z".into(),
        };
        assert!(
            !boot.needs_load(&filled),
            "missing pack_ts is a boot snapshot already done"
        );
        assert!(!filled.needs_load(&filled));
        assert!(boot.needs_load(&WatchStamp {
            work_mtime: 2,
            pack_ts: "2026-09-20T12:00:00Z".into(),
        }));
        assert!(filled.needs_load(&WatchStamp {
            work_mtime: 1,
            pack_ts: "2026-09-20T12:00:01Z".into(),
        }));
        assert!(WatchStamp::default().needs_load(&WatchStamp {
            work_mtime: 9,
            pack_ts: String::new(),
        }));
    }

    #[test]
    fn library_apis_not_cli_or_writer_paths() {
        let src = include_str!("data.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(prod.contains("due_of"));
        assert!(prod.contains("review_summary"));
        assert!(prod.contains("open_dir"));
        assert!(prod.contains("trust_rows"));
        assert!(prod.contains("personas_of"));
        assert!(prod.contains("packset_search"));
        assert!(prod.contains("packset_island"));
        assert!(prod.contains("packset_island(cue, false)"));
        assert!(prod.contains("format_island"));
        assert!(prod.contains("timeline_events"));
        assert!(!prod.contains("Command::new(\"vissue\")"));
        assert!(!prod.contains("due_report"));
        assert!(!prod.contains("sitting_due_report"));
        assert!(!prod.contains("WorkGraph::load_dir"));
        assert!(!prod.contains(".load_dir("));
        assert!(!prod.contains("personas_from_pack"));
        assert!(!prod.contains("trust_from_pack"));
        assert!(!prod.contains("packset_island(cue, true)"));
        assert!(
            !prod.contains("Ok(Vec::new())"),
            "open_dir Err must not become an empty column"
        );
    }

    #[test]
    fn banner_only_is_an_empty_canvas() {
        let snap = Snapshot::banner_only("pack: down".into());
        assert!(snap.graph.is_empty());
        assert!(snap.due.is_empty());
        assert!(snap.claims.is_empty());
        assert!(snap.island.is_empty());
        assert!(snap.events.is_empty());
        assert!(!snap.pack_ok);
        assert_eq!(snap.banner, "pack: down");
        assert!(snap.graph.paint_ops(0, true, 400.0, 400.0).is_empty());
    }

    #[test]
    fn boot_banner_only_is_not_pack_down() {
        let snap = Snapshot::banner_only(String::new());
        assert!(snap.pack_ok, "first paint must not fake pack-down");
        assert!(snap.graph.is_empty());
        assert!(snap.banner.is_empty());
    }

    #[test]
    fn event_line_keeps_store_and_text() {
        let e = ljos_cli::Event {
            days: 20_000,
            clock: "08:01".into(),
            source: "deed",
            text: "deed-file-x produced by seat".into(),
        };
        let line = event_line(&e);
        assert!(line.contains("deed"));
        assert!(line.contains("08:01"));
        assert!(line.contains("deed-file-x"));
    }
}
