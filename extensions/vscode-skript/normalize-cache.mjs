import { readFileSync, writeFileSync } from "fs";

// Projects the SkriptHub `/api/v1/addonsyntaxlist/` dump onto the corpus
// consumed by skript-registry (`include_bytes!("../testdata/skripthub.json")`).
//
// Usage:
//   bun run download-cache
//
// The output file is git-TRACKED: a run over a changed live API produces a
// reviewable diff. Commit data refreshes deliberately, not casually.

const raw = JSON.parse(readFileSync(0, "utf-8"));

// Fields dropped by the projection. Keep in sync with
// crates/skript-registry/src/model.rs (serde ignores unknown fields, but
// stripping them keeps `download-cache` diffs minimal and reviewable).
const DROP = new Set([
  "creator",
  "get_syntax_type_css_class",
  "created_at",
  "updated_at",
]);

const normalized = raw.map((entry) => {
  const out = {};
  for (const [k, v] of Object.entries(entry)) {
    if (DROP.has(k)) continue;
    out[k] = v;
  }
  // `entries` arrives as a JSON-encoded string; parse it (or null it out).
  let entries = out.entries;
  if (typeof entries === "string") {
    try {
      entries = JSON.parse(entries);
    } catch {
      entries = null;
    }
  }
  out.entries = entries || null;
  return out;
});

const out = "../../crates/skript-registry/testdata/skripthub.json";
writeFileSync(out, JSON.stringify(normalized), "utf-8");
console.log(
  `Normalized ${normalized.length} entries -> ${out} (git-tracked corpus; commit refreshes deliberately)`,
);
