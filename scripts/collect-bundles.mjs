// Copies the installers Tauri produced into one folder under fixed names, so
// download links like .../releases/latest/download/SanctuaryMix-mac-universal.dmg
// never change between versions.
//
// Usage: node scripts/collect-bundles.mjs <out-dir> <name-prefix>
import { copyFileSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { join } from "node:path";

const [outDir = "dist-release", prefix = "SanctuaryMix"] = process.argv.slice(2);

const sources = [
  { dir: "target/universal-apple-darwin/release/bundle/dmg", ext: ".dmg", name: "mac-universal.dmg" },
  { dir: "target/release/bundle/nsis", ext: "-setup.exe", name: "windows-x64-setup.exe" },
  { dir: "target/release/bundle/msi", ext: ".msi", name: "windows-x64.msi" },
];

mkdirSync(outDir, { recursive: true });
let copied = 0;
for (const { dir, ext, name } of sources) {
  if (!existsSync(dir)) continue;
  const files = readdirSync(dir).filter((f) => f.endsWith(ext));
  if (files.length !== 1) {
    console.error(`Expected exactly one ${ext} in ${dir}, found: ${files.join(", ") || "none"}`);
    process.exit(1);
  }
  const dest = join(outDir, `${prefix}-${name}`);
  copyFileSync(join(dir, files[0]), dest);
  console.log(`${join(dir, files[0])} -> ${dest}`);
  copied++;
}
if (copied === 0) {
  console.error("No installers found. Did the Tauri build run?");
  process.exit(1);
}
