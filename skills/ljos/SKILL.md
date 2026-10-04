---
name: ljos
description: >
  The seat protocol for vissue, packset, deedar, claimdag and consensus through ljos: which store answers which question, the order of verbs in a sitting, and the refusals worth knowing. Load before any work that touches an issue, a memory, a deed, a claim or a vote.
---

# The seat protocol

One seat, five stores, five questions. Ask the store that owns the question.
`ljos` is the one command in front of them; `ljos-mcp` serves the same verbs
over the Model Context Protocol (MCP). Every verb below has a tool of the same
name with the prefix `ljos_`.

| question | store | verbs |
|---|---|---|
| what did the human freeze | cards | `cards` (read-only; never extract-on-write) |
| what is the work, what blocks it, who agrees | tracker (vissue) | `file`, `note`, `recall`, `vote`, `consensus`, `deed`; `vissue append`, `vissue update` |
| what does this seat know, standing | pack (packset) | `search`, `island`, `remember`, `prefer`, `forget`, `due`, `graded` |
| what did the work produce | deed store (deedar) | `evidence`, `current`; `deedar create` |
| which work is claimable right now | claim graph (claimdag) | `claim`, `release`, `complete` |
| how do the voters weigh each other | pack, trust rows | `trust`, `learn`, `calibrate` |
| who votes with a view of its own | pack, persona atoms | `persona`, `vote --as` |
| which recipe this sitting copies | pack, playbook atoms | `playbook`, `playbooks`; `sitting --playbook` |

A failure is a store not answering. It is never an empty answer. When a verb
fails, run `doctor` before drawing any conclusion.

## Before the work: a sitting

One verb runs the whole opening in order and stops at the first store that
does not answer:

    ljos sitting ISSUE --assignee NAME [--playbook NAME]

It prints nine sections, and each one is a step you would otherwise run by
hand. Each answers something the next one needs. Between the island and the
recall it reads the issue's blockers from the tracker: an issue whose
blockers are still open is refused before anything is claimed, because the
graph says it is not workable; `--anyway` sits on it regardless and says so.
Then it copies one playbook into `== playbook` before recall: `--playbook`
names it, else a name already bound on the issue, else a closed-set token
in the title, else `sit`. Sitting always binds one of the five before
claim. A panel is refused until one is bound. The name lives on the
tracker as a `playbook:` note until `finish` or `release`.

1. `ljos doctor`. A `no` on `tracker`, `deed store` or `pack` is the answer;
   `packsetd` on a scratch port starts a pack writer. Do not proceed on a `no`.
2. `ljos cards`. What the human froze. Read, never write.
3. `ljos due`. What is due for review: the soonest eight and the total.
   The verb prints a page; it does not grade. After the sitting, check a
   shown claim against the work and `ljos graded ID` (`--lapsed` when it
   no longer holds). `graded` takes only a claim a due page showed in the
   last hour, so a saved list cannot be graded unread; `ljos due --all`
   lists everything to read and puts none of it up. The review clock
   moves only when you grade, and a backlog is left due, not drained.
4. `ljos island` on the issue's title. The sitting takes the strongest
   eight. The number on a row is spread along links, not a rank.
   `--as NAME` walks that persona's weights. `--fire` after the island
   was used rewrites those weights, and the next walk follows them. A
   weak island does not fire. `ljos search TOPIC` and a full
   `ljos island TASK` are during the work, not this opening.
5. `ljos playbook ISSUE NAME`, or `--playbook NAME` on the sitting. The
   five shipped recipes are `sit`, `arena`, `land`, `company-panel`,
   `overnight`. Kind `playbook`, weighed not recalled. Pack latest per
   name is the copy; shipped bodies seed only when the pack has no live
   atom of that name. Write, list, bind, and copy refuse any other name.
   `ljos playbooks` lists the five. Absent a name, the sitting matches
   the title or binds `sit`. Mid-sitting turns re-read the same note; a
   new task is a new sitting. Finish and release write `playbook:` so
   the next sitting does not reprint the previous recipe.
6. `ljos recall ISSUE`. The plan, the inputs' deeds, and what the issue has
   cited so far.
7. `ljos timeline ISSUE`. The last twelve dated events across the three
   stores; `ljos timeline` without a sitting prints them all.
8. `ljos claim ISSUE --assignee NAME`. Occupancy is `{name}:{issue}`:
   two conversations hold two tickets. The same issue is still one
   holder. `busy` on a named worker means that name still holds another
   node (`ljos complete` or `ljos release`). The tracker moves to STARTED under the same name, so
   `vissue claims` answers who holds it; a tracker that refuses the name
   refuses the sitting, and `ljos release` frees the claim graph.

A decision is handed to the panel by the sitting. An issue tagged
`decision`, typed `decision`, or with a body line opening `Options:`
binds the `company-panel` recipe, and the sitting writes one brief per
persona the issue speaks to under `== panel`. Start one subagent per
brief; each ends with `ljos vote ISSUE --for OPTION --as NAME`, then
`ljos consensus ISSUE`. `ljos finish ISSUE --close` refuses a decision
with fewer than two ballots. Put any choice with more than one defensible
answer on an issue this way before building it.

Memory crosses machines through the tracker repository. A repository
whose `.ljos/sync.toml` names a scope and its age recipients carries one
sealed log per machine under `.ljos/atoms/`. The sitting pulls and takes
the other machines' logs before the island is walked; finish writes and
pushes this machine's. `ljos sync --key` prints this machine's public key
for a scope's recipients, and `ljos sync` runs the pull and the push by hand. An
atom goes to the scope its `scope:NAME` entity names, else the machine's
`default_scope` in `~/.config/ljos/sync.toml`. A lesson from `finish`
takes the scope of the repository its issue lives in, and a
`[projects]` table in that repository's `sync.toml` sends one project's
lessons to another scope (`tools = "shared"`).

No issue yet? `vissue q -p PROJECT "TITLE"` mints one and prints its id.
Every piece of work has an issue before it has a claim.

## During the work

- Every artefact the work produces is a deed, then a citation:
  `deedar create file --name NAME --path PATH --agent NAME` prints an
  accession; `ljos deed ISSUE --add ACCESSION` cites it on the issue.
  Citing a deed names it; the bytes stay in deedar.
- A lesson is one `ljos remember` of two short sentences at most. It is
  stored as an episode and is not a refresher until `ljos graded ID`
  recalls it, or until consolidation finds it replaced an earlier claim.
  `ljos remember --standing` writes a rule now. A standing choice between
  two ways is one `ljos prefer`. Never a transcript, never a summary of the session. A
  correction from the person ("you should have", "do you not remember")
  is a preference the pack does not hold: write it with `ljos prefer`
  before the work it corrects, not after. A
  lesson that rewrites an earlier one closes the earlier one's window; the
  verb says `revises N earlier memories` when it did. `ljos consolidate`
  reports the pairs the rule would close across what is held, and
  `--apply` closes them; run it after a handover is imported.
  `ljos conflicts` lists the likeliest contradictions by distance rather
  than by words, when the `landscape` habitat is installed.
- Every number the seat keeps measuring is a habit: `ljos habit NAME VALUE
  [--unit U] [--every 7d] [--source JOB]` takes a reading, closes the one
  before it (kept as what it was), and puts the next reading on the review
  clock one cadence on, so `ljos due` and the hook say when it is late.
  `ljos habit` lists the habits as they stand with the change since the
  last reading; `ljos search --as-of` answers what one stood at then. A
  benchmark score, a latency, a count of open tickets: readings, not
  lessons.
- Every decision with more than one defensible answer is a ballot:
  `ljos vote ISSUE --for OPTION --confidence P --used DEED` once per
  identity (`VISSUE_AGENT`). `P` in `(0, 1]` is the probability the voter
  gives that its choice is the outcome (DeGroot 1974,
  doi:10.1080/01621459.1974.10480137). Omit it and the ballot is not a
  forecast. `--used` is the deeds the ballot drew on, or `none`
  (Buneman, Khanna and Tan 2001, doi:10.1007/3-540-44503-X_20). The
  line it prints is a count, not the settle. When an outcome is named,
  a stated probability is scored by the quadratic score `(p - o)^2`
  (Brier 1950; Gneiting and Raftery 2007,
  doi:10.1198/016214506000001437). The logarithmic score is `-ln` of the
  probability put on what happened (Good 1952,
  doi:10.1111/j.2517-6161.1952.tb00104.x); it is unbounded when that
  probability is 0. Across a voter's forecasts, mean probability against
  the event rate is calibration in the large (Dawid 1982,
  doi:10.1080/01621459.1982.10477856). From the second forecast, Murphy's
  partition splits the Brier score into reliability, resolution, and
  uncertainty (1973,
  doi:10.1175/1520-0450(1973)012<0595:ANVPOT>2.0.CO;2). None of these
  scores is a trust weight. Then
  `ljos consensus ISSUE`. The first lines are the reading. Polarization is
  how far voters still sit from the mean after listening, disagreement how
  far neighbors still sit from each other. Both zero with one option means
  there was one option. Act on the shares when polarization is about zero
  and two or more options were named. When polarization is away from zero,
  the mean is not a position the group reached. A tally printed later is
  who voted. On a hard question the ballot carries the private forecast
  of the others, `ljos vote ISSUE --for OPTION --expect OPTION`; `ljos predict`
  still records one on its own. With two or more forecasts
  the settle also names the surprisingly popular answer, the option whose
  actual share most exceeds its forecast, and shows each voter's standing.
  When the world says which option was right, `ljos finish ISSUE
  --outcome OPTION` (or `ljos learn`) writes every voter's record of
  outcomes as its weight, so the next settle weighs a voter by what it
  got right.
- When the work has shown that a kind of command must never run, or must
  be asked about first, write the law: `ljos rule 'PATTERN' --verdict
  deny|ask --why "..."`. The hook stops or asks at the point of action and
  `ljos policy` says the same; the rule is memory and travels in handovers.
- When the work wants readers with views of their own, such as a reviewer
  for a broad audience beside a domain expert, write each once:
  `ljos persona NAME --anchor A --view "..." --about DOMAIN...`, and
  `ljos personas` prints the roster the pack holds. Then
  `ljos vote ISSUE --for OPTION --as NAME` casts as it. The anchor in
  `[0, 1]` is how far it moves off its ballot in the settle; 0 never moves.
  A trust row scoped with `--about DOMAIN` applies when the issue's title
  carries that word; `learn` writes its rows scoped to what the issue's
  island is about, so being wrong on one topic costs nothing elsewhere.
  A panel is one subagent per persona, each started from
  `ljos brief NAME ISSUE` (the view, the bound playbook's full recipe,
  the five named principles, the arena rubric, what the seat knows on its
  domains, the working set), each casting one ballot as itself, then
  `ljos consensus`; over MCP the `run_a_panel` prompt orders it, and
  without MCP `ljos panel ISSUE --out DIR` writes one brief per persona.
  A panel is refused until a playbook is bound (`ljos playbook ISSUE NAME`
  or `ljos sitting ISSUE --playbook NAME`). Both seat the personas whose
  `--about` domains the issue's title or tags name. A persona the
  issue's island reaches sits only when its own view also uses a word of
  the title. A persona with no domains sits when no domain matches; with
  only specialists and no match, up to five whose views use two or more
  of the title's words sit. A panel with nobody to seat says so and
  stops: write the voters it needs (`--about` takes one domain per flag,
  many after it, or a comma list). A panel that seats everyone is a
  count. Model names on a
  playbook are optional spawn hints; every member still ends with
  `ljos vote --as` then `ljos consensus`. One playbook step per
  subagent; no resume across phases. Each brief is a file to start a
  subagent from. A panel
  member's own lesson goes in with `ljos remember --as NAME "..."` and
  comes back to it first in its next brief; the seat still reads it. The kind of work sets the dynamics: tag
  the issue `broad` when the panel is a broad audience, and the settle runs
  bounded confidence, so clusters are allowed and reported instead of being
  averaged into one position.
  Writing a persona also writes one unscoped inbound trust row (the seat
  weighs it at 1, everywhere); `--about` on a later trust row only adds
  weight.
- Jev, the fast judge, answers through the vote verbs. `ljos vote ISSUE --as
  NAME --jev` asks it for that persona's ballot and casts it when it is
  sure (confidence 0.8 or more); when it is not, the ballot goes to the
  persona's own session if the persona has a runner, else the command
  names the brief to start a subagent from. `ljos panel ISSUE --jev` asks
  for every seat at once and casts only when all are sure and agree.
  `ljos due --judge` puts the due page to the review judges. Which judges
  answer is `~/.config/ljos/jev.toml` (`[judges.NAME]`, `[route]`); with
  no file the verbs say so and the subagent path answers.
- A persona with a runner, `ljos persona NAME --runner RUNNER` where
  RUNNER is a `[[harness]]` with a `resume` argv, reasons in a session it
  keeps in a pane: it casts its own ballot and its memories are its own.
  `ljos ask NAME "..."` puts a question to it in that session.
- Work found while sitting is filed with `ljos file "TITLE" -t bug`: a
  child of the held issue, in its project, committed. `--parent ID`
  names another parent and `--top` none. A board this tracker projects
  from elsewhere takes the issue into its inbox under the printed id,
  so a child can name it at once.
- At a usage limit the hook holds the turn once. Spend it on the record and
  nothing else: `ljos note` the held issue with what is done and left,
  `ljos file` each item left, `ljos remember` each lesson, then stop.
- Progress goes on the issue, dated: `ljos note ISSUE "..."`; a longer
  report is `vissue append ISSUE "..."` (or `--file F`); a tag is
  `vissue update ISSUE -t TAG`.

### A bump, and a build campaign

A toolchain or version bump with eb-stack is one sitting on the ticket
and one island per recipe. Before a recipe is touched, `ljos island
"<name> <version> <toolchain>"` (the MCP `ljos_island` with that cue):
what the last bump of it taught, the patch it needed, the step it failed
in. Then the ladder in order, each rung its own claim with its own
artifact: `eb_recipe_check`, `eb_package_bump` (the lock under
`out/locks` is `resolves`), `eb_recipe_lint`, `eb_target_doctor`,
`eb_campaign_run` and `eb_campaign_status` (`builds`,
`binary-verified`). Say a rung only when its artifact exists.

A generation bump is many modules under one ticket, and the tracker's
graph is how a herd shares them: `ljos bump-plan out --project P
--parent TICKET` (over MCP, the tool `bump_plan`) puts every module the
bundle's lock builds on the tracker as a child issue, blocked by the
modules built before it along the SBOM's edges, with the same ids on
every run. `vissue ready -p P` is then the buildable frontier, each seat
sits on one module, and a sitting on a module whose blockers are open
is refused. Resolve each finding through eb-stack's `campaign finding
resolve` with the action and the files it changed, so the lesson below
carries the fix.

Every typed finding the campaign records is a lesson once somebody
resolved it: `ljos findings out/campaign.json --remember --issue ISSUE`
(the MCP `ljos_findings`) writes one lesson per resolved finding under
the recipe's name, the package and the failure class, and cites the
state file on the issue. A finding a later attempt merely got past is
not a lesson; `--all` takes those too. A lesson the seat writes by hand
names the recipe, the step, the error line and the fix: "GCCcore-15.2.0
on terra: compile failed in the build step with linux/scc.h missing.
Fix: the GCC 14 libsanitizer kernel headers patch." Not "verify the
lock exists before proceeding": the next seat cannot act on that.

- A subagent works under its parent's sitting and opens none of its
  own. What it finds joins the parent's issue. A judgement between
  options is `ljos vote ISSUE --for OPTION --as NAME`. A lesson still
  true next time is `ljos remember "..." --as NAME`, and a finding is
  `ljos note ISSUE "..."`. NAME is its persona, else its subagent
  type. On a runner that fires subagent events, the hook names the
  parent's issue on the subagent's first tool result. It keeps the
  subagent working once at its stop while that issue is open.

- Tracker and sync commits queue under `ljos-commit.lock` in the git
  directory, and a commit waits out another git process's `index.lock`.
  Never wrap a verb in a lock of your own, and never stash, reset or
  check out files another seat is editing to get a commit through.

## After the work

One verb closes the sitting:

    ljos finish ISSUE --status done --lesson "..." [--outcome OPTION] [--close]

It remembers the lesson, fires the island, completes the session node, and
learns from the outcome when one is named. Without `--lesson` it says so;
a sitting that taught nothing worth two sentences is rare. By hand, the
same four steps are:

1. `ljos remember "..."` when the sitting taught a lesson.
2. `ljos island TASK --fire` when the island served: the strongest memories
   fire together and their links gain weight.
3. `ljos complete ISSUE --status done` (`failed`, `cancelled`). Completing
   the session node does not close the ticket: `vissue update ISSUE -s DONE`
   does, when the work is accepted.
   Nor does `finish`: `--close` on it does, for the same acceptance.
4. `ljos learn ISSUE --outcome OPTION` when the world says which option was
   right. Every voter it refuted shrinks in every other voter's row, and a
   persona it refuted holds its next ballot less firmly.

When the tracker is a git checkout, `sitting` after its claim and `finish`
at the end commit the ticket's `issues.org` (that file alone) and push it,
and print a `tracker git:` line. A claim or a closure that stays in one
working tree does not exist for any other host. `LJOS_TRACKER_GIT=commit`
keeps it local; `=off` skips it. A refused push is reported, not raised:
push the tracker yourself before you leave. `ljos doctor` names how many
commits origin lacks, and fails the tracker row when that count sits
through the push wait; a leftover refused-push log is named on the row.

`ljos handover --out DIR --issue ISSUE [--to user@host:path]` is a separate
verb for when another seat takes over. The receiver runs `ljos receive DIR`,
then `--import`.

## When nobody names an outcome

Most issues close without anyone saying which option was right, and then
`learn` never runs and every voter keeps the same weight. `ljos calibrate
--project PROJECT` reads every issue of the project with two or more
ballots and estimates each voter's accuracy from how often it agrees with
the answer the other voters make likely (Dawid and Skene), then writes
those accuracies back as trust rows. Run it once per project after a few
issues have been voted on, and again when many more have. A consensus
under equal weights is a count; under calibrated rows it is not.

## Refusals worth knowing

- `claim: assignee busy HEX`: that name still holds that node.
  `ljos release HEX --assignee NAME` hands it back, `ljos complete HEX`
  finishes it. Occupancy is per issue, so a second ticket does not take
  this path.
- `already held by NAME; the sitting resumes`: not a refusal. A second
  `sitting` on the issue you hold renews the lease and goes on. Held by
  another seat, the claim names that actor and the two verbs that free it.
- `complete: status not terminal`: the statuses are `done`, `failed`,
  `cancelled`. To stop without finishing, `release`.
- `panel: no playbook bound`: personas cannot enter until a recipe is
  named. `ljos playbook ISSUE NAME` or `ljos sitting ISSUE --playbook NAME`.
- `playbook: ISSUE is bound to NAME until finish or release`: mid-sitting
  turns re-read that note. A new task is a new sitting.
- A claim on an issue whose earlier sitting finished reopens its session
  node and takes it: a new sitting on old work, with the ledger kept.
- `not a deed accession`: `--why` on `forget` and `trust` takes accessions
  from `deedar`, never free text.
- `the pack writer did not answer`: the pack is down, not empty.
  `packset ensure`.
- `no tracker ... relative root` or `root is not a directory` in `doctor`:
  the tracker root is private to your working directory (often a
  `VISSUE_ROOT` that kept a literal `~`). Anything filed there is invisible
  to every other seat. Fix the root before filing.
- `no tracker ... N unpushed` in `doctor`: the tracker checkout holds
  commits origin does not. Closures on this host are invisible everywhere
  else. Push the tracker. A leftover `tracker-push-*.log` names the last
  refusal when the push was refused. The row also fails when another remote
  of the tracker holds a different head of the branch, as of the last fetch;
  seats that push to different remotes never see each other's claims.
- `deedar: warning: this deed is signed by ed25519:...`: the host key is
  not a signer the store's `layout` lists, and `evidence` will refuse the
  deed. Add the printed `signer =` line to that file.
- A refused `remember` names the sentence, its word count and the limit:
  split it where it says.
- `ljos due` prints `0 due; nothing scheduled`: the seat has remembered
  nothing, and the review loop has nothing to run on. Remember something.
  `0 due; N scheduled, next at T` is a clock that is running.
- `ljos policy ARGV` prints the line a command would run under argv law,
  then what the pack knows that bears on it. It is not a check.
- `ljos hook` is the memory hook: a runner or a policy layer pipes the
  action about to happen (its hook JSON, or the plain argv) and gets back
  the memories that action activates, what two of the pack's scorers
  agreed on, preferences first, then lessons
  oldest to newest, each with its age (`[lesson, 3 weeks ago]`), so a
  later lesson reads as a revision of an earlier one. When the session
  ends, the memories it injected fire together, so what served one sitting
  is wired for the next. `ljos onboard`
  installs it on the runner's tool-call and prompt events, so the seat's
  memory reaches the agent at the point of action without being asked.

## Identity and environment

Nothing here needs a variable set. The pack is found on `127.0.0.1:8761`
and the seat's memory is one workspace, `seat`, whatever directory you
stand in (`PACKSET_WORKSPACE` names another). The deed store and claim
graph live in the user's state directories, the tracker at the root
`vissue identity` prints, and the host key at `~/.config/deedar/host.key`
when it exists. The seat is the program that connected: `ljos-mcp` names
it after the client that initialised it, and a shell the same runner opens
finds the same name through the process tree, so a runner's tools and its
command-line verbs claim and vote as one. Two names come from that: the
seat (`acme-cli`), which memory, ballots and trust rows accrue to across
every conversation of that runner, and the holder (`acme-cli-39u`),
which this conversation's claims are held under; any `*_SESSION_ID` the
runner stamped is the holder ahead of the process tag, and occupancy is
`{holder}:{issue}`, so two conversations of one runner hold two tickets and
a second sitting does not release the first. `ljos seat` prints both names
and where they came from. `LJOS_SEAT` names the seat; a `*_SESSION_ID` still
names the holder. `VISSUE_AGENT` is
the tracker's own name for the same thing; `--as` names a persona over
both; a person at a terminal is their login user.
