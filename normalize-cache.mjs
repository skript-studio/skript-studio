import { readFileSync, writeFileSync } from "fs";

const raw = JSON.parse(readFileSync(0, "utf-8"));

const normalized = raw.map((entry) => {
  const { creator, ...rest } = entry;
  let entries = rest.entries;
  if (typeof entries === "string") {
    try {
      entries = JSON.parse(entries);
    } catch {
      entries = null;
    }
  }
  return { ...rest, entries: entries || null };
});

const out = "../../crates/skript-lsp/data/skripthub-cache.json";
writeFileSync(out, JSON.stringify(normalized), "utf-8");
console.log(`Normalized ${normalized.length} entries -> ${out}`);
