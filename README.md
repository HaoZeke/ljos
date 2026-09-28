# ljos

What does this seat do next? One seat over five habitats and argv law. Each habitat keeps its own crate.

```
cargo binstall ljos packset
ljos remember "The lexical default is BM25+. It beat BM25 by two points on turns."
07be8829... lesson due 2026-09-26T23:27:31.528Z
ljos search lexical default
1.0000 3/3 lesson 07be8829... today The lexical default is BM25+. It beat BM25 by two points on turns.
```

That installs `ljos` and `ljos-mcp`. `ljos remember` starts the writer when none is answering. `cargo install ljos packset` is the same without `cargo-binstall`.

Citing a deed names it; the bytes stay in deedar. Neither a finish nor completing a node closes the ticket; `finish --close` does, when the work is accepted. Cards are read-only. Consensus is a [different crate](https://github.com/leidarljos/consensus).

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
ljos onboard --harness hermes                  # register with a runner named in harnesses.toml; opencode, hermes, omp, grok ship as shapes
ljos findings out/campaign.json --remember     # an eb-stack campaign's typed findings, one lesson each under the module that failed
```

## The loop, in two verbs

`ljos sitting ISSUE [--playbook NAME]` opens a sitting in the protocol's order and stops at the first store that does not answer: doctor, cards, the review clock, the island the issue's title activates, the playbook copied before recall, the working set, the timeline, the claim. Absent `--playbook`, a name already bound, else a closed-set token in the title, else `sit`. The playbook name is a tracker note until finish or release. `ljos panel` refuses until one is bound. `ljos finish ISSUE --lesson "..." [--outcome OPTION]` closes it: the lesson is remembered, the island fires, the session node completes, the ticket closes, and a named outcome shrinks the voters it refuted. The loop that makes the seat a memory runs every time, not only when somebody remembers it.

When the tracker is a git checkout, the claim and the finish each commit the ticket's `issues.org` (that file only) and push it, so another host sees the claim and the closure. `LJOS_TRACKER_GIT=commit` keeps it local; `=off` skips it. `ljos doctor` names how many commits origin lacks and fails the tracker row when that count sits through the push wait.

## Who is sitting

The seat is the program that connected. `ljos-mcp` names it after the client that initialised it, and `ljos` in a shell that runner opened finds the same name through the process tree or a shared session id, so a runner's tools and its verbs are one seat with nothing set. Claims are held per conversation; memory, ballots and trust accrue to the seat. `ljos seat` prints both names and where they came from. `ljos onboard` prints the one MCP entry any runner takes; `ljos protocol` prints the text an agent reads first.

Nothing needs a variable set: `ljos remember` starts the writer when none is answering, the seat's memory is the one workspace `seat` from any directory, and every claim carries the seat that wrote it.

## The smoke test

`scripts/smoke.sh` runs every loop on scratch stores: memory, agreement, learning, a sitting and its finish, a rewrite that closes its earlier claim, habits, an as-of read, a signed handover received and then refused after one byte is altered, a rule through the hook and through `policy`, then two seats (distinct `LJOS_SEAT` and `*_SESSION_ID` each) handing one ticket in sequence (A sits, cites a deed, hands over; B imports; A releases; B sits, both vote, consensus, finish closes, tracker git reports the commit), then `scripts/herd.sh` (eight sittings, one contended ticket, eight finishes, one island fire). `scripts/terra/` holds the Slurm scripts that build, smoke and herd the seat on the build host, as run.

## MCP

`ljos-mcp` serves the same verbs over stdio: thirty-nine tools, each opening with when to call it, the protocol at `ljos://protocol`, the cards read-only at `ljos://cards/`, and three prompts. `ljos_due` returns the soonest eight claims and the total still due. Paste this where the runner keeps its servers:

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
| [Policy](https://leidarljos.github.io/docs/policy/) | `ljos-policyd` is the TCB |
| [Getting started](https://leidarljos.github.io/ljos/getting-started.html) | Memory, agreement, and a sitting on scratch stores |
| [How-to](https://leidarljos.github.io/ljos/howto.html) | Hook, onboard, habits, handover, trust |
| [Reference](https://leidarljos.github.io/ljos/reference.html) | Every verb, tool and variable |
| [Explanation](https://leidarljos.github.io/ljos/explanation.html) | The contracts, the memory model, time as data, agreement that learns |
