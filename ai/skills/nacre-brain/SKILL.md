---
name: nacre-brain
description: Retrieve or save durable personal knowledge, host/device facts, project context, and operational lessons in the user's private local Markdown vault.
---

# Nacre Brain

The vault lives at `$HOME/Documents/Nacre-Brain`, independent of ChatGPT
accounts. Use `nacre-brain path` if the user configured a different location.

Search relevant terms before reading entire folders. Prefer verified, recent
notes; inspect the cited source when the fact controls an operation. Host
addresses are dated observations, so verify machine identity before changing it.
Repo-local instructions and specs stay authoritative for code and deployment.

Automatically record useful new knowledge at verified milestones and before
finishing a task; do not wait for a user reminder. During long tasks, checkpoint
important findings as they become established. Search first, skip trivial turns
and duplicates, and mark corrections as superseding older evidence. Never dump
whole conversations or tool logs into the vault. Use the CLI:

```sh
nacre-brain note --kind runbooks --title 'Short factual title' \
  --source 'Exact local file, command evidence, or source URL' --confidence verified
```

Supply Markdown on stdin. Include what was observed, verification date, relevant
commands, practical limits, and next step where needed. Choose `reported` for
user statements and `unverified` for claims that have not been checked. Each call
creates a unique file, so concurrent chats can safely add notes. Organize existing
notes without rewriting other active tasks' files.

Never store secret values. Credential notes contain OS keyring references only.
The helper rejects common credential patterns but is not a complete detector;
review the text. Never sync private vault content or account state into the
public OS repo. Ordinary ChatGPT web memory is a separate system and does not
automatically read this folder.

## Maintenance and project overviews

At meaningful milestones, or when notes conflict, read
[maintenance](references/maintenance.md). Keep reconciliation scoped, preserve
historical evidence, and save only changed reusable conclusions. Use the shared
Obsidian skills for formatting and views; this skill governs what belongs in the
private brain. The app does not need to be open for file-based maintenance.

## Conversation-first context and memory

New chats are general conversations, not assigned projects. Resolve the relevant
repositories only when the user requests work, using `nacre-ai-context QUERY`
(or MCP `nacre_workflow.resolve_context`). Keep the same chat across subjects.
Resolve ambiguity from the conversation and registry before asking; do not guess
which repository to modify. Registry build_host/build_path remain authoritative.

The brain includes personal life, ideas, research, devices and skills as well as
projects. At meaningful milestones, search existing evidence first, then use
`nacre-ai-memory --title TITLE --source SOURCE --confidence reported|verified|unverified
--world SUBJECT --topic TOPIC` with a concise Markdown body on stdin. Repeat
--world to connect several subjects, and --topic for shared subtopics. Codex also
exposes find_knowledge and remember in nacre_workflow; Claude uses the same CLI.
Keep entity names stable; reuse canonical wiki names/aliases. New subjects create
missing wiki descriptors automatically; this does not register a coding project.
Preserve existing curated pages and reconcile them through expected-hash updates.

Save decisions, outcomes, useful research and explicit personal preferences;
do not save every utterance, duplicate unchanged facts, transient output or raw
transcripts. User statements are reported; tested outcomes are verified; ideas
and hypotheses remain unverified. Link sources. Correct older facts explicitly.
The visual brain grows from dated evidence with activity decay, not message counts.
Meaningful new topics can emerge within existing worlds; promote a distinct new
world when it warrants its own stable subject. New repositories belong in the
project registry only when actually created or discovered and verified.
