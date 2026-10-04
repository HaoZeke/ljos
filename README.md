# ljos

Memory, a work tracker and a shell-command gate for coding agents: Claude Code,
Codex, Grok Build, Antigravity's `agy`, opencode, omp and hermes.

An agent forgets what it learned when the session ends, and it runs whatever
command it decides on. ljos keeps the lessons in a local store and hands the
ones that bear on a prompt back to the agent before it answers. It records the
work on a tracker the agent and the person both read. It checks each shell
command against rules the person wrote before it runs. It works through each
runner's MCP server list and its hooks. Nothing leaves the machine unless you
turn on a remote judge.

## Five minutes

```
cargo binstall ljos packset vissue-cli      # or: cargo install ljos packset vissue-cli
ljos onboard --harness claude               # MCP server, hooks and skill for Claude Code
ljos prefer "Tag a release with git push origin TAG; --follow-tags leaves v-tags behind."
ljos rule '*--force*' --verdict deny --why "Never force push."
```

`--harness` takes `claude`, `codex`, `grok`, `antigravity`, `opencode`, `omp`
or `hermes`. Start a new session and ask for something the preference bears
on, such as tagging a release. Before the model reads the prompt, the hook
adds:

```
What this seat already knows that bears on this (from the pack, each with its age, lessons oldest first; `ljos search` for more):
- [preference, today] Tag a release with git push origin TAG; --follow-tags leaves v-tags behind.
```

A preference stands from the moment it is written. A lesson (`ljos remember`)
comes back once a review or a consolidation promotes it; `ljos due` lists the
ones waiting.

## Claude Code plugin

The MCP server, the seat protocol skill, the sitting and finish commands, and
the hooks `ljos onboard --harness claude` registers are one plugin. The
marketplace file is in this repository.

```
claude plugin marketplace add HaoZeke/ljos
claude plugin install ljos@ljos
```

`ljos` and `ljos-mcp` stay on `PATH` (`cargo binstall ljos`, or
`cargo install ljos`). The plugin runs those binaries; it also looks in
`~/.cargo/bin` and `~/.local/bin`. Start a new session. `/ljos:sitting ISSUE`
opens a sitting and `/ljos:finish ISSUE --lesson "..."` closes it.
`claude plugin validate .` checks the manifest in a checkout.

When the agent then reaches for `git push --force origin main`, the runner
refuses the command before it runs:

```
{"permissionDecision":"deny","permissionDecisionReason":"Never force push. (seat rule `*--force*`)"}
```

## What you get

- Memory per prompt: one claim per memory, searched with the prompt and
  injected only when it shares the prompt's words or a judge is sure it bears
  on it. Lessons come back on a review clock, and a wrong one is superseded,
  not edited.
- A tracker, [vissue](https://github.com/leidarljos/vissue): plain org files
  in a git repository. `ljos sitting ISSUE` opens the work and claims it;
  `ljos finish ISSUE --lesson "..."` closes the loop and records what it
  taught.
- A gate on shell commands: rules in the pack (`ljos rule`), tried on each
  command a line runs, and a guard that keeps agents from rewriting the
  gate's own binaries and hook files. Pushes are free to your own unreleased
  repositories and cite a decision elsewhere.
- Decisions with more than one defensible answer go to personas, voters with
  a view of their own, and settle by trust-weighted
  [consensus](https://github.com/leidarljos/consensus) that learns from the
  outcome. A persona can reason in a runner session it keeps.
- Deeds ([deedar](https://github.com/leidarljos/deedar)): content-addressed
  records of what a piece of work produced, cited on the ticket.

## Words

| word | meaning |
|---|---|
| seat | one agent runner on one machine, with the memory and claims it holds |
| pack | the memory store ([packset](https://github.com/leidarljos/packset)), one claim per atom |
| sitting | one piece of work on one ticket, opened by `ljos sitting` and closed by `ljos finish` |
| island | the memories a title activates, and the links between them |
| deed | a signed record of what a piece of work produced |
| card | a file the person froze; agents read it and never write it |
| persona | a voter with a view, an anchor and the domains it speaks to |

## Commands

Citing a deed names it; the bytes stay in deedar. Neither a finish nor completing a node closes the ticket; `finish --close` does, when the work is accepted.

```
ljos sitting vissue-xxxx --playbook sit        # doctor, cards, due, island, playbook, recall, timeline, claim
ljos finish vissue-xxxx --lesson "..." [--outcome ship]   # remember, fire, complete, learn
ljos calibrate -p project                      # trust rows from the voting history, no truth labels
ljos remember "the default fuse is CombMNZ"
ljos prefer "CombMNZ over RRF"
ljos search fuse
ljos island "rebuild the packset site"
ljos evidence deed-…
ljos deed vissue-xxxx --add deed-…
ljos recall vissue-xxxx
ljos claim <node>                              # the session node, and the tracker issue to STARTED under the same name
ljos release <node>
ljos seat                                      # who is sitting: the seat, this conversation's holder, and where the names came from
ljos complete <node> --status done
ljos cards
ljos policy -- ls
ljos approve <request-id>                       # record explicit consent for one stopped command
ljos consensus vissue-xxxx
ljos trust alice bob 0.8 --why deed-… [--about docs]
ljos persona reviewer --anchor 0.2 --view "Reads for what breaks in production." --about release
ljos playbooks                                 # sit, arena, land, company-panel, overnight
ljos playbook vissue-xxxx company-panel        # bind a recipe; sitting copies the body before recall; panel refuses until then
ljos vote vissue-xxxx --for hold --expect ship --confidence 0.6 --used none
ljos predict vissue-xxxx --expect ship          # the same forecast on its own; two forecasts and consensus names the surprisingly popular answer
ljos rule '*--force*' --verdict deny --why "Never force push."   # argv law in the pack; the hook and policy enforce it
ljos brief reviewer vissue-xxxx                # what a subagent playing reviewer starts from
ljos remember --as reviewer "..."              # a lesson the persona keeps; its next brief opens with it
ljos panel vissue-xxxx --out panel             # every persona's brief as a file, for a runner without MCP
ljos vote vissue-xxxx --for hold --as reviewer
ljos panel vissue-xxxx --jev                   # with Jev on: cast when every seat is sure and agrees, else briefs for subagents
ljos learn vissue-xxxx --outcome ship
ljos due
ljos hud                                       # execs sibling ljos-hud (overlay; P pops out)
ljos habit mab-cr-all 0.579 --unit acc --every 7d --source 11793   # a reading; the one before closes and is kept as what it was
ljos habit                                     # every habit as it stands, the change since the last reading, when the next is due
ljos graded <atom-id> [--lapsed]
ljos handover --out bag --project x --issue vissue-xxxx
ljos receive bag [--since bridge.txt] [--import]
ljos doctor
ljos protocol
ljos onboard                                   # the one MCP server entry any runner takes
ljos onboard --harness hermes                  # register with a runner named in harnesses.toml; opencode, hermes, omp, grok, antigravity ship as shapes
ljos findings out/campaign.json --remember     # an eb-stack campaign's typed findings, one lesson each under the module that failed
```

## The loop, in two verbs

`ljos sitting ISSUE [--playbook NAME]` opens a sitting in the protocol's order and stops at the first store that does not answer: doctor, cards, the review clock, the island the issue's title activates, the playbook copied before recall, the working set, the timeline, the claim. Absent `--playbook`, a name already bound, else a closed-set token in the title, else `sit`. The playbook name is a tracker note until finish or release. `ljos panel` refuses until one is bound. `ljos finish ISSUE --lesson "..." [--outcome OPTION]` closes it: the lesson is remembered, the island fires, the session node completes, and a named outcome shrinks the voters it refuted. The ticket stays open unless `--close` is given, which closes it only when the work is accepted. The loop that makes the seat a memory runs every time, not only when somebody remembers it.

When the tracker is a git checkout, the claim and the finish each commit the ticket's `issues.org` (that file only) and push it, so another host sees the claim and the closure. `LJOS_TRACKER_GIT=commit` keeps it local; `off` skips it. `ljos doctor` names how many commits origin lacks and fails the tracker row when that count sits through the push wait.

## Jev, where a machine allows it

`~/.config/ljos/jev.toml` with `enabled = true` and a key command hands four of the seat's judgments to Jev, TypeSafe's decision model: which claims bear on a prompt, whether the prompt corrects the agent or puts a choice, a persona's ballot, and whether an agent may stop. The local answer stays as the fallback, and a machine without the file runs as before. A prompt costs about $0.00004 and `ljos doctor` prints the month's spend. The explanation's section on judgment says how it slots into what the seat already did; the how-to has the setup.

## Who is sitting

The seat is the program that connected. `ljos-mcp` names it after the client that initialised it, and `ljos` in a shell that runner opened finds the same name through the process tree or a shared session id, so a runner's tools and its verbs are one seat with nothing set. Claims are held per conversation; memory, ballots and trust accrue to the seat. `ljos seat` prints both names and where they came from. `ljos onboard` prints the one MCP entry any runner takes; `ljos protocol` prints the text an agent reads first.

Nothing needs a variable set: `ljos remember` starts the writer when none is answering, the seat's memory is the one workspace `seat` from any directory, and every claim carries the seat that wrote it.

## The smoke test

`scripts/smoke.sh` runs every loop on scratch stores: memory, agreement, learning, a sitting and its finish, a rewrite that closes its earlier claim, habits, an as-of read, a signed handover received and then refused after one byte is altered, a rule through the hook and through `policy`, then two seats (distinct `LJOS_SEAT` and `*_SESSION_ID` each) handing one ticket in sequence (A sits, cites a deed, hands over; B imports; A releases; B sits, both vote, consensus, finish closes, tracker git reports the commit), then `scripts/herd.sh` (eight sittings, one contended ticket, eight finishes, one island fire). `scripts/terra/` holds the Slurm scripts that build, smoke and herd the seat on the build host, as run.

## MCP

`ljos-mcp` serves the same verbs over stdio: forty-one tools, each opening with when to call it, the protocol at `ljos://protocol`, the cards read-only at `ljos://cards/`, and three prompts. `ljos_due` returns the soonest eight claims and the total still due. Paste this where the runner keeps its servers:

```json
{"mcpServers": {"ljos": {"command": "ljos-mcp"}}}
```

Unanimous green (two implementers, two reviewers, `ljos consensus` is GREEN) lives in [`examples/unanimous-green/`](examples/unanimous-green/).

## Documentation

The org site teaches sitting: <https://leidarljos.github.io>. The crate site is Org plus Sphinx: <https://leidarljos.github.io/ljos/>.

| Page | What it answers |
|---|---|
| [First write](https://leidarljos.github.io/docs/start/) | Install, remember one sentence, search it back |
| [Sit](https://leidarljos.github.io/docs/sit/) | `sitting` opens, `finish` closes |
| [Harnesses](https://leidarljos.github.io/docs/harnesses/) | One entry for any runner; the seat names itself |
| [Policy](https://leidarljos.github.io/docs/policy/) | Which shell commands are refused, by which layer, and how the person approves one |
| [Getting started](https://leidarljos.github.io/ljos/getting-started.html) | Memory, agreement, and a sitting on scratch stores |
| [How-to](https://leidarljos.github.io/ljos/howto.html) | Hook, onboard, habits, handover, trust |
| [Reference](https://leidarljos.github.io/ljos/reference.html) | Every verb, tool and variable |
| [Explanation](https://leidarljos.github.io/ljos/explanation.html) | The contracts, the memory model, time as data, agreement that learns |
