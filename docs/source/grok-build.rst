The pack only helps a Grok session if it reaches the model.

Grok discards ``UserPromptSubmit`` stdout. It delivers ``PostToolUse``
``additionalContext`` after the first tool. ``ljos hook`` searches on the
prompt, holds the text, and emits it once on ``PostToolUse``.
A conversation that holds no issue is told, on that first result, to
file one and sit. ``Stop`` holds such a turn once when it used tools and
never touched the seat. ``PreToolUse`` is TCB and pack rules; a deny
blocks. There is no ``sync.sh``.

Install
=======

.. code:: console

   $ ljos onboard --harness grok

That writes ``~/.grok/hooks/ljos.json`` once, naming ``ljos`` by the absolute
path beside ``ljos-mcp``, so a Grok started outside a login shell still finds
it. Then
``/hooks`` then ``r`` **once**. Later ``cargo binstall ljos`` is live on the
next event. Name ``ljos-mcp`` on PATH in ``~/.grok/config.toml``.
``ljos onboard --harness grok`` bumps ``LJOS_MCP_GENERATION`` in that table
when the crate version moved, so Grok's config watcher respawns the
server; a session restart is not required.

Why these events
================

- ``UserPromptSubmit`` searches and holds the text. Grok discards the stdout.
- ``PostToolUse`` emits the held text once. With no issue held, the first result says to file and sit. Grok delivers that after the tool.
- ``PreToolUse`` applies the TCB and pack rules. A deny blocks. A turn has many tool calls.
- ``Stop`` holds one turn that used tools and never touched the seat, when the conversation holds no issue. Grok continues that turn once.
- ``SessionEnd`` fires the injected memories. Grok ignores the stdout.

The frozen file is `crates/ljos-cli/assets/grok/ljos.json <../../crates/ljos-cli/assets/grok/ljos.json>`__.
Its ``{ljos}`` is filled in at onboard. ``PreToolUse`` gets 10 seconds, the
TCB check's budget; the others get Grok's default 5.

Grok's stdin is camelCase (``hookEventName``, ``sessionId``, ``toolInput``), and
``ljos hook`` reads it as the snake_case fields. A deny blocks on a top-level
``decision`` of ``deny`` beside ``hookSpecificOutput``. An ask rule is the same
pair with ``ask``: Grok shows that as the in-chat permission prompt. A runner
that cannot ask (stdin carrying ``turn_id``) still gets the ask rewritten to
a deny.

Smoke
=====

.. code:: console

   $ echo '{"hook_event_name":"PreToolUse","tool_input":{"command":"echo"}}' | ljos hook
   $ echo '{"hookEventName":"pre_tool_use","toolInput":{"command":"git push --force"}}' | ljos hook
   {"decision":"deny","hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"git-force-push (seat rule `ljos-policyd`)"},"reason":"git-force-push (seat rule `ljos-policyd`)"}
   $ echo '{"hook_event_name":"PostToolUse","session_id":"s"}' | ljos hook

An allow prints nothing unless a prompt has already held pack text.
A TCB deny still blocks.

harnesses.toml
==============

.. code:: toml

   [[harness]]
   name = "grok"
   config = "~/.grok/config.toml"
   marker = "[mcp_servers.ljos]"
   skills = "~/.config/ljos/skills"
   hooks = "~/.grok/hooks/ljos.json"
   hook_events = ["UserPromptSubmit", "PostToolUse", "PreToolUse", "SessionEnd"]
