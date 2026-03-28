#!/usr/bin/env node
// scripts/bump-version.mjs
//
// Increments the patch segment of the project version across all three
// version files.  package.json is the single source of truth — the other
// two files are updated to match it.
//
//   0.7.3  →  0.7.4
//
// Usage:  node scripts/bump-version.mjs
// Stdout: the new version string (e.g. "0.7.4") — nothing else.

import { readFileSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// ── 1. Read current version from package.json ─────────────────────────────
const pkgPath = join(root, "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
const [major, minor, patch] = pkg.version.split(".").map(Number);
const newVersion = `${major}.${minor}.${patch + 1}`;

// ── 2. package.json ───────────────────────────────────────────────────────
pkg.version = newVersion;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

// ── 3. src-tauri/tauri.conf.json ──────────────────────────────────────────
const tauriPath = join(root, "src-tauri", "tauri.conf.json");
const tauriConf = JSON.parse(readFileSync(tauriPath, "utf8"));
tauriConf.version = newVersion;
writeFileSync(tauriPath, JSON.stringify(tauriConf, null, 2) + "\n");

// ── 4. src-tauri/Cargo.toml ───────────────────────────────────────────────
// Replaces only the first `version = "x.y.z"` line (the [package] entry).
// Dependency version lines are indented or prefixed and will not match.
const cargoPath = join(root, "src-tauri", "Cargo.toml");
const cargo = readFileSync(cargoPath, "utf8");
const updated = cargo.replace(
  /^version = "\d+\.\d+\.\d+"/m,
  `version = "${newVersion}"`
);
if (updated === cargo) {
  process.stderr.write(`bump-version: could not find version line in Cargo.toml\n`);
  process.exit(1);
}
writeFileSync(cargoPath, updated);

// ── Output new version (consumed by release.bat via FOR /F) ───────────────
process.stdout.write(newVersion);
