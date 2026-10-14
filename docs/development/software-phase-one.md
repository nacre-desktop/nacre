# Software capability follow-up

## Developer environments

Software → Developer environments exposes install and remove actions for Node.js,
Bun, Deno, Python, Go, Rust, Java, .NET, Zig, Elixir, PHP and Ruby. Phoenix,
Laravel and Rails have separate framework actions. All entries also participate
in ordinary global launcher search.

The actions open Kitty and the native `software-environments run` command. The
terminal displays each argument array and requires `yes` before execution.
Non-interactive execution is refused. Mise installation uses the existing
visible package-manager action with a confirmation; it does not run while
browsing. No Docker group membership or system service is changed automatically.

Mise's global configuration owns runtime selections. Removal uses
[`mise unuse --global`](https://mise.jdx.dev/cli/unuse.html), retaining versions
needed by tracked projects. Removing a framework retains its runtime. Elixir
and Erlang use a compatible OTP pair, and PHP uses the precompiled
[`static-php-builds` backend](https://github.com/nunomaduro/static-php-builds)
without overwriting the user's PHP alias. Discovery does not run framework
commands and cannot install a missing runtime.

Docker database actions require a running, accessible daemon. PostgreSQL 18,
MySQL 8.4 and Redis 8 bind only to localhost, with passwords requested separately
and passed through the environment rather than displayed command arguments.
PostgreSQL uses the [18+ volume layout](https://docs.docker.com/guides/postgresql/immediate-setup-and-data-persistence/).
Removal checks Nacre's ownership label and targets the inspected immutable
container ID; database volumes remain. A foreign container with the same name
is preserved. These are development instances, not production database setup.

Validation uses private homes and mock external commands. A real PTY verifies
the visible command review, cancellation and confirmation boundaries. No real
language installation, database creation or privileged action is needed for
these checks.

## AI apps

Apps has an AI category and a searchable AI subpage. Ollama uses Arch's package,
including detection of installed CUDA/ROCm/Vulkan variants; installed Ollama opens
its local model listing in a visible terminal. LM Studio uses `lmstudio-bin` and
Claude uses `claude-desktop` from AUR, with clear repository labels.

ChatGPT uses the vendor desktop package when it is present in the configured
package database with an OpenAI/ChatGPT project URL. The similarly named
`chatgpt-bin` community CLI is not used as a desktop app. Nacre does not silently
add repositories or run a vendor installer it cannot inspect. The project link
opens the [official Linux installation guide](https://learn.chatgpt.com/docs/linux/linux-app).
When an applicable package/manager is unavailable, ChatGPT and Claude offer a
managed Chromium web app. Existing managed web apps remain searchable and
removable through their existing registry. This does not provide the vendor's
native desktop integrations.

## Validation boundary

Source validation exercises the launcher and native action plans in nested test
sessions. It does not require installing the catalog's software, restarting host
services or changing the active desktop. CI-only commits for this follow-up use
`[no-publish]` to keep automatic Arch package publication outside this phase.
