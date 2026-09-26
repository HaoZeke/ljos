Wire the seat into an agent runner
==================================

Describe the runners on the machine once, in
``~/.config/ljos/harnesses.toml``. A runner registers MCP servers either
through a command of its own or through a config file it reads; the file
holds one table per runner, in one of those two shapes, plus the directory
it loads skills from. ``ljos onboard --example`` prints the shape:

.. code:: toml

   [[harness]]
   name = "runner-with-a-command"
   register = ["runner", "mcp", "add", "-s", "user", "ljos", "--", "{server}"]
   registered = ["runner", "mcp", "get", "ljos"]
   skills = "~/.runner/skills"

   [[harness]]
   name = "runner-with-a-config-file"
   config = "~/.other/config.toml"
   marker = "[mcp_servers.ljos]"
   snippet = "\n[mcp_servers.ljos]\ncommand = \"{server}\"\nargs = []\n"
   skills = "~/.other/skills"

Then one verb per runner:

.. code:: console

   $ ljos onboard --harness runner-with-a-command
   ok  runner-with-a-command mcp   ran runner mcp add -s user ljos -- /home/you/.cargo/bin/ljos-mcp
   ok  skill   wrote /home/you/.runner/skills/ljos/SKILL.md
   $ ljos doctor | grep runner
   ok  runner mcp  runner-with-a-command: ljos registered
   ok  runner skill    runner-with-a-command: /home/you/.runner/skills/ljos/SKILL.md

``--dry-run`` reports what would be written. For a runner not in the file,
``ljos onboard --harness json`` prints the server entry to paste:

.. code:: json

   {"mcpServers": {"ljos": {"type": "stdio", "command": "/home/you/.cargo/bin/ljos-mcp", "args": [], "env": {}}}}

The skill is the protocol ``ljos protocol`` prints, under a front matter the
runner reads. The server serves the same text at ``ljos://protocol`` and
names it in its instructions, so a runner that loads neither skills nor
resources still reads it first. Twenty-three tools, fourteen of them
writers, two prompts (``start_a_sitting``, ``check_a_handover``), three
read-only resources. Each tool description opens with when to call it.

Inject memory at the point of action
====================================

Add ``hooks`` to the runner's table and onboard again:

.. code:: toml

   [[harness]]
   name = "runner-with-a-command"
   register = ["runner", "mcp", "add", "-s", "user", "ljos", "--", "{server}"]
   registered = ["runner", "mcp", "get", "ljos"]
   skills = "~/.runner/skills"
   hooks = "~/.runner/settings.json"

.. code:: console

   $ ljos onboard --harness runner-with-a-command
   ok  host key    /home/you/.config/deedar/host.key exists
   ok  runner-with-a-command mcp   ljos registered
   ok  hook    memory hook: add it on UserPromptSubmit in /home/you/.runner/settings.json
   ok  skill   /home/you/.runner/skills/ljos/SKILL.md is current

From then on, before each prompt, the runner pipes it to ``ljos hook`` and
the memories it activates come back as context, preferences first.
Do not add ``PreToolUse`` as a search event. A turn issues many tool
calls and one prompt. On a tool call the hook applies the TCB
(``ljos-policyd``) and then pack rules. A deny blocks.

.. code:: console

   $ echo '{"hook_event_name":"PreToolUse","tool_input":{"command":"git push --force"}}' | ljos hook
   {"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"git-force-push (seat rule `ljos-policyd`)"}}

**Grok Build.** ``ljos onboard --harness grok`` writes
``~/.grok/hooks/ljos.json``. The hook searches on the prompt, holds the
text, and emits it on ``PostToolUse``. The page :doc:`Grok Build <grok-build>`
is the install.

A
policy daemon does the same with the argv:

.. code:: console

   $ echo "cargo test --release" | ljos hook
   - [preference] Build and test on the cluster through Slurm, never on the laptop.
   - [lesson] The lme dense run needs 40G and sessions only; windows and sessions ran past four hours.
   $ ljos policy cargo test --release
   cargo test --release
   What this seat already knows that bears on this ...

Give an agent the protocol without a runner
===========================================

.. code:: console

   $ ljos protocol > instructions.md

The text is plain markdown: which store answers which question, the order
of verbs before, during and after the work, and the refusals worth knowing.
Put it where the agent reads its instructions.

Ask what a task involves
========================

.. code:: text

   $ ljos island "write the handbook tutorial for the seat"
   1.000  seed  ab8cb4d1bfca05ca60730141343dc177  3 weeks ago  Encoding under the write lock capped the pack at
   0.899  seed  51778929d855529fcb8df3f4d1c4d692  yesterday    A new claim enters the review clock on write. Tr
   0.890  seed  27e39df0198b67538f8487b1b44e9dfd  today        The decay slot reads the review clock when PACKS

The first column is activation relative to the strongest; ``seed`` marks a
claim search found itself, the rest were reached along the pack's links;
the age column says when each was written. Add ``--fire`` when you go on to
use the island: the strongest eight fire together and their links gain
weight.

Ask when
========

.. code:: console

   $ ljos timeline proj-1a2b
   timeline of proj-1a2b: Ship the fuse change?
   2026-08-30  13 days ago     tracker created
   2026-09-02 09:14    10 days ago +3 d    memory  [lesson] RRF lost two points to CombMNZ on turns.
   2026-09-02 11:48    10 days ago same day    tracker TODO -> STARTED
   2026-09-02 11:48    10 days ago same day    tracker claimed by you
   2026-09-05 16:02    7 days ago  +3 d    deed    deed-run-fuse-at-ten produced by you
   2026-09-12 08:30    today   +7 d    tracker note: the fused panel held at ten conversations

Three stores, one dated list, oldest first: the tracker's logbook, the
deeds the issue cites with the time each was produced, and the memories the
issue's title activates with the time each was written. Each line carries
its date, its age, the gap since the line before, and the store it came
from. The reader gets time as data rather than stamps to subtract, and a
later line supersedes an earlier one on the same matter. ``ljos sitting``
prints the last twelve as its ``timeline`` section.

``ljos search TOPIC --as-of 2026-09-01`` asks the pack as it stood then:
memories withdrawn since included, memories written since left out. The
pack tombstones rather than erases, which is what makes the question
answerable.

Start a sitting, and end one
============================

.. code:: console

   $ ljos sitting proj-1a2b
   == doctor
   ok  vissue  ...
   ok  seat    alice (from LJOS_SEAT)
   == cards
   == due
   unreviewed  conclusion  3f9c... The lexical default is BM25+.
   1 due; 4 scheduled, next at 2026-09-13T09:12:00Z
   == island: Ship the fuse change?
   1.000   seed    ab8c... 3 weeks ago CombMNZ over RRF for fusing two ballots.
   == playbook
   sit
   A sitting on one issue. Name this recipe at open (`ljos sitting ISSUE --playbook sit` or `ljos playbook ISSUE sit`). The sitting prints this body before recall and holds the name until finish or release.
   ...
   == recall
   ...
   == timeline
   timeline of proj-1a2b: Ship the fuse change?
   2026-08-30  13 days ago     tracker created
   ...
   == claim
   gen=2

One verb, in the protocol's order; it stops at the first store that does
not answer and claims nothing. ``--playbook NAME`` copies a recipe
(``sit``, ``arena``, ``land``, ``company-panel``, ``overnight``) into the
playbook section before recall; absent a name, a closed-set token in the
title else ``sit``. The name is a tracker ``playbook:`` note until finish
or release. ``ljos panel`` refuses until one is bound. The ``start_a_sitting``
prompt gives the same order to a runner that prefers single tools. A claim
refused as busy names the issue you still hold; ``ljos complete`` finishes it
and ``ljos release`` hands it back.

.. code:: console

   $ ljos finish proj-1a2b --lesson "CombMNZ held on turns. RRF lost two points." --outcome ship
   remembered 51778929d855529fcb8df3f4d1c4d692
   fired the island for "Ship the fuse change?": 8 memories
   completed the session node for proj-1a2b as done
   learned from outcome "ship": 6 trust rows rewritten
   the ticket stays proj-1a2b's state; `vissue update proj-1a2b -s DONE` closes it

Vote with personas
==================

.. code:: console

   $ ljos persona reviewer --anchor 0.2 --view "Reads for what breaks in production." --about release
   $ ljos persona reader --anchor 0.8 --view "Reads as a first-time user of the docs." --about docs
   $ ljos vote $id --for hold --as reviewer
   $ ljos vote $id --for ship --as reader
   $ ljos consensus $id

Each persona is one atom in the pack; its ballots carry its name. The
settle takes its anchor: the reviewer at 0.2 barely moves off ``hold``, the
reader at 0.8 is nearly a plain voter. A trust row scoped with ``--about
docs`` weighs only on issues whose title says ``docs``.

Move the trust rows without an outcome
======================================

.. code:: console

   $ ljos calibrate -p demo
   alice weighs bob at 0.917
   alice weighs carol at 0.643
   ...

Every issue of the project with two or more ballots is an item; Dawid and
Skene's estimate gives each voter an accuracy from how often it agrees with
the answer the others make likely, and each accuracy becomes the weight
every other voter gives that voter. Run it once a project has a few voted
issues, and again when it has many more.

Keep the cards
==============

Cards are two files a person writes: ``USER.md`` and ``MEMORY.md``. The seat
reads them and never writes them; ``ljos cards`` prints them. The pack is
where the seat writes.

Sign what you hand over
=======================

.. code:: console

   $ head -c 32 /dev/urandom > ~/.config/deedar/host.key && chmod 600 ~/.config/deedar/host.key
   $ export DEEDAR_HOST_SIGNING_KEY=~/.config/deedar/host.key
   $ ljos doctor | grep 'host key'
   ok  host key    /home/you/.config/deedar/host.key (32-byte seed)

``handover`` then signs the manifest and the log head. Two seats share
one ticket by this walk, in sequence: A sits and hands over, B imports
and sits after A releases, both vote, consensus, finish closes.
``scripts/smoke.sh`` runs it on scratch stores; ``scripts/herd.sh`` is
the concurrent contention check. A receiver adds
``signer = ed25519:<hex>`` to their deed store's ``layout`` and ``receive``
reports ``(accepted)``.

Receive from the same sender again
==================================

.. code:: console

   $ ljos receive /tmp/bag2 --since /tmp/bag/head.txt

The bridge shows the sender's log grew from the head you kept and was not
rewritten.

Close what a later lesson replaced
==================================

.. code:: console

   $ ljos consolidate
   closes 3f9c…  The default fuse is Borda.
       for 51ee…  The default fuse is CombMNZ.
   1 of 97 live memories would close; `ljos consolidate --apply` closes them
   $ ljos consolidate --apply
   closes 3f9c…  The default fuse is Borda.
       for 51ee…  The default fuse is CombMNZ.
   1 of 97 live memories closed

A write closes the earlier claim it rewrites on arrival (same opening
words, a new object; a correction; an explicit supersedes). A pack written
before that rule, or filled by a handover, holds pairs the rule never saw;
``consolidate`` reports them, ``--apply`` closes them. The closed claim keeps
its window: ``ljos search --as-of`` finds it at the time it was live.

Retire a claim with its reason
==============================

.. code:: console

   $ ljos forget 3f9c... --why deed-review-2026-09

The pack tombstones the claim and records the deed that withdrew it.
Disagreeing with a claim is not showing it wrong; a retraction names its
evidence.

Set trust by hand
=================

.. code:: console

   $ ljos trust alice carol 0.9 --why deed-postmortem-2026-09

One row, one weight in (0, 1], the deeds it rests on. A later row for the
same pair supersedes it.

Settle under the pack's trust
=============================

``ljos consensus ID`` reads the live trust rows and passes them to both
settles. To anchor voters to their own ballots (Friedkin-Johnsen) on the
model crate alone, pass the rows as JSON tuples:

.. code:: console

   $ ljos-consensus settle --issue ID --susceptibility 0.8 --trust '[["alice","carol",0.9]]'

Check the seat
==============

.. code:: text

   $ ljos doctor
   ok  vissue  ~/.local/bin/vissue
   ok  deedar  ~/.local/bin/deedar
   ok  claimdag  ~/.local/bin/claimdag
   ok  packset  ~/.local/bin/packset
   ok  packsetd  ~/.local/bin/packsetd
   ok  ljos-consensus  ~/.local/bin/ljos-consensus
   ok  ljos-mcp  ~/.local/bin/ljos-mcp
   ok  pack  http://127.0.0.1:8761 workspace git:github.com/leidarljos/ljos
   ok  host key  ~/.config/deedar/host.key (32-byte seed)
   ok  deed store  size=34 root=3d8e015509923724097e9f33d3a044fe0764f17bd5387769530ed5bfb6ada
   ok  tracker  vissue 0.16.2 root=/home/me/vault prefix=Software from VISSUE_ROOT=/home/me/vault
   ok  claim graph  a4a8fa1b8f05d259877be54da99f06bc  claimed  task  69f91712  gen=2  ljos-a6

Exit 1 when the tracker, the deed store, or the pack does not answer. The
tracker row names the root vissue resolved and where it came from, and fails
when that root is relative, missing, or holds no prefix directory: a ticket
filed there is invisible to every other seat. It also names how many commits
the checkout holds that origin does not, and fails when that count sits
through the push wait; a leftover refused-push log is named on the row. It
also fails when another remote of the tracker holds a different head of the
branch, as of the last fetch: two seats pushing to two remotes each read
only their own writes. A missing claim graph is reported and is not a failure: the first claim
creates it.
