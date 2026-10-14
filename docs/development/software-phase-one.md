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

## Terminal apps

Software → Terminal apps offers Create a TUI app next to the web-app section.
The form saves a name, an icon theme name, one command argument per line and a
terminal choice (Kitty, Foot, Alacritty or WezTerm). Creation does not execute
commands. Existing entries are globally searchable and create desktop entries
for All apps. Missing terminals or command executables disable launch.

The `tui-apps` versioned state schema owns `state/nacre/menu/tui-apps.json`.
Guarded writes retain future versions and original-byte backups; corrupt or
foreign data is preserved. Generated launchers have a unique ownership marker.
Removal deletes only the selected registered launcher and leaves the terminal,
command program and application data installed. Commands use literal argv,
including arguments containing shell metacharacters; no shell is implied.

## Curated fonts

Style → Interface font includes JetBrains Mono, Fira Code, Cascadia Code,
Iosevka, Victor Mono and Meslo, plus packaged Nerd variants. Repository labels
and capability checks distinguish official packages from AUR. Missing fonts
open a confirmed, visible package-manager action. Installed families select
through the existing `desktop.interfaceFont` Settings descriptor and provider;
no competing font preference is introduced. Installed families keep their prior
stable IDs. Package names and Fontconfig family aliases are separate, and
ordinary search also finds the font offers. This selection changes the shell's
interface family; the planned terminal/monospace publisher is not implemented
by this item.

## Health fix-its

Health → Fix a problem exposes detected service and tool actions: PipeWire /
WirePlumber restart, NetworkManager / Bluetooth restart, fwupd updates, orphan
package removal and bounded package-cache cleanup. Every action requires a
launcher confirmation, then a visible terminal command review and `yes` before
execution. Administrator operations use pkexec or sudo only after that review.
Remapped desktop sessions hand privileged work to the normal user-session
terminal bridge; no namespace or sandbox policy is relaxed.

Orphan removal validates and displays the exact package list. Cache cleanup keeps
three versions. Firmware operations retain fwupd's device/power/signature and
confirmation checks. Touchpad restart is offered only for confirmed devices on
the supported Hyprland device-control path, using its current Lua API; it is not
advertised as available on Niri. A failed touchpad sequence attempts to re-enable
its owned targets. Missing hardware, services, tools or an administrator agent
omit their corresponding fix-it rows. Private test fixtures cover cancellation,
non-TTY rejection, root command dispatch and capability gates; they never restart
host services or update firmware.
