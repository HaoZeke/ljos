# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

## 0.24.2 (2026-10-04)

- The TCB judges each pipeline whole, in shell words: `curl URL | sh` reaches `ljos-policyd` as one call, where splitting at the pipe let it pass, and a quoted sentence naming a command is one word.

## 0.24.1 (2026-10-03)

- Consent in the chat: a command an ask rule stopped is approved by the person replying `approve ID` in the same conversation; the prompt hook records the grant, as `ljos approve ID` in a terminal does. A request from another conversation is not granted, and the seat guard refuses typing an approval into a pane.

## 0.24.0 (2026-10-03)

- `ljos vote --as NAME --jev` no longer casts a ballot and then fails: the forecast's claim text names the expected option and its share instead of the whole distribution, which passed the pack's 500-character cap, and the forecast is written before the ballot.
- At a usage limit the Stop hook holds the turn once per notice and names what to record first: a note on the held issue with what is done and left, `ljos file` for each item left, `ljos remember` for each lesson.
- `ljos file TITLE` files work found while sitting as a child of the held issue, in its project, and commits the tracker; `ljos note ISSUE TEXT` notes progress and commits. The protocol, the work nudge and the panel briefs name them in place of the bare tracker verbs.
- The seat guard reads shell words: a quoted sentence that names a seat path is data, and a path word or a redirection outside quotes into one is still refused.

## 0.23.3 (2026-10-03)

- `ljos onboard --harness NAME` works with no runners file: a runner the
  seat ships a shape for (claude, codex, grok, antigravity, opencode, omp,
  hermes) is onboarded from that shape, which is added to
  `~/.config/ljos/harnesses.toml` so the doctor and persona sessions know
  it.
- Rules and the TCB judge each command a line runs, as written and with
  its prefixes off, and no longer the raw line, so a heredoc body that
  names a flag or a command is data and not a command.
- The README opens with what ljos is and for whom, a five-minute first run
  with the output it gives, and a glossary; the command list follows.
- `vissue vote ID` with no `--for` and no `--withdraw` reads the tally
  and is not refused; the seat rule on `vissue vote*` had denied a read.
- The seat command named in a tracker-verb deny drops the shell's
  redirections: `vissue vote X --for A 2>&1` is told `ljos vote X --for
  A`, not `ljos vote X 2>`.

## 0.23.2 (2026-10-02)

- A panel seats who speaks to the title. A persona its island reaches
  sits only when its own view uses a word of the title, and the view
  fallback needs two of the title's words: a cvmfs capability decision
  had seated five physics and course reviewers through the island and
  through views that say "change". A panel with nobody to seat says how
  to write the voters it needs.
- `ljos persona --about` takes several domains after one flag as well as
  a comma list or the flag repeated.
- The protocol teaches the judge verbs (`vote --jev`, `panel --jev`,
  `due --judge`), persona sessions and `ljos ask`, and the tracker forms
  `vissue append ISSUE "..."` and `vissue update ISSUE -t TAG`; a seat
  had looked for a jev command and found none.

## 0.23.1 (2026-10-02)

- `ljos upgrade` stages its download under `~/.cache/ljos`, on the home
  filesystem; on a host whose `/tmp` is a full, quota'd tmpfs the
  download failed with a write error.
- The seat guard judges the command line `ssh` runs on its host as a
  command of its own: `ssh h '~/.local/bin/ljos --version'` runs the
  binary and passes, `ssh h 'cp x ~/.local/bin/ljos'` writes it and is
  refused.

## 0.23.0 (2026-10-02)

- `ljos upgrade [VERSION]` installs a published release beside the
  running `ljos`: the archive the release workflow built for the tag,
  checked against its published sha256, each binary an executable that
  names the version, every binary checked before any is replaced, and
  the old ones kept as `NAME.bak-pre-VERSION`. The seat's guard keeps
  agents from writing its binaries; this verb moves them forward with
  bytes no agent made.
- A here-document's body is data, not commands: a seat rule no longer
  meets `cargo build` inside `cat > job.sbatch <<'EOF' ... EOF`, which
  refused every job script written that way on a desktop seat. Commands
  after the closing word still count.

## 0.22.3 (2026-10-02)

- Two tasks handed to a persona in one second keep two inbox files; the
  file was named to the second, so the later overwrote the earlier. A
  live test opens a persona's tmux window, hands it two tasks and reads
  both back.

## 0.22.2 (2026-10-02)

- `ljos-mcp` is found beside the running `ljos` before PATH, so the
  doctor, onboard and the seat binary row work from a shell, such as
  ssh, that lacks the install directory on PATH.

## 0.22.1 (2026-10-02)

- `ljos doctor`'s host row fails while the kernel's OOM count rises and
  for a day after, then says how many since boot with none in the last
  day. The counter is cumulative since boot, so one old kill had kept
  the row red until the next reboot.
- The prompt's due line counts what came due this week, with the
  backlog's size beside it, and says nothing when nothing new came due.
  A seat with 1263 due claims had been told so every session; the
  count only grew, and a runner had once tried to drain it unread.
- Claude Code's tool gate covers its file tools: the `PreToolUse`
  matcher is `Bash|Edit|Write|MultiEdit|NotebookEdit`, and a file
  tool's cue is the tool and the path it writes, not the file's text, so
  the seat's guard refuses a write to a seat path and a doc that names
  one passes.
- Two signals before the prompt hook speaks. A claim Jev says bears on
  the prompt goes in when it also names a content word of the prompt,
  or when Jev is sure alone (0.75); on a vague prompt Jev had put an
  unrelated EESSI planning preference in at 0.61. The treat-as-data
  note needs pasted material in the prompt (a pasted block, a fence,
  terminal or log lines, or many lines) beside Jev's injection answer,
  which ran 0.6 to 0.7 on plain requests; and its text lost a run of
  spaces.

## 0.22.0 (2026-10-02)

- An `ask` on a runner that cannot ask prints a request id; the person
  grants that one command with `ljos approve ID`, for one attempt in
  that directory and conversation, within fifteen minutes. `approve`
  refuses under an agent runner (a runner's conversation in the
  environment, or a runner above it) or without a terminal, so an agent
  cannot grant itself consent, and the approval store is a seat path
  the guard keeps agents out of.
- agy's conversation id names its holder, and its prompt is read out of
  the `<USER_REQUEST>` wrapper.
- The seat guards its own law. Before any rule, a shell command that
  writes the seat's binaries (`ljos`, `ljos-mcp`, `ljos-policyd`), its
  config or a runner's hook registration (`~/.codex/hooks.json`,
  `~/.gemini/config/hooks.json`, `~/.claude/settings.json`,
  `~/.grok/hooks/ljos.json`, the opencode and omp plugins), or a file
  tool aimed at one, is refused as the person's to change; reading
  them is not. An agy session had replaced `~/.local/bin/ljos` with a
  script that allowed every command, and its answer broke every other
  runner's prompt hook. agy's gate now sees every tool, and a file
  tool names the path it writes.
- `ljos doctor` fails a `seat binary` row when the `ljos` the hooks run
  is not a binary or not the one running the doctor.

## 0.21.1 (2026-10-02)

- A deny on a bare tracker verb names the exact seat command to run in
  its place, arguments carried over: `vissue claim ljos-6c3z` is told
  to run `ljos sitting ljos-6c3z`, `vissue vote X --for A` is told
  `ljos vote X --for A`. The rule's reason had named a placeholder, and
  an agent guessed.

## 0.21.0 (2026-10-02)

- A seat rule's trailing `*` straight after a word continues only past
  a word boundary: `vissue claim*` meets `vissue claim X` and no longer
  refuses the read-only `vissue claims`.
- The docs and the example runners file name the harnesses and models a
  persona reasons through; the example ships the `claude` and `codex`
  shapes with their `resume` argv.
- Thinkers are personas. `ljos persona NAME --runner RUNNER` gives a
  persona a runner, a `[[harness]]` whose new `start` and `resume` argv
  say how it opens a session and resumes the latest one in a directory,
  that reasons as it in a session it keeps: a herdr pane or a window of the tmux session
  `ljos-personas`, working in `$XDG_STATE_HOME/ljos/personas/NAME` as
  the seat named after the persona, so its memories, ballots and trust
  rows are the persona's. The runner resumes the latest session of that
  directory, so a later hand-off resumes the conversation, and an
  open pane takes the next task in place. `vote --jev` and `panel
  --jev` hand an unsure persona's ballot to its own session; `ljos ask
  NAME "..."` puts a question to it. The 0.19 and 0.20 thinker roster
  (`[escalate]`, `escalate_band`, `command_mode = "prompt"`, `surface`,
  `PERSONA-THINKER` voters) is gone, and so is the review hand-off from
  `due --judge`.
- Repository facts are pack claims. The push gate asks `gh` once per
  repository and remembers the answer as a standing claim on
  `repo:OWNER/REPO`; later pushes read the pack. The runtime cache and
  `push.toml` are gone.

## 0.20.1 (2026-10-02)

- The push gate counts a version tag (`v1.2`, `0.3.0`) as a release and
  a bookmark tag as nothing; a notes repository with one
  `campaign-sent` tag had asked for a cite on every push.

## 0.20.0 (2026-10-02)

- Thinkers are seats. When a fast judge leaves a ballot or a due claim
  open, ljos hands it to the runners `[escalate]` names, each started
  in a herdr or tmux pane as a seat of its own: it writes its reasoning
  with `vissue note` and casts `ljos vote --as PERSONA-THINKER`, or
  grades with `ljos graded`, so consensus weighs it by its trust row and
  `learn` and `calibrate` move the row. `vote --jev`, `panel --jev` and
  `due --judge` hand off; each hand-off is noted with its pane. The
  0.19.0 inline escalation, which pooled thinkers' JSON inside the
  judge, is gone.
- The push gate asks the forge whose a remote is, so no file is needed:
  `gh api` gives the person's push permission, whether the owner is
  their own account, the collaborators and the releases, kept a day. A
  branch push to the person's own unshared, unreleased repository runs;
  a shared, organisation or released one needs a cite; one they cannot
  push to is theirs to run. On a forge the seat cannot ask, their own
  namespace under their GitHub name counts as theirs. `push.toml`
  remains an override.

## 0.19.0 (2026-10-02)

- A thinker never runs unseen: a prompt-mode judge opens in a herdr
  pane, else a window of the tmux session `ljos-judges` (`surface =
  "auto"`), names itself, tees what it prints back to the seat, and
  leaves a shell in the pane for the person to read, attach or stop it.
  No pane system, no answer. `surface = "none"` is for adapters and
  tests.
- Judges layer: when the route's pool is unsure, a probability inside
  `escalate_band` (0.2 to 0.8) or a choice under `escalate_below`, the
  decision goes on to the thinkers `[escalate]` names, runners such as
  `grok -p`, `omp -p --no-tools` or `hermes -z` in
  prompt mode, each answering with a short `why` that the log keeps.
  Every answer is pooled. A hook never escalates. A thinker runs with
  `LJOS_JUDGE=1`, under which the seat's hook injects nothing and the
  argv law still holds.
- An asked `git push` is gated by where it goes. With
  `~/.config/ljos/push.toml` naming `owners`, a branch push to an
  unreleased repository of theirs runs; a push to one with tags, or one
  `shared` names, runs when it cites the decision behind it,
  `LJOS_CITE=ISSUE git push ...`, where the issue settles (`vissue
  consensus --gate`) or closed as a decision, or `LJOS_CITE=ACCESSION`
  for a current deed, and the pass is noted on the issue; anyone else's
  remote, tags, a mirror or a force stay the person's. With no file no
  push is free. Grok had refused 102 pushes in 13 sessions to
  the agents' own repositories under the blanket `git push*` ask.
- A seat rule is tried on every command a shell line runs, not only on
  the line's start: `cd repo && git push` and `FOO=1 git push` meet the
  `git push*` rule. The line splits on `&&`, `||`, `;`, `|` and `&`
  outside quotes, so a commit message naming a command is not that
  command. A pattern written as a regular expression (`re:`, or a `\b`,
  `\s`, `\d`, `\w` or an alternation group in it) is matched as one,
  anchored at the command's start; the three search-from-root rules in
  the seat's pack were regexes read as globs and had never fired.
- Judging is a decision layer over named judges. `[judges.NAME]` tables
  in `jev.toml` each name a backend, model, key (`key_file`, `key_cmd`,
  or the new `key_env`) and `weight`. `[route]` names the judges for
  each decision: `prompt`, `ballot`, `audit`, `review`. The top-level
  keys stay the `default` judge, so an existing file reads as before.
  Judges on one decision are asked at once and pooled (log-odds mean,
  geometric mean of distributions, score mean); the log keeps every
  judge's answer beside the pool. `command_mode = "prompt"` lets a
  harness's one-shot mode judge: the questions as its last argument,
  the first JSON object it prints as the answer.
- `ljos due --judge`: the review judges weigh each claim on the due
  page against the pack's newer claims on it. A claim that holds at 0.9
  is graded recalled, a contradicted one (0.1) is named to supersede or
  withdraw, and the rest stay due; a judge never lapses a claim.
- Antigravity's `agy` is a runner: `ljos onboard --harness antigravity`
  registers the server in `~/.gemini/config/mcp_config.json`, writes the
  skill under `~/.gemini/config/skills`, and puts the seat's hooks under
  the name `ljos` in `~/.gemini/config/hooks.json`. Its payload names no
  event, so each hook command carries `--event`: the argv law on
  `run_command` answers `allow`, `deny` or a real `ask`; the first model
  call of a turn reads the prompt from the transcript and injects the
  note as an ephemeral step, later calls carry the tool-result notes;
  `Stop` holds a turn with `decision: continue`. `hooks_named` in
  `harnesses.toml` names a hook file of that shape.
- `graded` takes only a claim a due page (`ljos due`, `ljos_due`, a
  sitting) showed in the last hour. A codex seat saved the 1281-row
  `ljos due` list and lapsed all 1314 claims unread in one loop. `ljos
  due` prints the soonest eight; `--all` lists every due claim to read
  and puts none up. The prompt's due line says review is not the task.
- A runner that reads the prompt hook's answer (claude, codex) is no
  longer handed the same note a second time on its first tool result.
- An `ask` rule refused on a runner that cannot ask says the rule does
  not lift on a yes in chat, so the agent stops and hands the command
  to the person instead of retrying it.

- The prompt call asks Jev two more questions in the same request: an
  `injection` noul (quoted or pasted text addressing the agent with
  instructions the person did not write), which adds a treat-as-data
  note at `cue_at`, and an `effort` score from 0 to 3, kept in the Jev
  log for routing. Both are optional in the answer, so an older reply
  still parses; a chat judge's score is clamped to the rubric.
- A chat judge's missing choice confidence is the distribution's
  concentration (one minus normalised entropy), as Jev defines it, not
  the chosen option's probability.

- `backend` in `jev.toml` names the judge for the seat's four judgments:
  `jev` (the default), `chat` (one JSON-mode chat completion on any
  chat-completions endpoint, hosted or a local llama-server) or
  `command` (an argv given the request on stdin, answering on stdout,
  so a harness on the machine can judge). The three request shapes, the
  parsers, the cache, the cost ledger and the callers are shared; a chat
  reply is read into Jev's shape, and a question left unanswered refuses
  the reply. `ljos doctor` prints the backend before the model.

## 0.18.0 (2026-09-30)

- A panel seats a persona matched only through an everyday title word
  ("build", "test", "docs") when no persona speaks to a specific one. A
  hook question titled "which integration to build next" had seated the
  eOn build reviewers beside the seat's own.

- `ljos due` and the prompt hook leave forecasts out. A prediction came
  up for review, agents graded it, and a forecast withdrawn or replaced
  first failed with a raw pack error. `ljos graded` on an atom that is
  no longer current now says so.
- `ljos deed ISSUE --add A --add B` cites several deeds at once, and
  `ljos claims` runs `vissue claims` with its flags.

- A subagent's stop hook names only the issue its own conversation holds.
  A hold written from a shell whose runner the process tree had lost
  recorded the multiplexer (herdr) as its conversation, and every subagent
  under that multiplexer then matched it and was asked for a ballot on
  another session's decision. herdr is a session process now, a command
  under one records its pane's shell, and a hold or seat record owned by a
  session process matches nobody.
- `ljos vote ISSUE --withdraw [--as NAME]` (MCP `withdraw`) takes back a
  ballot and its forecast. The tracker's logbook keeps what the ballot
  was, and the settle and the surprisingly popular reading drop both.

## 0.17.0 (2026-09-29)

- With Jev on, `Stop` and `SubagentStop` audit the turn once from the
  runner's transcript. A final message that claims done beside a red
  test run, or that puts asked work off with no named block, holds the
  agent for one more round with the reason. Whether a test ran is read
  from the commands in code; the second stop is never audited.

- `ljos vote ISSUE --as NAME --jev` and `ljos panel ISSUE --jev` ask Jev
  for persona ballots. A sure ballot is cast with the option's
  probability as its confidence and Jev's forecast as its prediction.
  An unsure one (under `escalate_below`, 0.8) is left for a subagent. A
  panel casts only when every seat is sure and all agree, since personas
  from one model are correlated. `ljos_vote` takes `jev` over MCP.
  Every Jev answer is logged to `jev-log.jsonl`. With Jev, the hook
  ranks what it judged by Jev's probability. An identical Jev request
  within `cache_days` (7) is answered from a local cache at no cost.

- The prompt hook can ask Jev, TypeSafe's decision model, which
  candidate claims bear on the prompt and whether the prompt corrects
  the agent or puts a choice. One call answers all three and replaces
  the local rerank and the phrase lists. It stays off unless
  `~/.config/ljos/jev.toml` sets `enabled = true` with a `key_cmd` (such
  as `pass show`) or `key_file`. The hook skips prompts under four
  words, prompts with fewer than two candidates, and every prompt once
  the month's spend reaches `monthly_usd`. With Jev on, the local
  cross-encoder is never loaded. `ljos doctor` prints a `jev` row with
  the month's calls and spend. `bears_at` and `cue_at` (0.5) set the
  probability each judgment needs.

- A prompt or tool-result hook answers inside 8 s, with no context rather
  than being cut off by its runner; an identical call started in the last
  20 s returns at once, so a hook file two runners load runs once per
  event. The reranked prompt search gets 2.5 s, then the lexical search
  answers. Tool gates are exempt from both.

- A lesson is stored as an episode (`horizon:transient`). It is kept
  and it is not a refresher. A recalled review promotes it to
  `horizon:standing`, and so does consolidation when the lesson
  replaces an earlier claim. `ljos remember --standing` writes the
  rule immediately. A preference is standing. The pack note asks the
  cross-encoder and injects standing claims only. A lesson with no
  horizon tag is an episode.

- A panel matches personas on topic domains. A `sync:` scope stamped on
  the roster is not a topic, so it does not seat everyone who carries it.

- `ljos vote --expect` records the private forecast of the others on the
  same command as the ballot. Briefs, the company-panel recipe, and the
  subagent stop line name that flag. `ljos predict` still records a
  forecast on its own. Two or more forecasts and `ljos consensus` names
  the surprisingly popular answer.

- A runner that discards prompt-hook stdout still receives the pack note,
  on the first tool result. `Stop` speaks only when the turn ran no tool,
  because its feedback would start another round. An empty later prompt
  does not erase a note that has not been delivered. Memory ids and the
  correction, decision, and due nudges are marked seen only once that
  note is emitted.

- `ljos sync` shares the seat's memory across machines through the tracker
  repository. Each repository names a scope and its age recipients in
  `.ljos/sync.toml`, and each machine writes one sealed log under
  `.ljos/atoms/`. The sitting takes the other machines' logs before the
  island, and finish writes this one's. A `[projects]` table sends one
  project's lessons to another scope, and a finish lesson takes the scope
  of the repository its issue lives in. An import skips texts it already
  holds, and a clone another host pushed past merges instead of sticking.

- A finish lesson names the issue it was learned on (`issue:ID`), and every
  claim ljos writes carries `source`: the runner, the conversation, the host
  and, when the runner sets one, the turn.

- The hook answers each runner in the contract it speaks: grok's camel-case
  fields, codex's deny-only verdicts and hermes's `pre_llm_call` context.
  grok's hook file runs ljos by absolute path. `ljos onboard` installs
  plugins for opencode and omp, whose hooks are code.

- A conversation is one holder across nested runners and threads, a
  runner's client names (for example `claude` and `claude-code`) are one
  seat through the harness `clients` list, and a library's default client
  name is no seat.

- A subagent is told, on its first tool result, the issue its parent holds
  and how its result joins it: a ballot `--as` its role, a lesson or a
  note. grok's `SubagentStop` holds it once while that issue is open, and
  asks for the ballot on a decision. The parent's issue is found under the
  holder the runner's server recorded.

- A decision issue binds `company-panel` in the sitting and writes one brief
  per seated persona; `finish --close` refuses a decision with fewer than
  two ballots. A prompt that puts a choice in its opening is sent to a
  panel once a session. A panel seats personas by the issue's tags and by
  its island only when the island is strong. When only specialists exist
  and none matches, the five whose views use the issue's words most sit,
  and a panel with nobody to seat says so and stops.

- Every verb that writes the tracker commits and pushes it, to every remote
  that carries the branch. Seats sharing one checkout queue their commits
  under `ljos-commit.lock` and wait out another git process's `index.lock`.
  A rejected push merges and pushes once more.

- An `ljos-mcp` whose binary was replaced forwards each tool call to the
  installed one, so an install reaches a running agent.

- The doctor gains a host row (out-of-memory kills, the kernel, the ljos-mcp servers
  and their memory), a probe row that checks a runner loads the ljos tools,
  and tracker-row failures for remotes that disagree and for a clone that
  lacks the merge driver its `.gitattributes` names.

- The sitting's due list puts the claims its island holds first, unless the
  island is weak. The timeline reads every store on the reader's local
  day, and the pack is read without its vectors.

- The doctor's tracker row names how many commits the checkout holds that
  origin does not. A count that has sat through the push wait fails the
  row; a leftover `tracker-push-*.log` from a refused push is named on it.
  A live background push, or commits younger than the wait, stay healthy.

- `scripts/smoke.sh` walks two seats handing one scratch ticket in
  sequence: A sits, cites a deed, hands over; B imports; A releases; B
  sits; both vote; consensus; finish closes; the tracker git line reports
  the commit. Distinct from the herd's concurrent contention. A second
  sitting on a closed ticket reopens the tracker heading. The HUD
  message enum boxes its snapshot so clippy `large_enum_variant` stays
  clean.

- The tracker push after a claim or finish waits at most 5 seconds
  (`LJOS_TRACKER_PUSH_WAIT`). A pre-push hook that publishes data first
  held every sitting for minutes; a push still running finishes in the
  background and the `tracker git:` line names its log.

- The doctor's host key row fails when the deed store's `layout` does not
  list the key (`deedar host`), instead of reading healthy while every new
  deed fails `evidence`.

- A seat record carries the conversation ids its writer held, and a shell
  refuses a record written under an id it shares with another
  conversation. Two conversations started from one terminal share the line
  editor's session id, and a shell could take the other conversation's
  holder and then fail to finish its own claim.

- `ljos` and `ljos-mcp` expand a leading `~` in `ISSUE_ROOT` and
  `VISSUE_ROOT` at start. The linked tracker crate took such a root as
  relative to the working directory, so `ljos finish` could not find a
  ticket `vissue` itself resolved.

- `ljos sitting` after its claim and `ljos finish` at the end commit the
  ticket's tracker file (that file only) and push it, when the tracker is a
  git checkout. A closure that stayed in one working tree was lost to every
  other host. `LJOS_TRACKER_GIT=commit` commits without pushing; `=off`
  skips it. A refused push is reported and does not fail the verb.

- On a fresh host the doctor's claim graph row reads ok, `none yet; the
  first claim creates it at DIR`, instead of a failing row in every
  sitting header. Any other claimdag refusal still fails it.

- The doctor's tracker row names the root vissue resolved, its prefix, and
  where the root came from (`ISSUE_ROOT`, `VISSUE_ROOT`, seat config or the
  working directory). A relative or missing root, or one with no prefix
  directory, fails the row.

- Onboarding retains registration and skill result order while initializing
  the pack and host key before configuring the client.

- Forecast scoring accepts the decimal strings emitted by the tracker as
  well as JSON numbers. Invalid probabilities are reported instead of being
  silently treated as absent forecasts.

- Doctor checks that the installed tracker accepts evidence citations and
  forecast confidence on ballots. An incompatible vote command fails the
  required tracker row, even when its version is listed in the registry.

- `ljos hud` opens the summonable icedtea pane (`ljos-hud`, a workspace
  member): due, claims, trust canvas, island, and the deed rail. The
  `ljos` crate does not link iced. Pane behavior is the 0.16.3 notes.
- Five playbooks (`sit`, `arena`, `land`, `company-panel`, `overnight`) are
  kind `playbook`, weighed not recalled. `ljos playbooks` lists them;
  `ljos playbook ISSUE NAME` binds one and copies the full recipe body.
  The name is a tracker `playbook:` note until finish or release; those
  verbs write an empty `playbook:` sentinel so the next sitting does not
  reprint it. A different name while one is held is refused. `ljos sitting
  ISSUE --playbook NAME` (MCP `playbook`) prints `== playbook` with that
  body before recall; absent a name, a closed-set token in the title else
  `sit`, so a sitting always binds one of the five before claim. Pack
  latest per name is the copy source; shipped bodies seed only when the
  pack has no live atom of that name. Write, list, bind, and copy refuse
  names outside the five.
- The unscoped inbound floor a persona is owed is the seat's own row
  (`from` is the seat). A third-party unscoped row does not skip it.
- `ljos brief` carries three blocks: the bound playbook's full recipe,
  five named principles (split-fence, prove-on-real-surface,
  open-sibling-first, arena-then-compose, one-step-delegate), and the
  arena rubric. `ljos panel` refuses until a playbook is bound.
  Model names on a recipe are optional spawn hints; every panel still
  ends in `ljos vote --as` then `ljos consensus`.
- Writing a persona also writes one unscoped inbound trust row (the seat
  weighs it at 1, everywhere). `--about` on a later trust row only adds
  weight.

## 0.16.3 (2026-09-22)

- `ljos hud` execs sibling `ljos-hud` (`LJOS_HUD_BIN`, same directory, then
  PATH). Missing is 127; `--hide` with nothing running is 0. The `ljos`
  crate does not link iced. Dist ships the HUD on
  `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu` only.
- `ljos-hud` is a long-lived icedtea daemon: summon socket
  (`LJOS_HUD_SOCKET`, `$XDG_RUNTIME_DIR/ljos/hud.sock`), overlay
  `me.rgoswami.ljos-hud`, pop-out `me.rgoswami.ljos-hud.window` (P),
  StatusNotifier tray id and `.desktop` `StartupWMClass` are the overlay
  app_id, `--install-desktop`, guest `xdg_activation_v1`. Default
  detaches; `--foreground` stays attached. First-start `--show`/`--toggle`
  keeps `XDG_ACTIVATION_TOKEN` for the owner (hide still unsets without
  activating). Overlay `Closed` hides. Three read-only panes over library
  APIs: due (with `due_at`, overdue when `due_at` is past, later when
  future, ungraded when empty; no exact-equality `due` chip;
  `review_summary`, display-only grade chips),
  claims (assignee, `cas_gen`, occupancy, lease remaining), a trust-graph
  canvas (personas ∪ trust endpoints, stroke by weight, ring by
  `(1 - anchor)`, missing persona a hollow disk, dashed when scoped),
  island (`packset_search` plus `packset_island(cue, false)`; idle until
  enter; `format_island` weak/dense banners; search-down is a banner), and a deed
  rail (`ljos_cli::timeline_events` → `Vec<Event>`; idle until the
  operator enters an issue; tracker rows from
  `vissue_core::agent::show_json`, not `vissue show --json`). Skip chips sit in
  the layout in key order 1-5 (due, claims, graph, island, deeds). Watch
  `work.bin` plus pack `last_write_ts` (Snap stamps both; a missing
  `pack_ts` is boot, not a load); the 50 ms tick is chrome. A
  habitat that is down is an icedtea banner and a status page; pack-down
  and honest-empty differ, and a claims banner does not blank an up-empty
  graph. HUD sources never call `graded` / `post_atom` / `sweep` /
  `fire=true` / `due_report` / `complete`.
  `ljos doctor` lists `ljos-hud` and does not require it. crates.io
  publishes `-p ljos-hud` after `-p ljos`. The HUD crate does not copy
  the CLI launcher (`resolve_hud_bin` stays in `ljos-cli`).
- `ljos remember` and `ljos search` start the default writer when none is
  answering. `PACKSET_URL=off` stays off. A URL pointed elsewhere is not
  replaced. The crates.io description and keywords name the first command.
- `ljos finish` no longer closes the tracker ticket on `--status done`; a
  sitting ending is not the work being accepted, and a ticket closed early
  released every blocker on it. `--close` (`close` over MCP) closes it.
  The protocol document already said so; the tool now agrees.
- `--gen` on `ljos finish` and `ljos complete` (and `gen` over MCP) is
  optional: absent, the live generation is read off the claim graph. An
  explicit stale generation is still refused.
- `ljos claim`, and so a sitting, stamps the tracker as well as the claim
  graph: `vissue claim ISSUE` runs under the assignee's name, so the issue
  reads STARTED and `vissue claims` names who holds it. A tracker that
  refuses the name fails the claim with `ljos release` named as the way
  out; a node the tracker does not know is left alone.

## 0.16.2 (2026-09-22)

- Doctor compares the `ljos-mcp` binary to the `ljos` crate. That crate
  ships the binary; the crates.io name `ljos-mcp` stopped at 0.14.0.
- A sitting sweeps the review clock before it prints the due prefix, the
  same sweep `ljos due` already ran.
- `ljos_due` returns the soonest eight claims, the total due, and the
  clock summary. The full list stays `ljos due`.
- A panel that matches no domain seats only the personas that name no
  domain. It does not seat every specialist in the pack.
- `ljos vote` prints a count and says so. The settle stays `ljos consensus`.
- An island prints whose weights it walked, and whether fire rewrote them.
  Activation is spread along links, not a rank. A persona brief says to
  walk its own island and to fire only after that island was used.
- A search score is a rank from the scorers, named as such. Learn names
  the rows it rewrote and says the call is not a settle. Finish names
  that the island it fires is the seat's.
- A ballot can state a probability and the deeds it used. The probability
  is the voter's initial opinion and, once an outcome is known, a Brier
  score. The score is not a trust weight. A fire records a trace of the
  links it strengthened.
- A stated probability is also scored by the logarithmic score. The
  voter's running record keeps mean probability against the event rate,
  and from the second forecast Murphy's reliability, resolution, and
  uncertainty. Those scores stay off the trust weight.
## 0.16.1 (2026-09-20)

- `cargo binstall ljos` takes the GitHub tarball (`ljos` and `ljos-mcp`)
  and skips cargo-quickinstall, which only had `ljos`.
- `scripts/smoke.sh` runs `scripts/herd.sh` after the four loops, so the
  herd sits beside the smoke in the release check.

## 0.16.0 (2026-09-20)

- `ljos bump-plan BUNDLE --project P --parent I` puts an eb-stack bundle
  on the tracker: one child issue per module the lock builds, blocked by
  the modules built before it along the SBOM's dependency edges, with ids
  that are a hash of module and generation so a rerun holds what exists.
  `vissue ready` is then the buildable frontier and a sitting refuses the
  rest. `ljos_bump_plan` over MCP; `--dry-run` prints the rows.

## 0.15.0 (2026-09-20)

- A sitting reads the issue's blockers from the tracker before it claims:
  an issue whose blockers are still open is refused, with the blockers
  and their states named, and nothing is claimed. `ljos sitting ISSUE
  --anyway` (MCP `anyway: true`) sits on it regardless and says so. The
  claim graph and the tracker's graph agree on what is workable.

## 0.14.3 (2026-09-20)

- `ljos findings --issue` run twice on one state file cites the deed the
  first run froze instead of failing on the store's refusal.

## 0.14.2 (2026-09-20)

- `ljos findings --issue` reads the accession out of `id=deed-...`, which
  is what `deedar create` prints; the first run against a real campaign
  wrote its lessons and then failed to cite the state file.

## 0.14.1 (2026-09-20)

- A finding names the module whose build failed (`GCCcore-15.2.0` for
  `eOn-2.17.10-foss-2026.1`), read from EasyBuild's own line, not only
  the recipe the campaign drives; the lesson and its entities carry both.
  A finding a later attempt got past says so in its second sentence
  instead of quoting the campaign's automatic resolution.
- A pack refusal on prose reaches the MCP client with what passes, so the
  second attempt is not a guess; four runners hit the ceiling on their
  first lesson.
- The `ljos` crate is the one published: it carries the `ljos-mcp` binary,
  so `cargo install ljos` and `cargo binstall ljos` install both. The
  `ljos-mcp` package stays in the workspace unpublished; the crate already
  on crates.io stays where it is.

## 0.14.0 (2026-09-20)

- `ljos findings STATE` reads an eb-stack campaign state file and prints
  its typed findings; `--remember` writes one lesson per finding a person
  or a seat resolved (the recipe, the step, the error line, the fix) under
  the recipe's name, its package and the failure class; `--issue` cites
  the state file as a deed. `ljos_findings` over MCP. The protocol and
  the how-to carry the bump: one island per recipe before it is touched,
  the ladder rung by rung, the findings remembered after.
- A claim leaves a hold record beside the claim graph: the name it is
  held under, the seat, the runner process and when. A sitting that finds
  the node held by another conversation now names it and says whether its
  runner still runs; when the holder is this seat's own conversation and
  its runner is gone, the sitting takes the node over. A runner that
  exited without finishing no longer blocks the next run of the same
  seat.
- `ljos_remember` says what the pack refuses: a third sentence, prose
  above readability grade 14; and what a lesson names.
- `ljos onboard --example` carries the runners this seat has carried
  through one piece of work (opencode, hermes, omp, grok beside the two
  generic shapes), each in the shape it takes the server; the how-to says
  which take the memory hook.

## 0.13.8 (2026-09-20)

- A conversation's holder does not move when a second `*_SESSION_ID`
  appears: the first resolution leaves a record under every stamped id,
  and a later process carrying one of them and more finds the holder by
  the shared id. A sitting opened under one id is finished under it when
  a line editor has stamped another since.
- `ljos doctor` says where a registry answer came from (`crates.io
  (cached)` when read from the day cache) and labels a binary ahead of it
  as well as one behind; a cached answer the binary on `PATH` is already
  ahead of is asked again.
- `ljos_policy` returns what `ljos policy` prints: the TCB verdict, the
  rule that fired and the memories the line activates, under `ruling`.
  `ljos_consensus` runs the surprisingly popular answer and the voters'
  standing beside the settle, as `ljos consensus` does.
- The `ljos-mcp` crate page carries the README.

## 0.13.7 (2026-09-20)

- `ljos onboard` registers into a runner's JSON config by pointer: a
  harness names `config_json`, `json_pointer` and a `json_entry` template
  (opencode's `mcp` object, for one), and registered means the pointer
  resolves.

## 0.13.6 (2026-09-20)

- Every MCP tool answers an object: the ten that answered a bare array
  now answer `{ rows: [...] }`, since a strict client refused the whole
  server over an array output schema.

- A persona's memory tree: `remember --as` and `prefer --as` write into
  the set `persona-NAME`, its own tree for the duplicate and replacement
  rules; `ljos island --as NAME` and the `ljos_island` tool's `as` walk
  the pack through the persona's own link weights and fire those, not
  the seat's. Facts stay one substrate; readings and paths are the
  persona's. `packset-client` 0.9.20.

## 0.13.5 (2026-09-20)

- A finish says when the island fired already this hour (the pack holds
  a second fire of the same claims for an hour, so several seats or
  personas closing sittings on one issue tighten its links once).
  `scripts/herd.sh` runs eight sittings from four seats at once, contends
  one ticket between two seats, and closes all eight in parallel.

## 0.13.4 (2026-09-19)

- The roster, a brief and the panel prompt read only the persona atoms
  (`packset-client` 0.9.17), not every atom in the workspace.
- `ljos doctor` has a memory row: the live count against the cap and
  the claims forgotten, by reason (packset 0.9.17 counts them).
- Writing a persona that the pack already holds supersedes its previous
  atom, so moving an anchor or a view leaves one live persona of that
  name; the roster showed one, the pack kept both.

## 0.13.3 (2026-09-19)

- `ljos personas` and the `ljos_personas` tool print the roster the pack
  holds: name, anchor, the domains each speaks to, its view. The only
  way to see it was the panel prompt.
- `ljos due` sweeps the pack first and says what the sweep did: reviews
  left due past twice their interval lapse, never-recalled claims missed
  three times are forgotten by neglect, and the list that follows is the
  one after that. `packset-client` 0.9.14.

## 0.13.2 (2026-09-19)

- A panel seats only the personas whose domains the issue speaks to,
  read from its title's words and the island it activates; every persona
  sits when none speaks to it. The `ljos panel` brief and the
  `run_a_panel` prompt agree on the roster, and the prompt says how many
  of the pack it seated.
- `ljos onboard --harness grok` bumps `LJOS_MCP_GENERATION` in the runner
  config when the crate version moved, so Grok's watcher respawns
  `ljos-mcp` and a session restart is not required.

## 0.13.1 (2026-09-19)

- The holder is any `*_SESSION_ID` the runner set, the full value, ahead
  of the process tag. Two ids that share an eight-character prefix occupy
  different slots. A server sitting and a shell sitting of one session
  are one occupancy name.
- `ljos finish ISSUE` with status done (the default) closes the ticket
  in the tracker, so a board never shows TODO over a completed claim and
  hands the work out again. Failed or cancelled leaves the ticket where
  it is. Completing a node alone still does not close a ticket.
- The server's seat record is also kept under each conversation id the
  runner stamped, and a shell reads it by any id it shares with the
  server. A line editor that stamps a session id of its own into the
  shell no longer makes that shell a second holder.
- Every write names the seat that wrote it (`seat:<name>` first among
  the entities; a persona's, a habit's or a trust row's entities join it,
  a trust row's stay the deeds it cites). A hit written by another seat
  says so: `[lesson, 3 days ago, from brio]` in the hook and a brief,
  `(from brio)` in `ljos search`, `from` on the `ljos_search` row. Many
  seats share one pack; a reader now sees whose lesson it is reading.
- `packset-client` 0.9.12, whose hits carry entities and whose writer
  holds a workspace at a live cap.

## 0.13.0 (2026-09-19)

- The doctor keeps each crates.io answer on disk for a day, so a herd of
  seats opening sittings asks the registry once a day per binary rather
  than once a sitting each. A busy refusal names this conversation's
  holder and the release that frees a conversation that is gone.
- Habits: `ljos habit NAME VALUE [--unit U] [--every 7d] [--source S]`
  takes a reading of a number the seat keeps measuring, as a claim of
  kind `habit` that supersedes the earlier reading and carries it as
  `was`, due for its next reading one cadence on; `ljos habit` lists
  them with the change since the last reading and when the next is due.
  `ljos_habit` is the same over MCP.
- The seat names itself. `ljos-mcp` takes the client's name at initialize
  (`acme-cli`, `brio`, whatever the runner says) as the seat and
  leaves a record under the runtime directory keyed by the runner's
  process; `ljos` in a shell that runner opened walks its own process tree
  to the same record, or to the first ancestor that is not a shell, so a
  runner's tools and its verbs are one seat with nothing set and nothing
  in `env`. Two names: the seat, which memory, ballots and trust accrue to
  across conversations, and the holder (`acme-cli-39u`), which this
  conversation's claims are held under. `ljos seat` prints both; the
  doctor's `seat` row does too. `LJOS_SEAT` still overrides. The runners
  file no longer passes `LJOS_SEAT={name}`; `ljos onboard` alone prints
  the one entry any runner takes. A ballot cast without a persona is cast
  as the seat.
- `cargo install ljos` installs `ljos` and `ljos-mcp`.
- Occupancy is `{holder}:{issue}` and the holder is any `*_SESSION_ID` the
  runner stamped, else the seat tagged with the conversation's process,
  else `LJOS_SEAT`. No product list. Two conversations hold two tickets;
  the same ticket is still one holder. `doctor` prints the session and
  which variable it came from.

## 0.12.15 (2026-09-15)

- Sitting no longer treats a binary behind crates.io as a habitat
  that does not answer. Doctor still names the gap.

## 0.12.14 (2026-09-15)

- The README no longer says the hook never blocks. A TCB deny on
  PreToolUse still blocks.

## 0.12.13 (2026-09-14)

- `ljos remember` and `prefer` print one line (id, kind, due, text).
  They no longer dump the embedding.

## 0.12.12 (2026-09-14)

- `packset-client` 0.9.2, so the seat tracks the writer that prints
  `packset-mcp --version`.

## 0.12.11 (2026-09-14)

- Doctor times out MCP `--version` so a silent stdio server cannot
  hang the seat. `ljos-mcp` depends on the workspace ljos version.

## 0.12.10 (2026-09-14)

- `packset-client` 0.9.1. `ljos-mcp --version` prints and exits.
  Doctor can version the MCP binary. The door install includes
  `packset-embed`.

## 0.12.9 (2026-09-14)

- `ljos doctor` lists every seat binary (`ljos`, `packset-embed`,
  `packset-mcp`, …) with the version on PATH against crates.io. A
  part that is missing or behind is not ok. Encoder and policyd are
  required with the rest.

## 0.12.8 (2026-09-14)

- `ljos doctor` treats the grok harness hook as the frozen events
  (prompt, PostToolUse, Bash rules, SessionEnd), not a stale
  SessionStart list. `~/.config/ljos/env` is loaded when those
  keys are unset, so a shell `ljos` shares the MCP pack. Argv law
  runs only on PreToolUse and argv, not on PostToolUse.

## 0.12.7 (2026-09-14)

- `ljos onboard --harness grok` writes the frozen `~/.grok/hooks/ljos.json`.
  No table in harnesses.toml is required.

## 0.12.6 (2026-09-14)

- Grok hook is `ljos hook` only. The prompt's pack text is held and
  emitted once on `PostToolUse`, the event Grok delivers. No
  `sync.sh`. `PreToolUse` stays rules-only.

## 0.12.5 (2026-09-14)

- Grok `PreToolUse` no longer remaps to a pack search. The inject
  script exits on a tool call. `ljos hook` on Bash only decides
  rules. The due nudge no longer walks `consolidate` (that sitting
  is what timed the hook out at 20s).

- `finish` does not fire a weak island (seeds no two scorers agreed on)
  and says why; `island` marks one as weak and as the pack's
  best-connected cluster rather than what the cue is about; `doctor`
  shows the encoder row, since a down encoder is what makes seeds weak.

## 0.12.4 (2026-09-14)

- Grok onboard writes `GROK_SESSION_ID` into the MCP env so the
  occupancy split in 0.12.3 is live for that runner.

## 0.12.3 (2026-09-14)

- Occupancy uses the runner session when the assignee is a shared
  name (`grok`, `seat`, `you`). Two conversations hold two tickets.

## 0.12.0 (2026-09-13)

- `learn` writes rows from each voter's record: hits and misses so far
  as log-odds weights, the same scale `calibrate` writes, carried on the
  rows. On voters of known accuracy the record reaches batch calibration
  (0.929 against 0.934 over 8000 decisions) where Hedge reaches 0.831 and
  Hedge with recovery 0.877. `--rule hedge` keeps the shrink.
- `conflicts` leaves out passes between trust rows, personas, forecasts
  and rules: weighed, not recalled, so not contradictions to judge.

## 0.11.0 (2026-09-13)

- `conflicts` (and the `ljos_conflicts` tool): candidate contradictions
  by geometry, the lowest passes between single memories in the pack's
  embedding landscape, from the optional `landscape` habitat; nothing
  written.
- The hook reads a correction: a prompt that opens with "do you not
  remember", "you should have", "I told you" and the like gets one line,
  once per cue a session, to write the preference or lesson into the pack
  before the work. A correction the pack never held cannot fire.

## 0.10.0 (2026-09-12)

- `doctor` prints a `seat` row: the name this runner claims and votes
  under, and whether it came from `LJOS_SEAT`, `VISSUE_AGENT` or the
  default.
- `calibrate` writes log-odds weights (Nitzan and Paroush): a voter right
  nine times in ten now outweighs one right six times in ten five to one,
  where the linear rule gave three to two; chance earns the floor.
- `search` prints how many scorers named each hit; the prompt nudge counts
  the pairs `consolidate` would close beside the claims due.
- `consolidate` (and the `ljos_consolidate` tool): the pack's replacement
  rule run over what it holds, pairs reported, `--apply` to write.
- The hook injects only hits at least two of the pack's scorers named
  (`ballots` of `of` on a hit), when more than one ran; a claim one
  scorer alone matched on a command line stays in the pack.
- A second `sitting` on an issue this name already holds is a sitting
  resumed: the lease is renewed and the verb goes on, where it refused
  with `claim: status claimed`. Held by another seat, the refusal names
  the actor.

## 0.9.0 (2026-09-12)

- Every recalled memory carries its age: the hook's lines, a brief's
  lines, `search` and the island in `sitting` say `today`, `3 weeks ago`,
  `6 months ago` beside the kind, and the hook's lessons run oldest to
  newest behind the preferences. The reader lays what it recalls on a
  timeline instead of a bag.
- `timeline ISSUE` (and the `ljos_timeline` tool): the tracker's logbook,
  the cited deeds and the activated memories as one dated list, oldest
  first, each line with its age and the gap since the line before.
  `sitting` prints the last twelve as `== timeline`.
- Two runners on one host: `--assignee` defaults to `LJOS_SEAT`, then
  `VISSUE_AGENT`, then `seat`; a ballot cast without a persona is cast as
  `LJOS_SEAT` when set; the runners file may write `{name}` in `register`
  and `snippet`, so a registration passes `LJOS_SEAT={name}` to the server.
- `remember` and `finish` say when the pack closed earlier memories for
  the new one (`revises N earlier memories, now closed`), so a revision is
  seen as one.
- `search --as-of TIME` (and `as_of` on the `ljos_search` tool): the pack
  as it stood then, so "what did the seat know when it decided that" has
  an answer.

## 0.8.0 (2026-09-12)

- `scripts/smoke.sh`: every loop on scratch stores, as a check.
- `sitting` prints the island's strongest eight; `island` prints it all.
- A habitat cut off by the seat's own reader closing the pipe is not a
  refusal: `ljos consensus ID | head` ends quietly.

## 0.7.0 (2026-09-12)

- `learn --share S`: a fixed share of recovery toward one after the Hedge
  step (Herbster and Warmuth), so a voter refuted long ago can come back;
  zero, the default, is plain Hedge.
- `receive --import` tags every imported atom with its sender
  (`from:<signing key>`, or `from:handover` for an unsigned bag).
- `onboard` starts a pack writer when none answers, before wiring the
  runner to it.
- The hook fires the memories it injected during a session together when
  the session ends (the runner's `SessionEnd` event, on by default), so
  what served one sitting is wired for the next.

## 0.6.0 (2026-09-12)

- `ljos hubs`: the claims the pack's link graph turns on, highest first.
- `handover --to user@host:path` copies the sealed, signed bag to another
  seat over ssh; the receiver runs `ljos receive`.
- `predict ISSUE --expect OPTION` records a forecast of the others; with
  two or more, `consensus` prints the surprisingly popular answer (Prelec,
  Seung and McCoy) and, with trust rows, each voter's EigenTrust standing.
- `rule PATTERN --verdict deny|ask --why TEXT`: argv law in the pack. The
  hook returns the verdict as the runner's permission decision on tool
  calls; `policy` prints it beside the line. Over MCP: `ljos_predict`,
  `ljos_rule`.

## 0.5.0 (2026-09-12)

- `ljos search -n N --rerank`: the writer's cross-encoder over the top hits.
- `ljos panel ISSUE --out DIR`: every persona's brief as a file, so a runner
  without MCP can start one subagent per persona.
- `remember --as NAME` and `prefer --as NAME` (and `as` on the tools): a
  persona keeps lessons of its own, which open its next `brief`.
- `handover` signs the manifest with the seat's default host key, not only
  with one named by `DEEDAR_HOST_SIGNING_KEY`; it went out unsigned while
  `doctor` reported the key present.

## 0.4.0 (2026-09-12)

- The kind of work sets the dynamics: an issue tagged `broad` settles
  under bounded confidence on the model crate; untagged issues run the
  anchored model. On a prompt the hook says, once per session, how many
  claims are due for review. `sitting` no longer waits on the runners'
  command lines; `doctor` asks them beside the seat's own rows.
- `learn` moves a refuted persona's anchor toward one, and a scoped trust
  row that applies stands in for the unscoped row of its pair instead of
  adding to it. Islands print claims only; persona and trust atoms are
  weighed, not recalled.
- `ljos brief NAME ISSUE` and `ljos_brief`: the text a subagent playing a
  persona starts from; `run_a_panel` starts each member from it.
- `--version`. A claim on an issue whose earlier sitting finished reopens
  the session node (`claimdag reopen`) and takes it.

## 0.3.0 (2026-09-12)

Memory at the point of action:

- `ljos hook` reads a runner's hook JSON (or an argv line) on stdin and
  answers with the memories the action activates, preferences first, in the
  runner's `additionalContext` shape or plain lines. `onboard` installs it
  on a runner's prompt event when its table names a `hooks` file, or on the
  events `hook_events` lists, and drops it from the rest; `doctor` shows
  the row. The default was settled by a panel of the seat's personas. `ljos policy` prints the memory beside the
  argv line.

Personas and scoped trust:

- `ljos persona NAME --anchor A --view TEXT [--about DOMAIN]` writes a voter
  with a view; `ljos vote --as NAME` casts as it; `consensus` passes every
  persona's anchor to both settles as `--susceptibility-of`.
- Trust rows carry `about` domains: an unscoped row applies everywhere, a
  scoped one when the issue's title carries the word. `learn` writes rows
  scoped to the entities of the issue's island.
- Over MCP: `ljos_persona`, `as` on `ljos_vote`, `about` on `ljos_trust`,
  and the `run_a_panel` prompt: one subagent per persona, one ballot each,
  then the settle.

The loop runs every time:

- `ljos sitting ISSUE --assignee NAME` opens a sitting in the protocol's
  order (doctor, cards, due, island, recall, claim) and stops at the first
  store down; `ljos finish ISSUE [--lesson] [--outcome]` closes it
  (remember, fire, complete, learn) and says when no lesson was given.
  Both over MCP as `ljos_sitting` and `ljos_finish`.
- `ljos calibrate -p PROJECT` writes trust rows from the project's voting
  history with no truth labels (Dawid and Skene, through
  `ljos-consensus reliability`), so weights move when nobody names an
  outcome.
- `ljos due` lists claims that never entered the review clock as due, and
  ends with one line on the clock: due, scheduled, next.
- `ljos onboard` writes the seat's host key when there is none, so
  handovers go out signed.

For an agent, or the person running one:

- `ljos protocol` prints the sitting protocol: which store answers which
  question, the order of verbs before, during and after the work, and the
  refusals worth knowing. The server serves it at `ljos://protocol` and
  names it first in its instructions.
- `ljos onboard --harness NAME [--dry-run]` registers `ljos-mcp` with a
  runner described in `~/.config/ljos/harnesses.toml` and installs the
  protocol as its skill; `--harness json` prints the entry for any other,
  `--example` the file's shape. `doctor` reports whether each runner named
  is onboarded.
- `ljos release ID --assignee NAME` and `ljos_release` hand a session node
  back unfinished. A claim refused as busy now names the tracker id the
  name still holds and the two verbs that free it.
- Nothing needs a variable set: the pack is found on `127.0.0.1:8761`
  (`PACKSET_URL=off` means no pack), the seat's memory is the one
  workspace `seat` from any directory (`PACKSET_WORKSPACE` names another),
  and the host key at `~/.config/deedar/host.key`.
- Every verb and flag has help; every tool description opens with when to
  call it; the claim and finish arguments say they take tracker ids.

## 0.2.0 (2026-09-12)

The seat over four stores, as one command and one MCP server.

- Memory: `remember`, `prefer`, `forget --why DEED`, `search`, `island
  CUE [--fire]`, `due`, `graded ID [--lapsed]`.
- Agreement: `vote`, `consensus` under the pack's trust rows on both the
  model crate and the tracker's verb, `trust FROM TO W --why DEED`, `learn
  ID --outcome OPTION` (Hedge reweighing, rows written whole).
- Work: `claim` and `complete` take a tracker id or a name and map them to
  claim-graph ids; `deed`, `recall`.
- Handover: `handover --out DIR` packs the satchel, the atoms and the deeds
  both cite, seals and signs; `receive DIR [--since] [--import]` checks and
  imports.
- `doctor` reports every habitat, the host signing key included.
- MCP: twenty-two tools with read/write annotations, two resources, two
  prompts that sequence a sitting and a handover check.
- A documentation site and handbook at https://leidarljos.github.io/ljos/.

## 0.1.0

The first seat: remember, prefer, search, evidence, current, deed, recall,
vote, claim, complete, cards, policy, consensus.
