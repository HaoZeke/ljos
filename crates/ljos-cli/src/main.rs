//! `ljos`: one seat over the habitats. Each habitat keeps its own crate.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ljos_cli::{
    age_of, brief, bump_plan, calibrate, cards, claim, complete, conflicts, consensus_steps_for,
    copy_playbook, doctor, due_report, finish, forecasts_from_json, format_bump_rows,
    format_consolidation, format_doctor, format_findings, format_hits, format_hubs, format_island,
    format_personas, format_playbooks, format_readings, format_remembered, format_seat,
    format_steps, format_write_ack, graded, habit, habits, handover, healthy, hook_call, hook_note,
    hook_output_ruled, identity_or_seat, island_entities, join, learn_anchors, learn_and_write,
    learn_reading, learn_shared, mark_seen, now_utc, on_path, onboard, pack, packset_consolidate,
    packset_forget, packset_hubs, packset_island_as, packset_search_as_of, packset_write_as, panel,
    panel_steps, parse_every, personas_from_pack, playbooks_from_pack, policy_with_memory,
    policyd_required, post_hook_stdout, predictions_of, prompt_hook_stdout, read_campaign, receive,
    release, remember_findings, resolve_assignee, rows_about, rules_from_pack, run, run_as,
    run_captured, session_end, sitting_gated, stop_hook_stdout, tcb_check, timeline, topic_words,
    tracker_show_json, trim_num, trust_from_pack, verdict_for, whoami, withdraw_prediction,
    write_persona, write_prediction, write_rule, write_trust, Persona, Reading, Rule, Trust,
    HARNESSES_EXAMPLE, LEARN_BETA, POLICY_TCB, PROTOCOL,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "ljos",
    version,
    about = "One seat over cards, packset, deedar, vissue, claimdag, and policyd"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Remember one lesson. It is stored as an episode until a recalled review promotes it.
    Remember {
        text: Vec<String>,
        /// Remember as this persona: the lesson comes back to it first in its next brief.
        #[arg(long = "as")]
        as_persona: Option<String>,
        /// Store this as an episode. It is kept, and it is not a refresher. This is the default.
        #[arg(long)]
        transient: bool,
        /// Store this as a standing rule now, without waiting for a review.
        #[arg(long)]
        standing: bool,
    },
    /// Prefer one way over another, as a standing preference. Stored as written.
    Prefer {
        text: Vec<String>,
        /// Prefer as this persona.
        #[arg(long = "as")]
        as_persona: Option<String>,
    },
    /// Retire one atom by id. Tombstones it; the pack keeps the record.
    Forget {
        id: String,
        /// The deed accession that withdrew the claim. Refused if it is not one.
        #[arg(long)]
        why: Option<String>,
    },
    /// What the seat knows about a topic, ranked. Empty means the pack holds nothing on it.
    Search {
        query: Vec<String>,
        /// Most hits to print.
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: u32,
        /// Rerank the top hits with the writer's cross-encoder; slower, sharper.
        #[arg(long)]
        rerank: bool,
        /// Ask the pack as it stood then (YYYY-MM-DD or RFC 3339): what the seat knew at that time, withdrawn memories included, later ones left out.
        #[arg(long, value_name = "TIME")]
        as_of: Option<String>,
    },
    /// Candidate contradictions from the geometry of the seat's memory: the lowest passes between single memories, read by the optional `landscape` habitat.
    Conflicts {
        /// Most pairs to print.
        #[arg(short = 'n', long, default_value_t = 12)]
        limit: usize,
    },
    /// Consolidate the seat's memory: a later claim that rewrites an earlier one closes it. Reports the pairs; --apply writes.
    Consolidate {
        /// Write the closures. Without it the pairs are reported and nothing changes.
        #[arg(long)]
        apply: bool,
    },
    /// Share the seat's memory across machines through the tracker repository: pull and take other machines' sealed logs, then write and push this machine's.
    Sync {
        /// Print this machine's age public key (made on first use) for a scope's recipients, and stop.
        #[arg(long)]
        key: bool,
        /// Only pull and take other machines' logs.
        #[arg(long, conflicts_with = "export")]
        import: bool,
        /// Only write and push this machine's log.
        #[arg(long)]
        export: bool,
    },
    /// What this seat's memory turns on: the claims most linked to, by a weighted PageRank over the pack's links.
    Hubs {
        /// Most hubs to print.
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
    },
    /// The memories a task activates: search hits as seeds, spread along the pack's links.
    Island {
        cue: Vec<String>,
        /// The strongest eight fired together: their links gain weight.
        #[arg(long)]
        fire: bool,
        /// Walk it as this persona: its own weights on the way in, and a fire writes its weights, not the seat's.
        #[arg(long = "as")]
        as_persona: Option<String>,
    },
    /// Whether a deed's bytes are intact and its sources are too.
    Evidence { accession: String },
    /// Whether a deed is still the tip, or a later take superseded it.
    Current { accession: String },
    /// Cite a deed on an issue, or list what it cites. Citing a deed names it; the bytes stay in deedar.
    Deed {
        issue: String,
        /// An accession to cite, from `deedar create`; repeat for several.
        #[arg(long)]
        add: Vec<String>,
    },
    /// Who holds what on the tracker: `vissue claims`, with its flags.
    Claims {
        /// Passed to `vissue claims` as given (`--by NAME`, `--json`, `-p PROJECT`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// The working set for an issue: plan, its inputs' deeds, its own citations.
    Recall { issue: String },
    /// Cast this identity's ballot (VISSUE_AGENT), or read the tally with no --for.
    Vote {
        issue: String,
        /// The option to vote for.
        #[arg(long = "for")]
        choice: Option<String>,
        /// Probability in (0, 1] that the choice is the outcome.
        #[arg(long)]
        confidence: Option<f64>,
        /// Deed accessions this ballot used, comma-separated, or `none`.
        /// Required when `--for` is set.
        #[arg(long)]
        used: Option<String>,
        /// The option you expect the others to pick, or a JSON object of
        /// option to share. Same command as the ballot, so the surprisingly
        /// popular reading has a forecast without a second verb.
        #[arg(long)]
        expect: Option<String>,
        /// Cast as this persona instead of the seat's identity.
        #[arg(long = "as")]
        as_persona: Option<String>,
        /// Ask Jev for the persona's ballot (needs `--as`, not `--for`):
        /// cast with its confidence and forecast when it is sure, handed to
        /// a subagent when it is not.
        #[arg(long, requires = "as_persona", conflicts_with = "choice")]
        jev: bool,
        /// Take back the ballot and its forecast, as the seat or `--as` a
        /// persona: a ballot cast on the wrong issue or with no basis stops
        /// counting, and the issue's logbook keeps what it was.
        #[arg(long, conflicts_with_all = ["choice", "jev", "confidence", "used", "expect"])]
        withdraw: bool,
    },
    /// The brief a subagent playing a persona starts from: view, domains, what the seat knows there, the work.
    Brief {
        /// The persona's name.
        name: String,
        /// The tracker id of the issue.
        issue: String,
    },
    /// An issue's timeline: the tracker's logbook, the deeds it cites, and the memories it activates, one dated list oldest first with age and gap.
    Timeline {
        /// The tracker id of the issue.
        issue: String,
        /// Most events to print, the latest kept.
        #[arg(short = 'n', long, default_value_t = 60)]
        limit: usize,
    },
    /// A panel without MCP: one brief per persona written to a directory, then the settle line.
    Panel {
        /// The tracker id of the issue.
        issue: String,
        /// Where the briefs go, one `<persona>.md` each.
        #[arg(long, default_value = "panel")]
        out: PathBuf,
        /// Ask Jev for every ballot first; only the personas it is unsure
        /// for get a brief.
        #[arg(long)]
        jev: bool,
    },
    /// A voter with a view: NAME holds its ballot by ANCHOR in [0, 1] (0 never moves).
    Persona {
        name: String,
        /// How far the persona moves off its ballot in a settle; 1 is a plain voter.
        #[arg(long, default_value_t = 0.5)]
        anchor: f64,
        /// How this persona reads the work, in a sentence or two.
        #[arg(long)]
        view: String,
        /// Domains it speaks to; a trust row scoped to one of them applies when the issue is about it.
        #[arg(long, value_delimiter = ',')]
        about: Vec<String>,
    },
    /// The personas the pack holds: name, anchor, domains and view, one per line.
    Personas,
    /// Bind a playbook to an issue and copy its recipe into the working set, before personas enter.
    Playbook {
        /// The tracker id of the issue.
        issue: String,
        /// The recipe name: sit, arena, land, company-panel, overnight.
        name: String,
    },
    /// The playbooks the pack holds: the closed set sit, arena, land, company-panel, overnight.
    Playbooks,
    /// Take a session node for an issue. One live claim per assignee.
    Claim {
        /// A tracker id, or a 32-hex claim-graph id.
        node: String,
        /// Your name; mapped to one actor id. Absent: this conversation's holder (`ljos seat`).
        #[arg(long)]
        assignee: Option<String>,
    },
    /// Hand a session node back unfinished: ready again, generation moved.
    Release {
        /// A tracker id, or a 32-hex claim-graph id.
        node: String,
        /// The name that holds it. Absent: this conversation's holder (`ljos seat`).
        #[arg(long)]
        assignee: Option<String>,
    },
    /// Finish a session node. Does not close the ticket.
    Complete {
        /// A tracker id, or a 32-hex claim-graph id.
        node: String,
        /// done (default), failed, or cancelled.
        #[arg(long)]
        status: Option<String>,
        /// Generation from the sitting's claim. Absent: the live one. A stale gen is refused.
        #[arg(long)]
        gen: Option<u64>,
        /// The name that holds it. Absent: this conversation's holder (`ljos seat`).
        #[arg(long)]
        assignee: Option<String>,
    },
    /// Frozen working core. Read-only. Never extract-on-write.
    Cards {
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
    /// Forecast how the others will vote on an issue; two or more forecasts let the settle name the surprisingly popular answer.
    Predict {
        issue: String,
        /// The option you expect to win, or a JSON object of option to share.
        #[arg(long)]
        expect: String,
        /// Forecast as this persona instead of the seat's identity.
        #[arg(long = "as")]
        as_persona: Option<String>,
    },
    /// Argv law kept in the pack: a glob over the command line with a verdict the hook and `policy` enforce.
    Rule {
        /// A glob over the whole command line: `rm -rf *`, `*--force*`, `git push*`.
        pattern: String,
        /// deny stops the action; ask hands it to the person.
        #[arg(long, default_value = "ask")]
        verdict: String,
        /// The reason a stopped reader sees.
        #[arg(long)]
        why: String,
    },
    /// Argv law: the line as it would run, then what the pack knows that bears on it.
    Policy { argv: Vec<String> },
    /// The memory hook a runner or a policy layer calls before an action: reads the
    /// hook JSON (or plain text) on stdin, answers with the memories the action activates.
    Hook {
        /// Most memories to inject per call; each is injected once per session.
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
    /// DeGroot/Seldon over the pack's trust rows, then the tracker verb.
    Consensus { id: String },
    /// One trust row: FROM weighs TO at WEIGHT in (0, 1]. Written to the pack.
    Trust {
        from: String,
        to: String,
        weight: f64,
        /// Deed accessions this row stands on.
        #[arg(long)]
        why: Vec<String>,
        /// Domains this row is scoped to; none means it applies everywhere.
        #[arg(long, value_delimiter = ',')]
        about: Vec<String>,
    },
    /// Which habitats answer. Exit 1 when a required one does not.
    Doctor,
    /// Read-only icedtea pane over due, claims, and trust. Execs sibling `ljos-hud`.
    Hud {
        /// Stay on the terminal. Default detaches.
        #[arg(long)]
        foreground: bool,
        /// Show or hide a running HUD.
        #[arg(long, group = "summon")]
        toggle: bool,
        /// Show a running HUD.
        #[arg(long, group = "summon")]
        show: bool,
        /// Hide a running HUD.
        #[arg(long, group = "summon")]
        hide: bool,
        /// Write a user-local .desktop launcher and Sway overlay include.
        #[arg(long)]
        install_desktop: bool,
    },
    /// Print the sitting protocol: which store answers what, and the order of verbs.
    Protocol,
    /// Who is sitting: the seat this runner votes under, the holder this conversation claims under, and where the names came from.
    Seat,
    /// Print the one MCP server entry any runner takes; with --harness, register it with a runner named in ~/.config/ljos/harnesses.toml and install the protocol as its skill.
    Onboard {
        /// A runner named in ~/.config/ljos/harnesses.toml; absent, print the entry to paste anywhere.
        #[arg(long, default_value = "json")]
        harness: String,
        /// Report what would be written and write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Print an example harnesses.toml and stop.
        #[arg(long)]
        example: bool,
    },
    /// Pack a slice of the seat: satchel, atoms, the deeds both cite; sealed and signed.
    Handover {
        /// Where to write the satchel.
        #[arg(long)]
        out: PathBuf,
        /// Projects to take whole.
        #[arg(long)]
        project: Vec<String>,
        /// Issues to take, with what they stand on.
        #[arg(long)]
        issue: Vec<String>,
        /// Copy the sealed satchel to another seat over ssh, as `user@host:path`; the receiver runs `ljos receive`.
        #[arg(long)]
        to: Option<String>,
    },
    /// Check a satchel that arrived; --import puts its atoms in this seat's pack.
    Receive {
        dir: PathBuf,
        /// A bridge file from the last handover by the same sender.
        #[arg(long)]
        since: Option<PathBuf>,
        #[arg(long)]
        import: bool,
    },
    /// Atoms whose review is due, then one line on the state of the clock.
    Due,
    /// Put an eb-stack bundle's modules on the tracker: one child issue per module under the parent, blockers along the dependency edges, the same ids on every run. `vissue ready` then lists what a seat can build now.
    BumpPlan {
        /// The bundle directory: `locks/default.lock.json` and `package.sbom.cdx.json` inside it.
        bundle: PathBuf,
        /// The tracker project the issues go in.
        #[arg(long)]
        project: String,
        /// The bump ticket every module issue is a child of.
        #[arg(long)]
        parent: String,
        /// The generation named in titles and ids; default the lock's toolchain (`foss/2026.1`).
        #[arg(long)]
        generation: Option<String>,
        /// Print the rows and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// The typed findings of an eb-stack campaign state file, one per line; `--remember` writes one lesson per finding a person or a seat resolved, and `--issue` cites the state file on the issue.
    Findings {
        /// The campaign state file (`campaign.json`).
        state: PathBuf,
        /// Write one lesson per resolved finding to the pack.
        #[arg(long)]
        remember: bool,
        /// With --remember, every finding, the ones a later attempt superseded too.
        #[arg(long)]
        all: bool,
        /// Cite the state file as a deed on this issue.
        #[arg(long)]
        issue: Option<String>,
    },
    /// A habit is a number the seat keeps measuring. `ljos habit NAME VALUE` takes a reading and closes the one before; `ljos habit NAME` shows one; `ljos habit` lists them all with the change since the last reading and when the next is due.
    Habit {
        /// The habit's name; absent, list every habit.
        name: Option<String>,
        /// The reading; absent, show the habit.
        value: Option<f64>,
        /// The unit the reading is in, for the reader.
        #[arg(long, default_value = "")]
        unit: String,
        /// How often a reading is taken: 7d, 24h, 2w, 30m. The next is due one cadence on.
        #[arg(long, default_value = "7d")]
        every: String,
        /// Where the reading came from: a job id, a run, a file.
        #[arg(long, default_value = "")]
        source: String,
    },
    /// Open a sitting on an issue in the protocol's order: doctor, cards, due, island, playbook, recall, timeline, claim.
    Sitting {
        /// The tracker id of the issue.
        issue: String,
        /// Occupancy name. Absent: this conversation's holder (`ljos seat`). Always scoped to the issue.
        #[arg(long)]
        assignee: Option<String>,
        /// Where the cards are read from.
        #[arg(long, default_value = ".")]
        cards: PathBuf,
        /// Sit even when the issue's blockers are open. Without it a blocked issue is refused before anything is claimed.
        #[arg(long)]
        anyway: bool,
        /// The recipe this sitting copies before recall. Absent, a closed-set token in the title else sit. Held until finish or release.
        #[arg(long)]
        playbook: Option<String>,
    },
    /// Close a sitting: remember the lesson, fire the island, complete the node, learn from the outcome.
    Finish {
        /// The tracker id of the issue.
        issue: String,
        /// done, failed, or cancelled.
        #[arg(long, default_value = "done")]
        status: String,
        /// The lesson this sitting taught, two sentences at most.
        #[arg(long)]
        lesson: Option<String>,
        /// The option that turned out right, when the ballots are in and the world has said.
        #[arg(long)]
        outcome: Option<String>,
        /// The factor a refuted voter shrinks by when an outcome is named.
        #[arg(long, default_value_t = LEARN_BETA)]
        beta: f64,
        /// Generation from the sitting's claim. Absent: the live one. A stale gen is refused.
        #[arg(long)]
        gen: Option<u64>,
        /// The name that holds it. Absent: this conversation's holder (`ljos seat`).
        #[arg(long)]
        assignee: Option<String>,
        /// Also close the tracker ticket. Only when the work is accepted, not when the sitting ends.
        #[arg(long)]
        close: bool,
    },
    /// Write trust rows from a project's voting history: Dawid-Skene accuracy per voter, no truth labels.
    Calibrate {
        /// The tracker project whose settled issues to read.
        #[arg(short, long)]
        project: String,
        /// Expectation-maximisation rounds.
        #[arg(long, default_value_t = 20)]
        rounds: usize,
    },
    /// Grade one review; recalled unless --lapsed.
    Graded {
        id: String,
        #[arg(long)]
        lapsed: bool,
    },
    /// Reweigh the voters on an issue by what turned out right.
    Learn {
        id: String,
        /// The option that turned out right.
        #[arg(long)]
        outcome: String,
        /// The factor a refuted voter shrinks by.
        #[arg(long, default_value_t = LEARN_BETA)]
        beta: f64,
        /// A fixed share of recovery toward one after the step, so a voter refuted long ago can come back; 0 is plain Hedge.
        #[arg(long, default_value_t = 0.0)]
        share: f64,
        /// `record` (the default): each voter's hits and misses so far as log-odds weights. `hedge`: refuted voters shrink by --beta, with --share recovery.
        #[arg(long, default_value = "record")]
        rule: String,
    },
}

fn main() -> Result<()> {
    ljos_cli::normalize_tracker_env();
    // A closed pipe ends the run quietly: `ljos learn | head` is not a panic.
    // SAFETY: resetting a signal disposition before any thread is spawned.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    match Cli::parse().cmd {
        Cmd::Remember {
            text,
            as_persona,
            transient,
            standing,
        } => {
            if transient && standing {
                bail!("remember: pass --transient or --standing, not both");
            }
            let horizon = if transient {
                Some(true)
            } else if standing {
                Some(false)
            } else {
                None
            };
            let body = packset_write_as("Remember", &join(&text), as_persona.as_deref(), horizon)?;
            println!("{}", format_write_ack(&body));
            if let Some(ids) = body["supersedes"].as_array().filter(|ids| !ids.is_empty()) {
                eprintln!(
                    "revises {} earlier memor{}, now closed: {}",
                    ids.len(),
                    if ids.len() == 1 { "y" } else { "ies" },
                    ids.iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        Cmd::Prefer { text, as_persona } => {
            let body =
                packset_write_as("Prefer", &join(&text), as_persona.as_deref(), Some(false))?;
            println!("{}", format_write_ack(&body));
        }
        Cmd::Forget { id, why } => {
            let body = packset_forget(&id, why.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        Cmd::Sync {
            key,
            import,
            export,
        } => {
            if key {
                println!("{}", ljos_cli::sync::public_key()?);
            } else {
                let both = !import && !export;
                print!(
                    "{}",
                    ljos_cli::sync::sync_repo(both || import, both || export)?
                );
            }
        }
        Cmd::Hubs { limit } => print!("{}", format_hubs(&packset_hubs(limit)?)),
        Cmd::Conflicts { limit } => print!("{}", conflicts(limit)?),
        Cmd::Consolidate { apply } => {
            print!("{}", format_consolidation(&packset_consolidate(apply)?))
        }
        Cmd::Island {
            cue,
            fire,
            as_persona,
        } => {
            print!(
                "{}",
                format_island(&packset_island_as(
                    &join(&cue),
                    fire,
                    as_persona.as_deref()
                )?)
            );
        }
        Cmd::Search {
            query,
            limit,
            rerank,
            as_of,
        } => {
            print!(
                "{}",
                format_hits(&packset_search_as_of(
                    &join(&query),
                    limit,
                    as_of.as_deref(),
                    rerank
                )?)
            );
        }
        Cmd::Evidence { accession } => run("deedar", &["evidence", &accession])?,
        Cmd::Current { accession } => run("deedar", &["current", &accession])?,
        Cmd::Deed { issue, add } => {
            if add.is_empty() {
                run("vissue", &["deed", &issue])?;
            } else {
                let mut argv = vec!["deed".to_string(), issue.clone()];
                for a in &add {
                    argv.push("--add".into());
                    argv.push(a.clone());
                }
                let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                run("vissue", &refs)?;
                print!("{}", ljos_cli::persist_tracker(&issue, "cited a deed"));
            }
        }
        Cmd::Claims { args } => {
            let mut argv = vec!["claims"];
            argv.extend(args.iter().map(String::as_str));
            run("vissue", &argv)?;
        }
        Cmd::Recall { issue } => run("vissue", &["recall", &issue])?,
        Cmd::Vote {
            issue,
            choice,
            confidence,
            used,
            expect,
            as_persona,
            jev,
            withdraw,
        } => match choice {
            None if withdraw => {
                let who = identity_or_seat(as_persona.as_deref()).unwrap_or_else(whoami_tracker);
                // A ballot already taken back still leaves its forecast to
                // take back, so a second withdraw finishes the first.
                match ljos_cli::run_captured_as(
                    "vissue",
                    &["vote", &issue, "--withdraw"],
                    Some(&who),
                ) {
                    Ok(said) => print!("{}", said.stdout),
                    Err(e) if format!("{e:#}").contains("holds no ballot") => {
                        println!("{issue}: {who} holds no ballot; withdrawing the forecast");
                    }
                    Err(e) => return Err(e),
                }
                let n = withdraw_prediction(&issue, &who)
                    .with_context(|| format!("ballot withdrawn; the forecast for {who} was not"))?;
                println!("{n} forecast(s) withdrawn for {who}");
                print!("{}", ljos_cli::persist_tracker(&issue, "ballot withdrawn"));
            }
            None if jev => {
                let name = as_persona.as_deref().unwrap_or_default();
                match ljos_cli::jev_vote(name, &issue)? {
                    ljos_cli::JevVote::Cast(b) => {
                        println!(
                            "{name}: Jev cast {} at confidence {:.2}",
                            b.choice, b.confidence
                        );
                        print!("{}", ljos_cli::persist_tracker(&issue, "ballot cast"));
                    }
                    ljos_cli::JevVote::Escalated(b) => {
                        println!(
                            "{name}: Jev leaned {} at confidence {:.2}, under the {:.2} cut; not cast. \
                             Start a subagent from `ljos brief {name} {issue}`",
                            b.choice, b.confidence, b.escalate_below
                        );
                        print!(
                            "{}",
                            ljos_cli::persist_tracker(&issue, "escalated a ballot")
                        );
                    }
                }
            }
            Some(c) => {
                let used = used.as_deref().unwrap_or("");
                if used.is_empty() {
                    bail!(
                        "a ballot records what it used (doi:10.1007/3-540-44503-X_20); pass --used deed-... or --used none"
                    );
                }
                let mut args = vec![
                    "vote".to_string(),
                    issue.clone(),
                    "--for".into(),
                    c,
                    "--used".into(),
                    used.to_string(),
                ];
                if let Some(p) = confidence {
                    args.push("--confidence".into());
                    args.push(format!("{p}"));
                }
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                run_as("vissue", &refs, as_persona.as_deref())?;
                if let Some(expect) = expect.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
                    let who =
                        identity_or_seat(as_persona.as_deref()).unwrap_or_else(whoami_tracker);
                    write_prediction(&issue, &who, expect).with_context(|| {
                        format!("ballot cast; forecast for {who} was not recorded")
                    })?;
                    println!("forecast recorded for {who}");
                }
                print!("{}", ljos_cli::persist_tracker(&issue, "ballot cast"));
            }
            None => run("vissue", &["vote", &issue])?,
        },
        Cmd::Brief { name, issue } => print!("{}", brief(&name, &issue)?),
        Cmd::Timeline { issue, limit } => print!("{}", timeline(&issue, limit)?),
        Cmd::Panel { issue, out, jev } => {
            if jev {
                print!("{}", ljos_cli::panel_jev(&issue, &out)?);
                print!("{}", ljos_cli::persist_tracker(&issue, "panel through Jev"));
            } else {
                print!("{}", panel(&issue, &out)?);
            }
        }
        Cmd::Persona {
            name,
            anchor,
            view,
            about,
        } => {
            let body = write_persona(&Persona {
                name,
                anchor,
                view,
                entities: about,
            })?;
            println!("{}", format_write_ack(&body));
        }
        Cmd::Personas => {
            print!("{}", format_personas(&personas_from_pack()?));
        }
        Cmd::Playbook { issue, name } => {
            print!("{}", copy_playbook(&issue, &name)?);
            print!("{}", ljos_cli::persist_tracker(&issue, "bound a playbook"));
        }
        Cmd::Playbooks => {
            print!("{}", format_playbooks(&playbooks_from_pack()?));
        }
        Cmd::Claim { node, assignee } => {
            print!("{}", claim(&node, &resolve_assignee(assignee.as_deref()))?)
        }
        Cmd::Release { node, assignee } => {
            print!(
                "{}",
                release(&node, &resolve_assignee(assignee.as_deref()))?
            );
            print!("{}", ljos_cli::persist_tracker(&node, "released"));
        }
        Cmd::Complete {
            node,
            status,
            gen,
            assignee,
        } => print!(
            "{}",
            complete(
                &node,
                status.as_deref(),
                &resolve_assignee(assignee.as_deref()),
                gen
            )?
        ),
        Cmd::Cards { dir } => print!("{}", cards(&dir)?),
        Cmd::Predict {
            issue,
            expect,
            as_persona,
        } => {
            let who = as_persona
                .or_else(|| std::env::var("VISSUE_AGENT").ok())
                .unwrap_or_else(whoami_tracker);
            let body = write_prediction(&issue, &who, &expect)?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        Cmd::Rule {
            pattern,
            verdict,
            why,
        } => {
            let body = write_rule(&Rule {
                pattern,
                verdict,
                reason: why,
            })?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        Cmd::Policy { argv } => {
            eprintln!("ljos: {POLICY_TCB}");
            print!("{}", policy_with_memory(&argv)?);
        }
        Cmd::Hook { limit } => {
            use std::io::Read;
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            let call = hook_call(&input);
            // A context event (a prompt, a tool result) says what the seat
            // knows, and saying nothing is a correct answer; a runner that
            // cuts the hook off throws the answer away and says it failed.
            // So those events answer inside a deadline, whatever the pack
            // does, and a call a second registration of the same hook makes
            // at the same moment is answered once. A tool gate is exempt
            // from both: its verdict must not be lost to a clock.
            if matches!(call.event.as_str(), "UserPromptSubmit" | "PostToolUse") {
                if ljos_cli::hook_already_running(&call) {
                    return Ok(());
                }
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(
                        ljos_cli::HOOK_DEADLINE_MS,
                    ));
                    // Taking the lock waits out an answer being written.
                    let _held = std::io::stdout().lock();
                    std::process::exit(0);
                });
            }
            // A hook answers within the runner's timeout: lookups that walk
            // the whole tracker are skipped from here on.
            // SAFETY: single-threaded here, before anything reads the environment.
            unsafe { std::env::set_var("LJOS_IN_HOOK", "1") };
            let (subagent, stop_active, agent) = ljos_cli::hook_subagent(&input);
            ljos_cli::hook_trace(&input, &call, subagent.as_deref());
            // A subagent about to stop is held once while its parent holds
            // an issue, so its result reaches the issue as a ballot or a
            // lesson instead of ending in the parent's context.
            if call.event == "SubagentStop" {
                let kind = subagent.as_deref().unwrap_or("subagent");
                let key = format!("subagent-gate:{agent}");
                let seen = ljos_cli::seen_ids(call.session.as_deref());
                if !seen.contains(&key) {
                    let issue = ljos_cli::held_issue();
                    let decision = issue.as_deref().is_some_and(|i| {
                        ljos_cli::tracker_show_json(i).is_ok_and(|v| ljos_cli::is_decision(&v))
                    });
                    if let Some(reason) = ljos_cli::subagent_stop_reason(
                        kind,
                        issue.as_deref(),
                        decision,
                        stop_active,
                    ) {
                        ljos_cli::mark_seen(call.session.as_deref(), &[key]);
                        println!(
                            "{}",
                            serde_json::json!({"decision": "block", "reason": reason})
                        );
                        return Ok(());
                    }
                }
                if let Some(reason) = ljos_cli::stop_audit(&input, stop_active) {
                    println!(
                        "{}",
                        serde_json::json!({"decision": "block", "reason": reason})
                    );
                }
                return Ok(());
            }
            // An agent about to end its turn is audited once: a done claim
            // beside a red test run, or asked work put off, holds it for
            // one more round.
            if call.event == "Stop" && call.shape != ljos_cli::HookShape::Context {
                if let Some(reason) = ljos_cli::stop_audit(&input, stop_active) {
                    println!(
                        "{}",
                        serde_json::json!({"decision": "block", "reason": reason})
                    );
                    return Ok(());
                }
            }
            // At the end of a session the memories it used fire together,
            // and there is nothing to say.
            if call.event == "SessionEnd" {
                session_end(call.session.as_deref());
                return Ok(());
            }
            // On a tool call the pack's rules give a verdict; on a prompt
            // there is nothing to stop, only something to know.
            let rules = if call.event == "PreToolUse" || call.event == "argv" {
                rules_from_pack().unwrap_or_default()
            } else {
                Vec::new()
            };
            let argv: Vec<String> = call.cue.split_whitespace().map(String::from).collect();
            // A tool call with no command line (a file read, a search) has
            // no argv for the law to judge; the TCB sees only shell lines.
            let tcb_rule =
                if (call.event == "PreToolUse" || call.event == "argv") && !argv.is_empty() {
                    match tcb_check(&argv) {
                        Some(t) if t.starts_with("deny") => Some(Rule {
                            pattern: "ljos-policyd".into(),
                            verdict: "deny".into(),
                            reason: t.split('\t').nth(1).unwrap_or("tcb").to_string(),
                        }),
                        None if policyd_required() => Some(Rule {
                            pattern: "ljos-policyd".into(),
                            verdict: "deny".into(),
                            reason: "TCB required".to_string(),
                        }),
                        _ => None,
                    }
                } else {
                    None
                };
            let verdict = tcb_rule.as_ref().or_else(|| verdict_for(&rules, &call.cue));
            // Search on the prompt. A camel-case runner discards that
            // stdout, so the note is held and emitted on the first tool
            // result. Stop additionalContext would start another round, so
            // Stop speaks only when no tool ran. PreToolUse / argv only decide.
            let context = match call.event.as_str() {
                "PreToolUse" | "argv" => {
                    // A seat verb about to run resets the work count.
                    let _ = ljos_cli::work_nudge(&call, subagent.is_some());
                    String::new()
                }
                "PostToolUse" => {
                    let (mut ctx, ids) = post_hook_stdout(call.shape, call.session.as_deref());
                    mark_seen(call.session.as_deref(), &ids);
                    if let Some(nudge) = ljos_cli::work_nudge(&call, subagent.is_some()) {
                        ctx = if ctx.is_empty() {
                            nudge
                        } else {
                            format!("{ctx}\n{nudge}")
                        };
                    }
                    if let Some(kind) = subagent.as_deref() {
                        let key = format!("subagent-brief:{agent}");
                        if !ljos_cli::seen_ids(call.session.as_deref()).contains(&key) {
                            if let Some(issue) = ljos_cli::held_issue() {
                                let decision = ljos_cli::tracker_show_json(&issue)
                                    .is_ok_and(|v| ljos_cli::is_decision(&v));
                                ljos_cli::mark_seen(call.session.as_deref(), &[key]);
                                let brief = ljos_cli::subagent_brief(kind, &issue, decision);
                                ctx = if ctx.is_empty() {
                                    brief
                                } else {
                                    format!("{brief}\n{ctx}")
                                };
                            }
                        }
                    }
                    ctx
                }
                // A turn with no tool never fired PostToolUse. Stop is the
                // only channel left, and its feedback does start another
                // round. A turn that already delivered on PostToolUse
                // finds an empty hold and ends.
                "Stop" => {
                    let (ctx, ids) = stop_hook_stdout(call.session.as_deref(), stop_active);
                    mark_seen(call.session.as_deref(), &ids);
                    ctx
                }
                // A turn ending is not a session ending, and has nothing
                // to say either.
                "SessionStart" | "TurnEnd" => String::new(),
                _ => {
                    let (ctx, ids) = hook_note(&call, limit);
                    if call.shape != ljos_cli::HookShape::CamelCase {
                        mark_seen(call.session.as_deref(), &ids);
                    }
                    prompt_hook_stdout(call.shape, call.session.as_deref(), &ctx, &ids)
                }
            };
            print!("{}", hook_output_ruled(&call, &context, verdict));
        }
        Cmd::Consensus { id } => {
            // Rows scoped to a domain apply when the issue is about it; the
            // personas' anchors go to both settles; the issue's tags pick
            // the model.
            let (topic, tags) = issue_topic_and_tags(&id);
            let trust = rows_about(&pack_trust_or_none(), &topic);
            let personas = personas_from_pack().unwrap_or_default();
            for step in consensus_steps_for(
                &id,
                on_path("ljos-consensus"),
                on_path("vissue"),
                &trust,
                &personas,
                &tags,
            )? {
                run(step.bin, &step.args)?;
            }
            // Beside the settle: the surprisingly popular answer when two
            // or more voters forecast, and the voters' standing when rows
            // exist.
            let predictions = pack()
                .and_then(|c| {
                    ljos_cli::atoms_lean(&c, &c.workspace())
                        .context("consensus: GET /v1/atoms failed")
                })
                .map(|atoms| predictions_of(&atoms, &id))
                .unwrap_or_default();
            for step in panel_steps(&id, on_path("ljos-consensus"), &trust, &predictions) {
                run(step.bin, &step.args)?;
            }
        }
        Cmd::Trust {
            from,
            to,
            weight,
            why,
            about,
        } => {
            let row = Trust {
                from,
                to,
                weight,
                about,
            };
            let v = write_trust(&row, &why)?;
            println!("{v}");
        }
        Cmd::Doctor => {
            let rows = doctor();
            print!("{}", format_doctor(&rows));
            if !healthy(&rows) {
                std::process::exit(1);
            }
        }
        Cmd::Hud {
            foreground,
            toggle,
            show,
            hide,
            install_desktop,
        } => {
            std::process::exit(ljos_cli::hud::run(ljos_cli::hud::HudLaunch {
                foreground,
                toggle,
                show,
                hide,
                install_desktop,
            }));
        }
        Cmd::Protocol => print!("{PROTOCOL}"),
        Cmd::Seat => print!("{}", format_seat(&whoami())),
        Cmd::Onboard {
            harness,
            dry_run,
            example,
        } => {
            if example {
                print!("{HARNESSES_EXAMPLE}");
                return Ok(());
            }
            let steps = onboard(&harness, dry_run)?;
            print!("{}", format_steps(&steps));
            if steps.iter().any(|s| !s.ok) {
                std::process::exit(1);
            }
        }
        Cmd::Handover {
            out,
            project,
            issue,
            to,
        } => {
            for line in handover(&out, &project, &issue)? {
                println!("{line}");
            }
            if let Some(dest) = to {
                // The bag is sealed and signed before it moves; the copy is
                // the runner's scp, so the receiving seat's keys and hosts
                // apply as they would by hand.
                run_captured("scp", &["-rq", &out.display().to_string(), &dest])?;
                println!(
                    "copied to {dest}; there, `ljos receive {}`",
                    out.file_name()
                        .map_or_else(|| "DIR".into(), |n| n.to_string_lossy().into_owned())
                );
            }
        }
        Cmd::Receive { dir, since, import } => {
            for line in receive(&dir, since.as_deref(), import)? {
                println!("{line}");
            }
        }
        Cmd::Due => print!("{}", due_report()?),
        Cmd::BumpPlan {
            bundle,
            project,
            parent,
            generation,
            dry_run,
        } => {
            let (generation, rows) =
                bump_plan(&bundle, &project, &parent, generation.as_deref(), dry_run)?;
            print!("{}", format_bump_rows(&generation, &rows));
        }
        Cmd::Findings {
            state,
            remember,
            all,
            issue,
        } => {
            if remember || issue.is_some() {
                print!(
                    "{}",
                    format_remembered(&remember_findings(&state, issue.as_deref(), all)?)
                );
            } else {
                print!("{}", format_findings(&read_campaign(&state)?));
            }
        }
        Cmd::Habit {
            name,
            value,
            unit,
            every,
            source,
        } => match (name, value) {
            (Some(name), Some(value)) => {
                let every_s = parse_every(&every)?;
                let (body, prev) = habit(&name, value, &unit, every_s, &source)?;
                println!("{}", format_write_ack(&body));
                if let Some(p) = prev {
                    eprintln!(
                        "was {}{}{} ({}), now closed",
                        trim_num(p.value),
                        if p.unit.is_empty() { "" } else { " " },
                        p.unit,
                        age_of(p.ts.as_deref(), &now_utc())
                    );
                }
            }
            (None, Some(_)) => {
                anyhow::bail!("habit: a reading needs a name; `ljos habit NAME VALUE`")
            }
            (name, None) => {
                let now = now_utc();
                let rows: Vec<Reading> = habits()?
                    .into_iter()
                    .filter(|r| name.as_deref().is_none_or(|n| r.name == n.trim()))
                    .collect();
                if rows.is_empty() {
                    println!(
                        "no readings{}; `ljos habit NAME VALUE` takes the first",
                        name.map(|n| format!(" of {n}")).unwrap_or_default()
                    );
                } else {
                    print!("{}", format_readings(&rows, &now));
                }
            }
        },
        Cmd::Sitting {
            issue,
            assignee,
            cards: cards_dir,
            anyway,
            playbook,
        } => print!(
            "{}",
            sitting_gated(
                &issue,
                &resolve_assignee(assignee.as_deref()),
                &cards_dir,
                anyway,
                playbook.as_deref()
            )?
        ),
        Cmd::Finish {
            issue,
            status,
            lesson,
            outcome,
            beta,
            gen,
            assignee,
            close,
        } => print!(
            "{}",
            finish(
                &issue,
                &status,
                lesson.as_deref(),
                outcome.as_deref(),
                beta,
                &resolve_assignee(assignee.as_deref()),
                gen,
                close
            )?
        ),
        Cmd::Calibrate { project, rounds } => {
            let rows = calibrate(&project, rounds)?;
            for row in &rows {
                println!("{} weighs {} at {:.3}", row.from, row.to, row.weight);
            }
        }

        Cmd::Graded { id, lapsed } => {
            let v = graded(&id, !lapsed)?;
            println!("{}", v["due_at"].as_str().unwrap_or("graded"));
        }
        Cmd::Learn {
            id,
            outcome,
            beta,
            share,
            rule,
        } => {
            let said = run_captured("vissue", &["vote", &id, "--json"])?;
            let forecasts = forecasts_from_json(&said.stdout)?;
            let ballots: Vec<(String, String)> = forecasts
                .iter()
                .map(|f| (f.agent.clone(), f.choice.clone()))
                .collect();
            // The rows written are scoped to what the issue's island is
            // about, so a voter wrong here keeps its standing elsewhere.
            let about = island_entities(&id).unwrap_or_default();
            let (rows, moved, calibration) = if rule == "hedge" {
                let rows =
                    learn_shared(&ballots, &outcome, &trust_from_pack()?, beta, &about, share)?;
                let moved = learn_anchors(&personas_from_pack()?, &ballots, &outcome, beta);
                for row in &rows {
                    write_trust(row, &[])?;
                }
                for p in &moved {
                    write_persona(p)?;
                }
                (rows, moved, std::collections::BTreeMap::new())
            } else {
                learn_and_write(&ballots, &outcome, beta, &about, &forecasts)?
            };
            println!(
                "{}",
                learn_reading(rows.len(), moved.len(), &forecasts, &outcome, &calibration)
            );
            for row in &rows {
                println!("{} weighs {} at {:.3}", row.from, row.to, row.weight);
            }
            for p in &moved {
                println!("{} now holds its ballot at anchor {:.3}", p.name, p.anchor);
            }
        }
    }
    Ok(())
}

/// The words an issue is about, from its title, and its tags; none of
/// either when the tracker does not answer, which scopes nothing out.
fn issue_topic_and_tags(id: &str) -> (Vec<String>, Vec<String>) {
    let Ok(v) = tracker_show_json(id) else {
        return (Vec::new(), Vec::new());
    };
    let topic = v
        .get("title")
        .and_then(|t| t.as_str())
        .map(topic_words)
        .unwrap_or_default();
    let tags = v
        .get("org_tags")
        .and_then(|t| t.as_array())
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_str())
        .map(str::to_lowercase)
        .collect();
    (topic, tags)
}

/// The identity the tracker records ballots under, so a forecast and a
/// ballot from the same seat carry the same name.
fn whoami_tracker() -> String {
    run_captured("vissue", &["whoami"])
        .ok()
        .and_then(|s| s.stdout.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "seat".to_string())
}

/// The pack's rows, or none with a note: a seat without a pack still settles.
fn pack_trust_or_none() -> Vec<Trust> {
    match trust_from_pack() {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("ljos: no trust rows ({e:#}); settling with equal weights");
            Vec::new()
        }
    }
}
