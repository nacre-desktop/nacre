# Orient scene samples

`orient-scenes.json` contains bounded color-analysis results produced by our Orient
extractor from four Nick Nazzaro illustrations: desert, jungle-red, space-blue and
underwater. Source image URLs and attribution are recorded in each fixture. The
images themselves are not bundled in this repository. Their publisher identifies
them as **Creative Commons Attribution-ShareAlike 4.0 International**, by Nick
Nazzaro, commissioned by System76. Source revision:
`pop-os/wallpapers@20a9fdd1ed86aadfbbfcd55dc3d2d9eb8ae28e15`.

[Publisher and licensing](https://github.com/pop-os/wallpapers#nick-nazzaro) ·
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).
The extracted sample records are retained under CC BY-SA 4.0 with attribution.

These fixture records test generation/selection from real sampled pigment families;
they do not substitute for testing image decoding/extraction. The synthetic images
in test_orient.py exercise extraction, coverage, neutral fallback and cache behavior.
For a visual review, download the four originals to a private temporary folder,
analyze them using the pinned engine, and render body/primary/secondary/tertiary
roles in both modes. Do not add the user's wallpaper collection to Git.
