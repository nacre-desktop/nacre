# Optional AI and Brain

```sh
./install.sh --with ai --with brain --apply
```

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
