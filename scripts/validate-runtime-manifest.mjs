import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..");

function parseArgs(argv) {
  const args = {
    manifest: "src-tauri/runtime-manifests/vulkan-v0.8.4-hotfix.json",
    root: "src-tauri/bin/vulkan",
    label: "runtime",
    allowExtra: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--manifest" && argv[i + 1]) {
      args.manifest = argv[i + 1];
      i += 1;
      continue;
    }
    if (arg === "--root" && argv[i + 1]) {
      args.root = argv[i + 1];
      i += 1;
      continue;
    }
    if (arg === "--label" && argv[i + 1]) {
      args.label = argv[i + 1];
      i += 1;
      continue;
    }
    if (arg === "--allow-extra") {
      args.allowExtra = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      printUsage();
      process.exit(0);
    }
    console.error(`[validate-runtime-manifest] Unknown argument: ${arg}`);
    printUsage();
    process.exit(2);
  }

  return args;
}

function printUsage() {
  console.error(
    "Usage: node scripts/validate-runtime-manifest.mjs [--manifest <path>] [--root <path>] [--label <name>] [--allow-extra]",
  );
}

function resolveFromRepo(inputPath) {
  return path.isAbsolute(inputPath) ? inputPath : path.join(repoRoot, inputPath);
}

function sha256File(filePath) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
}

function assertSafeManifestName(name) {
  if (!name || typeof name !== "string") {
    throw new Error("Manifest file entry is missing a string name.");
  }
  if (name.includes("/") || name.includes("\\") || name === "." || name === "..") {
    throw new Error(`Manifest file entry '${name}' must be a plain file name.`);
  }
}

const args = parseArgs(process.argv.slice(2));
const manifestPath = resolveFromRepo(args.manifest);
const rootPath = resolveFromRepo(args.root);

if (!fs.existsSync(manifestPath)) {
  console.error(`[validate-runtime-manifest] Manifest not found: ${manifestPath}`);
  process.exit(1);
}
if (!fs.existsSync(rootPath) || !fs.statSync(rootPath).isDirectory()) {
  console.error(`[validate-runtime-manifest] Runtime root not found: ${rootPath}`);
  process.exit(1);
}

const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
if (!Array.isArray(manifest.files) || manifest.files.length === 0) {
  console.error(`[validate-runtime-manifest] Manifest has no files array: ${manifestPath}`);
  process.exit(1);
}

const expected = new Map();
for (const entry of manifest.files) {
  try {
    assertSafeManifestName(entry.name);
  } catch (error) {
    console.error(`[validate-runtime-manifest] ${error.message}`);
    process.exit(1);
  }
  expected.set(entry.name, {
    length: Number(entry.length),
    sha256: String(entry.sha256 || "").toLowerCase(),
  });
}

const actualFiles = fs
  .readdirSync(rootPath, { withFileTypes: true })
  .filter((entry) => entry.isFile())
  .map((entry) => entry.name)
  .sort((a, b) => a.localeCompare(b));

const expectedFiles = [...expected.keys()].sort((a, b) => a.localeCompare(b));
const failures = [];

for (const name of expectedFiles) {
  const filePath = path.join(rootPath, name);
  if (!fs.existsSync(filePath)) {
    failures.push(`missing: ${name}`);
    continue;
  }
  const stat = fs.statSync(filePath);
  const spec = expected.get(name);
  if (!stat.isFile()) {
    failures.push(`not a file: ${name}`);
    continue;
  }
  if (stat.size !== spec.length) {
    failures.push(`length mismatch: ${name} expected=${spec.length} actual=${stat.size}`);
  }
  const actualHash = sha256File(filePath);
  if (actualHash !== spec.sha256) {
    failures.push(`sha256 mismatch: ${name} expected=${spec.sha256} actual=${actualHash}`);
  }
}

if (!args.allowExtra) {
  const expectedSet = new Set(expectedFiles);
  for (const name of actualFiles) {
    if (!expectedSet.has(name)) {
      failures.push(`unexpected file: ${name}`);
    }
  }
}

if (failures.length > 0) {
  console.error(`[validate-runtime-manifest] ${args.label} validation failed.`);
  console.error(`  manifest: ${manifestPath}`);
  console.error(`  root    : ${rootPath}`);
  for (const failure of failures) {
    console.error(`  - ${failure}`);
  }
  process.exit(1);
}

console.log(
  `[validate-runtime-manifest] OK: ${args.label} matches ${path.relative(repoRoot, manifestPath)} (${expectedFiles.length} files).`,
);