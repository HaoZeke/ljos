---
description: Open a sitting on one issue
argument-hint: ISSUE
allowed-tools: Bash(ljos:*)
---

Open a sitting on the issue named in the arguments.

Arguments: $ARGUMENTS

Run `ljos sitting` with those arguments and follow the sections it prints: doctor, cards, due, island, playbook, recall, timeline, and the claim. Stop at the first store that does not answer. Do not invent an issue id. When no issue is named, ask for one.
