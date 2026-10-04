// Writes downloads/latest.json for the website, describing the installer CI
// just copied next to it. Contract: website/src/release.ts.
//
// Usage: node scripts/write-site-manifest.mjs <downloads-dir> <tag> <publishedAt>
import { statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [dir, tag, publishedAt] = process.argv.slice(2);
if (!dir || !tag) {
  console.error("Usage: write-site-manifest.mjs <downloads-dir> <tag> [publishedAt]");
  process.exit(1);
}

const file = "SanctuaryMix-mac-universal.dmg";
const manifest = {
  version: tag.replace(/^v/, ""),
  publishedAt: publishedAt || new Date().toISOString(),
  mac: { file, size: statSync(join(dir, file)).size },
};
writeFileSync(join(dir, "latest.json"), JSON.stringify(manifest, null, 2) + "\n");
console.log(manifest);
