# Contributing to Nacre

Nacre is GPL-3.0-licensed, and contributions are accepted under the same [license](LICENSE).

## Where things go

- Bugs and feature requests: open an issue here with the template. Questions go to [Discussions](https://github.com/nacre-desktop/nacre/discussions).
- Compositor behavior on nacre-niri (collections, edge cues, overview, lock): this repository's issues too; code changes go to [nacre-niri](https://github.com/nacre-desktop/nacre-niri).
- Framework code (rendering, input, Luau UI runtime): [Bead](https://github.com/nacre-desktop/bead). Palette generation: [Orient](https://github.com/nacre-desktop/orient).
- Vulnerabilities: privately, as described in [SECURITY.md](SECURITY.md).

## Build and test

The desktop job in [`.github/workflows/check.yml`](.github/workflows/check.yml) is the complete check; [docs/development/ci.md](docs/development/ci.md) explains it, and the [product-shell build guide](docs/development/product-shell-build.md) covers building and previewing the shell. Run focused checks while you work, then the workflow's steps in its Arch container before you open a pull request.

Never run setup or the installer against your real home directory while testing; use a throwaway `HOME` and `XDG_*` directories.

## Rules

- Make the smallest change that fixes the cause. Behavior changes come with tests.
- Rust is rustfmt-formatted and passes Clippy with warnings denied.
- Committed files contain no personal paths, usernames or machine details.

## Specs

Work units and contracts are specs in [`docs/specs/`](docs/specs/README.md). A change to a contract updates its spec in the same pull request.
