# Optional AI and Brain

Core Nacre works without AI or Brain. Install either or both matching binary
packages, then select them in setup:

```sh
sudo pacman -U nacre-ai-*.pkg.tar.zst nacre-brain-*.pkg.tar.zst
nacre setup --with ai --with brain
```

For one app, install its archive and pass only its `--with` option. Source
installs use `./install.sh --with ai --with brain --apply` after reviewing the
plan. See [installation](installation.md) for prerequisites and package availability.

## AI

AI connects an existing Codex or Claude installation and its accounts to
conversations and project tasks. Install and sign into your chosen provider
separately, then use AI Settings to select it. Open the sidebar with Super+A or
from the launcher. Installing Nacre's code does not sign in, copy authentication,
download models or interrupt existing conversations. Codex defaults to write
access in the task workspace and configured Brain vault; Claude uses its own
permission prompts. `"aiFullAccess": true` is a profile opt-in for broader access.

## Brain

Brain browses and searches a local Markdown vault. Open it with Super+B or from
the launcher; Super+N captures a note. The default vault is
`~/Documents/Nacre-Brain`. Set `NACRE_BRAIN` to select an existing vault consistently
across Brain and AI tools. `NACRE_BRAIN_DIR` overrides the Brain server's path only.
Installation does not replace your vault
or download a semantic model; optional local model setup is a separate choice.
`NACRE_BRAIN_PORT` selects a different server port when needed.

Setup explains missing optional components. Accounts, conversations and notes
stay separate from application code and are preserved by managed code updates.
Use [recovery and troubleshooting](recovery.md) for reports and logs.
