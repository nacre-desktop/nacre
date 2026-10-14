# Software library extensions

A Nacre extension repository contains `nacre.extension.json` at its root. The
Software → Extensions page imports a public HTTPS Git repository, validates its
manifest and assets, and leaves it disabled. Enable registers launcher actions;
panels only start when you explicitly open them. Imported code has the same
user-level file, process and network access as other applications. Separate panel
processes avoid sharing the main shell's object tree; they are not a filesystem
sandbox.

The initial supported kinds are `launcher`, `theme` and `panel`. A manifest can
combine kinds. Installation hooks, Git submodules and symbolic links are rejected.
Updates preserve edited checkouts and disable the new revision. Clone to edit
creates a disabled local copy under your own ID; `org.nacre.*` is reserved for
bundled content.

```json
{
  "schemaVersion": 1,
  "id": "example.project-tools",
  "name": "Project tools",
  "description": "Shortcuts for working on projects",
  "version": "1.0.0",
  "website": "https://example.org/project-tools",
  "kinds": ["launcher"],
  "actions": [
    {
      "id": "projects",
      "name": "Open Projects",
      "icon": "folder",
      "command": ["xdg-open", "{home}/Projects"]
    }
  ]
}
```

Commands are executable/argument arrays; each argument remains literal. `{home}`
is replaced with the current user's home directory. There is no implicit shell
expansion. Use explicit shell arguments only when you intend to run shell code.
Action IDs must be unique within a pack. Keep labels short and descriptions
useful; the launcher displays metadata as plain text.

A `panel` declares `"entryPoint": "shell.qml"`. That file is a complete standalone
Quickshell configuration. It is launched in its own managed user scope. Disable,
remove and update stop only a scope with the matching recorded ownership marker.
Panel packages can provide a relative `preview` image. Assets must remain inside
the extension folder. Whole replacement bars and dashboard widget injection are
not supported by this format.

A `theme` declares `theme.modes.dark` and `theme.modes.light`, each containing a
complete Orient palette. Export an existing theme from Software → Themes for a
working template rather than inventing role names. Both palettes are validated by
Orient, including text contrast. Add an extension's theme to Themes through its
item actions; applying a palette remains a separate action. Saved themes use
Nacre Colors' existing storage and lock.

Extension configuration is private at `$XDG_CONFIG_HOME/nacre/software-library.json`
(or `~/.config/nacre/software-library.json`). Managed checkouts are under
`$XDG_DATA_HOME/nacre/software/extensions` (or `~/.local/share/nacre/software/extensions`).
The launcher uses the additive `nacre-helper software-library` interface directly;
it does not expand the frozen helper-service request grammar.

App screenshot URLs are taken from the project's AppStream metadata distributed
by Flathub; screenshots are displayed from their published URLs, not bundled or
represented as Nacre screenshots. The package catalog remains the Arch/AUR
catalog. Missing previews have a project-website fallback.
