import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..");
const baseConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");

function parseArgs(argv) {
  const out = { variant: "", outPath: "" };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--variant" && argv[i + 1]) {
      out.variant = argv[i + 1].trim().toLowerCase();
      i += 1;
      continue;
    }
    if (argv[i] === "--out" && argv[i + 1]) {
      out.outPath = argv[i + 1].trim();
      i += 1;
      continue;
    }
  }
  return out;
}

const { variant, outPath } = parseArgs(process.argv.slice(2));
if (!variant || !outPath) {
  console.error(
    "[generate-tauri-variant-config] Usage: node scripts/generate-tauri-variant-config.mjs --variant <mini|full|ci> --out <path>",
  );
  process.exit(2);
}

// mini: app + quantize only (setup wizard downloads the speech engine). full: bundles bin/cuda + bin/vulkan.
// ci: no runtime payload at all (cargo check/test on runners without the DLLs).
const allowed = new Set(["mini", "full", "ci"]);
if (!allowed.has(variant)) {
  console.error(`[generate-tauri-variant-config] Unknown variant '${variant}'.`);
  process.exit(2);
}

const config = JSON.parse(fs.readFileSync(baseConfigPath, "utf8"));
const resources = config?.bundle?.resources;
if (!resources || typeof resources !== "object" || Array.isArray(resources)) {
  console.error("[generate-tauri-variant-config] bundle.resources in tauri.conf.json is missing or invalid.");
  process.exit(1);
}

const filteredResources = {};
for (const [key, value] of Object.entries(resources)) {
  const isRuntime = key.startsWith("bin/cuda/") || key.startsWith("bin/vulkan/");
  if (isRuntime && variant !== "full") {
    continue;
  }
  filteredResources[key] = value;
}

config.bundle.resources = filteredResources;

const absoluteOutPath = path.isAbsolute(outPath) ? outPath : path.join(repoRoot, outPath);
fs.mkdirSync(path.dirname(absoluteOutPath), { recursive: true });
fs.writeFileSync(absoluteOutPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");

console.log(
  `[generate-tauri-variant-config] Wrote ${path.relative(repoRoot, absoluteOutPath)} (variant=${variant}).`,
);

// Compile-time variant define consumed by src-tauri/nsis/hooks.nsh (gitignored, regenerated per build).
if (variant !== "ci") {
  fs.writeFileSync(
    path.join(repoRoot, "src-tauri", "nsis", "variant-define.nsh"),
    `!define TRISPR_VARIANT "${variant}"\n`,
    "utf8",
  );
}
