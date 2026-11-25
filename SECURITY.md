# Security policy

Report suspected vulnerabilities in Nacre privately through GitHub's private vulnerability reporting: on this repository's **Security** tab, choose **Report a vulnerability**. Please do not open a public issue, discussion or pull request for a vulnerability.

Include the package versions (`pacman -Q nacre nacre-niri`), the compositor, steps to reproduce and the expected impact. Fixes land on `main` and ship in the next package release; only the latest release is supported.

This covers Nacre and [nacre-niri](https://github.com/nacre-desktop/nacre-niri). A problem in [Bead](https://github.com/nacre-desktop/bead) or [Orient](https://github.com/nacre-desktop/orient) can be reported on that repository's Security tab instead.

By design, the optional AI sidebar runs Codex with full access to your user account; see [optional apps](docs/optional-apps.md). Report it if it can do more than the documentation says.
