import fs from "node:fs";
import path from "node:path";

export const hierarchyWorlds = [
  "World Alder",
  "World Birch",
  "World Cedar",
  "World Dogwood",
  "World Elm",
  "World Fir",
  "World Grove",
];

export function writeRegistry(vault, worlds = hierarchyWorlds) {
  const today = new Date().toISOString().slice(0, 10);
  const areas = [];
  for (const [worldIndex, world] of worlds.entries()) {
    const count = worldIndex < 2 ? 5 : 4;
    for (let i = 0; i < count; i += 1) {
      const name = `${world.split(" ")[1]} Area ${i + 1}`;
      areas.push({
        name: worldIndex === 0 && i === 2 ? "Proposed Grove" : name,
        parent: world,
        aliases: [],
        patterns: [],
        title_only: true,
        status:
          worldIndex === 0 && i === 0
            ? "auto"
            : worldIndex === 0 && i === 1
              ? "auto"
              : worldIndex === 0 && i === 2
                ? "proposed"
                : "seed",
        created: worldIndex === 0 && i === 0 ? today : "2026-10-01",
        updated: today,
        uses: [],
      });
    }
  }
  const registry = {
    version: 1,
    worlds: worlds.map((name, index) => ({
      name,
      aliases: [],
      patterns: [],
      status: "seed",
      created: "2026-10-01",
      updated: today,
    })),
    areas,
  };
  fs.writeFileSync(
    path.join(vault, "registry.json"),
    JSON.stringify(registry, null, 2),
  );
  return { worlds: registry.worlds, areas };
}

export function writeHierarchyNotes(
  vault,
  worlds = hierarchyWorlds,
  count = 2000,
) {
  const { areas } = writeRegistry(vault, worlds);
  const regular = path.join(vault, "notes");
  fs.mkdirSync(regular, { recursive: true, mode: 0o700 });
  const areaNames = areas.map((area) => area.name);
  for (let i = 0; i < count; i += 1) {
    const worldIndex = i % worlds.length;
    const world = worlds[worldIndex];
    const start = worldIndex < 2 ? worldIndex * 5 : 10 + (worldIndex - 2) * 4;
    const area =
      areaNames[
        start + (Math.floor(i / worlds.length) % (worldIndex < 2 ? 5 : 4))
      ];
    const review = i < 15 ? "\nReview: unsorted" : "";
    const relative =
      i === 0
        ? "notes/stable-move.md"
        : i % 100 === 0
          ? `archive/${String(i).padStart(4, "0")}.md`
          : i % 137 === 0
            ? `digests/${String(i).padStart(4, "0")}.md`
            : `notes/${String(i).padStart(4, "0")}.md`;
    const destination = path.join(vault, relative);
    fs.mkdirSync(path.dirname(destination), { recursive: true, mode: 0o700 });
    fs.writeFileSync(
      destination,
      `# ${world} fixture note ${i}\nRecorded: ${new Date().toISOString()}\nConfidence: verified\nWorlds: ${world}\nAreas: ${area}${review}\n\nSynthetic hierarchy fixture ${i}.\n`,
      { mode: 0o600 },
    );
  }
  const wiki = path.join(vault, "wiki", "alder-area-1.md");
  fs.mkdirSync(path.dirname(wiki), { recursive: true, mode: 0o700 });
  fs.writeFileSync(
    wiki,
    "# Alder Area 1\nEntity: area\nName: Alder Area 1\nParent: World Alder\n\nSynthetic wiki page.\n",
    { mode: 0o600 },
  );
  const cache = path.join(vault, ".brain-state", "index");
  fs.mkdirSync(cache, { recursive: true, mode: 0o700 });
  fs.writeFileSync(
    path.join(cache, "graph.json"),
    JSON.stringify({
      schema: 1,
      fingerprint: "stale-synthetic-fixture",
      graph: {
        nodes: [
          {
            id: "suggestion:synthetic",
            kind: "hub",
            label: "Synthetic suggested grouping",
          },
        ],
      },
    }),
    { mode: 0o600 },
  );
}

export function writeMinimalRegistry(vault, worlds) {
  const registry = {
    version: 1,
    worlds: worlds.map((name) => ({
      name,
      aliases: [],
      patterns: [],
      status: "seed",
      created: "2026-10-01",
      updated: new Date().toISOString().slice(0, 10),
    })),
    areas: [],
  };
  fs.writeFileSync(path.join(vault, "registry.json"), JSON.stringify(registry));
}
