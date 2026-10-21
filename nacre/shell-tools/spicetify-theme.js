// Nacre: reload palette CSS when Nacre rewrites it, so wallpaper changes apply without restarting Spotify.
(function nacreLivePalette() {
  let last = null;
  setInterval(async () => {
    try {
      const text = await (await fetch("colors.css?" + Date.now(), { cache: "no-store" })).text();
      if (last !== null && text !== last) {
        document.querySelectorAll("link.userCSS").forEach((link) => {
          link.href = link.getAttribute("href").split("?")[0] + "?" + Date.now();
        });
      }
      last = text;
    } catch (_) {}
  }, 2000);
})();
