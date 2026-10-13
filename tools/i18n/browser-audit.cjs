const { chromium } = require(
  process.env.NACRE_PLAYWRIGHT_MODULE || "playwright",
);
const fs = require("fs"),
  path = require("path"),
  assert = require("assert");
const root = path.resolve(__dirname, "../.."),
  out = process.argv[2] || "/tmp/nacre-translation-audit";
fs.mkdirSync(out, { recursive: true });
const colors = JSON.parse(
  fs.readFileSync(
    path.join(root, "nacre/shell-tools/tests/orient-gallery/palettes.json"),
  ),
)["night-city"]["dark-natural"].colours;
const graph = {
  nodes: [
    {
      id: "demo",
      kind: "hub",
      label: "Demo subject",
      category: "software",
      count: 1,
      activity: 1,
    },
    { id: "topic", kind: "topic", label: "Demo topic", count: 1, activity: 1 },
    {
      id: "note",
      kind: "note",
      label: "Demo note",
      path: "demo.md",
      date: "2026-10-12",
      recorded: "2026-10-12T12:00:00Z",
      confidence: "verified",
      preview: "Demo evidence body",
      memberships: [{ subject: "demo", reason: "manual grouping" }],
    },
  ],
  links: [
    { source: "demo", target: "topic", kind: "group" },
    { source: "topic", target: "note", kind: "group" },
  ],
  noteCount: 1,
  generated: "2026-10-12T12:00:00Z",
  theme: colors,
};
(async () => {
  const browser = await chromium.launch({ headless: true });
  const report = [];
  for (const language of ["en", "pseudo"]) {
    const page = await browser.newPage({
      viewport: { width: 1280, height: 900 },
    });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.route("http://nacre.test/**", async (route) => {
      const url = new URL(route.request().url());
      if (url.pathname.startsWith("/api/")) {
        let json =
          url.pathname === "/api/brain"
            ? graph
            : url.pathname === "/api/search"
              ? { ids: ["note"] }
              : url.pathname === "/api/note"
                ? {
                    text: "# Demo note\n\nDemo evidence body.\n\n- Public test data\n",
                  }
                : {};
        return route.fulfill({ json });
      }
      const name = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
      if (
        ![
          "index.html",
          "app.js",
          "style.css",
          "i18n.js",
          "i18n-data.js",
          "translation-engine.js",
        ].includes(name)
      )
        return route.fulfill({ status: 404, body: "" });
      let body = fs.readFileSync(path.join(root, "brain/web", name), "utf8");
      if (name === "index.html")
        body = body.replace(
          '<html lang="en"',
          '<html lang="en" data-nacre-language="' + language + '"',
        );
      return route.fulfill({
        body,
        contentType: name.endsWith(".html")
          ? "text/html"
          : name.endsWith(".css")
            ? "text/css"
            : "text/javascript",
      });
    });
    await page.goto("http://nacre.test/");
    await page.waitForFunction(() => ready);
    assert.equal(
      await page.evaluate(() =>
        document.documentElement.style.getPropertyValue("--text"),
      ),
      "#" + colors.onSurface,
    );
    async function capture(name) {
      await page.screenshot({
        path: path.join(out, language + "-browser-" + name + ".png"),
      });
      const rows = await page.evaluate(() => {
        const rows = [];
        const walk = document.createTreeWalker(
          document.body,
          NodeFilter.SHOW_TEXT,
        );
        while (walk.nextNode()) {
          const node = walk.currentNode,
            el = node.parentElement,
            text = node.nodeValue.trim();
          if (
            !text ||
            el.closest("script,style") ||
            el.closest("[hidden]") ||
            !el.getClientRects().length
          )
            continue;
          const style = getComputedStyle(el);
          rows.push({
            text,
            tag: el.tagName,
            truncated:
              style.textOverflow === "ellipsis" &&
              el.scrollWidth > el.clientWidth + 1,
          });
        }
        return rows;
      });
      report.push({ language, name, rows });
    }
    await capture("overview");
    for (const view of ["map", "notes"]) {
      await page.evaluate((view) => {
        state.view = view;
        render();
      }, view);
      await capture(view);
    }
    await page.evaluate(() => select("demo"));
    await capture("subject");
    await page.evaluate(() => {
      state.view = "read";
      state.selected = "note";
      render();
    });
    await page.waitForTimeout(150);
    await capture("reader");
    const groupingLabel = await page.evaluate(() =>
      window.NacreI18n.tr("brain.ui.label.your-grouping", "Your grouping"),
    );
    assert(await page.getByText(groupingLabel, { exact: true }).isVisible());
    await page
      .getByRole("button", {
        name: language === "en" ? "Change grouping" : /Çħáñğë ğŕöüþïñğ/,
      })
      .click();
    await capture("grouping");
    await page.setViewportSize({ width: 720, height: 900 });
    await page.evaluate(() => home());
    await capture("narrow");
    await page.locator("#search").fill("Demo");
    await page.waitForTimeout(500);
    await capture("search");
    assert.deepEqual(errors, []);
    await page.close();
  }
  fs.writeFileSync(
    path.join(out, "browser.json"),
    JSON.stringify(report, null, 2),
  );
  const external = new Set([
    "Demo subject",
    "Demo topic",
    "Demo note",
    "Demo evidence body.",
    "Public test data",
    "demo.md",
  ]);
  for (const row of report.filter((r) => r.language === "pseudo")) {
    assert.deepEqual(
      row.rows.filter((v) => v.truncated && v.text.startsWith("[")),
      [],
    );
    assert.deepEqual(
      row.rows.filter(
        (v) =>
          !v.text.startsWith("[") &&
          /[A-Za-z]{2}/.test(v.text) &&
          !external.has(v.text),
      ),
      [],
    );
  }
  console.log(
    "Browser translation audit passed: eight public demo states at desktop/narrow widths per language. Screenshots/report: " +
      out,
  );
  await browser.close();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
