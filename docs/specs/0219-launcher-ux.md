# N0219 Launcher keyboard and search UX

Keep the results area at a fixed position while the file-search hint changes. Support keyboard movement among the category rail, content and quick controls, with a visible focus indicator and caret-aware Left/Right behavior in search. The bar logo and Super+Space open the same Home/Favorites state.

Acceptance: pixel coverage compares the first result row at `""`, `":"` and `":ab"`; interaction tests cover every zone transition, quick-control activation and the search caret rule; a state test compares bar-logo and Super+Space opening. Run the focused checks and the complete `nacre-shell` binary on dev-1.
