# Optional AI and Brain

```sh
./install.sh --with ai --with brain --apply
```

The full AI workflow installer (`python3 ai/install.py`) prints a plan and makes
no changes by default. Review every link, guidance update and setup command, then
rerun it with `--apply` to install. It refuses private AI worktrees and linked Git
worktrees unless `--allow-worktree` is supplied after review.

To update the owner's pinned private conversation runtime at
`~/.local/share/nacre/ai/conversation-runtime/<stamp>`, use the controller-only
path. First run `python3 ai/install.py --launcher-only` and review the reported
source and target. Apply that reviewed update with
`python3 ai/install.py --launcher-only --apply`. For one controller, use
`--helper-only nacre-ai-chat` or `--helper-only nacre-ai-claude` in both steps.
These code-only plans preserve a backup of replaced private files and do not
reinstall AI accounts or credentials.

AI connects your existing Codex/Claude installation and accounts to conversations
and project tasks. Provider installation/sign-in is a separate user choice. The
code-only component installer preserves existing accounts and running workers;
it never copies somebody else's authentication. General profiles use workspace
write access; full access is an explicit `aiFullAccess:true` profile preference.

Brain displays/searches a local Markdown vault. The default path is
`~/Documents/Nacre-Brain`; `NACRE_BRAIN_DIR`/`NACRE_BRAIN` can select an existing
vault. Installing the UI does not initialize a replacement vault or install a
semantic model. Its optional local model setup is a separate choice. A separate
instance can select `NACRE_BRAIN_PORT` rather than collide with an existing server.

Both applications can be reopened from the launcher. Optional setup pages explain
missing components instead of making the base desktop depend on them. Your
accounts and memory remain yours when you update or remove application code.

Codex accounts share local chats. Switching the selected account changes the
login used to continue a saved chat, while preserving its title, identity and
workspace. Credentials stay in separate account homes. New account homes point
Codex's supported `sqlite_home` setting at the default chat store; existing
profile histories remain discoverable in place, without copying databases or
credentials. A chat in an older private store resumes with that store and the
selected login in a separate Codex process. Running remote workers retain their
original account until they exit. Claude Code histories remain account-specific.

The standalone native `nacre-ai-chat` and `nacre-ai-claude` helpers provide title,
thread and session metadata operations. Claude history and sidebar text restore
read local session files directly; this path does not require
`claude_agent_sdk` or the venv from `ai/install-claude.py`. The retained Python
rollback sidebar still uses that optional dependency. Upstream history formats
are fixture-tested; unsupported or malformed history is left untouched.
