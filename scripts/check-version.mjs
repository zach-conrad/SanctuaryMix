// Fails unless package.json, src-tauri/tauri.conf.json, Cargo.toml and
// (optionally) the release tag all carry the same version.
//
// Usage: node scripts/check-version.mjs [v1.2.3]
import { readFileSync } from "node:fs";

const tag = process.argv[2];
const versions = {
  "package.json": JSON.parse(readFileSync("package.json", "utf8")).version,
  "src-tauri/tauri.conf.json": JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version,
  "Cargo.toml [workspace.package]": readFileSync("Cargo.toml", "utf8").match(
    /\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/,
  )?.[1],
};
if (tag) versions[`tag ${tag}`] = tag.replace(/^v/, "");

const distinct = new Set(Object.values(versions));
for (const [where, v] of Object.entries(versions)) console.log(`${where}: ${v}`);
if (distinct.size !== 1 || distinct.has(undefined)) {
  console.error("Version mismatch. Bump all of them together before tagging.");
  process.exit(1);
}
console.log(`All versions agree: ${[...distinct][0]}`);
