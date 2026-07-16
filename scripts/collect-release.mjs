/**
 * Copy the built installer(s) and portable executable into a top-level `dist/` folder
 * so you don't have to dig through target/release/bundle/.
 *
 * Run via: npm run release  (builds first) or npm run collect-release (copy only)
 */
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { basename, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const releaseDir = join(root, "src-tauri", "target", "release");
const bundleDir = join(releaseDir, "bundle");
const distDir = join(root, "dist");

const exeCandidates = ["HelixGT.exe", "helixgt.exe"];
const bundleSubdirs = ["nsis", "msi", "deb", "appimage", "dmg", "macos"];

function listFilesRecursive(dir, acc = []) {
  if (!existsSync(dir)) return acc;
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) listFilesRecursive(full, acc);
    else acc.push(full);
  }
  return acc;
}

function formatBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

if (!existsSync(releaseDir)) {
  console.error(`No release build found at ${relative(root, releaseDir)}.`);
  console.error("Run: npm run tauri build");
  process.exit(1);
}

if (existsSync(distDir)) {
  rmSync(distDir, { recursive: true, force: true });
}
mkdirSync(distDir, { recursive: true });

const copied = [];

for (const name of exeCandidates) {
  const src = join(releaseDir, name);
  if (existsSync(src)) {
    const dest = join(distDir, name);
    cpSync(src, dest);
    copied.push({ kind: "portable executable", path: dest, bytes: statSync(dest).size });
    break;
  }
}

for (const sub of bundleSubdirs) {
  const dir = join(bundleDir, sub);
  if (!existsSync(dir)) continue;
  for (const file of listFilesRecursive(dir)) {
    const lower = file.toLowerCase();
    if (
      !(
        lower.endsWith(".exe") ||
        lower.endsWith(".msi") ||
        lower.endsWith(".dmg") ||
        lower.endsWith(".deb") ||
        lower.endsWith(".appimage") ||
        lower.endsWith(".rpm")
      )
    ) {
      continue;
    }
    const dest = join(distDir, basename(file));
    cpSync(file, dest);
    copied.push({ kind: `installer (${sub})`, path: dest, bytes: statSync(dest).size });
  }
}

if (copied.length === 0) {
  console.error("Build finished but no installer/executable was found to copy.");
  console.error(`Looked in: ${relative(root, releaseDir)} and ${relative(root, bundleDir)}`);
  process.exit(1);
}

const lines = [
  "HelixGT release artifacts",
  "========================",
  "",
  "This folder is created by `npm run release` so installers are easy to find.",
  "You can share any of these files; you do not need the full project tree.",
  "",
  ...copied.map(
    (item) =>
      `- ${basename(item.path)}  (${item.kind}, ${formatBytes(item.bytes)})`,
  ),
  "",
  "Tips:",
  "- Prefer the NSIS setup .exe or MSI for end users (installs Start Menu entry).",
  "- HelixGT.exe is a portable run-in-place binary (still needs the same machine architecture).",
  "",
];
writeFileSync(join(distDir, "README.txt"), lines.join("\n"), "utf8");

console.log("");
console.log("Release files are ready in:");
console.log(`  ${distDir}`);
console.log("");
for (const item of copied) {
  console.log(`  • ${basename(item.path)}  — ${item.kind} (${formatBytes(item.bytes)})`);
}
console.log("");
