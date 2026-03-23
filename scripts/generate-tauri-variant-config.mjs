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
    "[generate-tauri-variant-config] Usage: node scripts/generate-tauri-variant-config.mjs --variant <vulkan|cuda-lite|cuda-complete> --out <path>",
  );
  process.exit(2);
}

const allowed = new Set(["vulkan", "vulkan-only", "cuda-lite", "cuda-complete"]);
if (!allowed.has(variant)) {
  console.error(`[generate-tauri-variant-config] Unknown variant '${variant}'.`);
  process.exit(2);
}

const normalizedVariant = variant === "vulkan-only" ? "vulkan" : variant;
const config = JSON.parse(fs.readFileSync(baseConfigPath, "utf8"));
const resources = config?.bundle?.resources;
if (!resources || typeof resources !== "object" || Array.isArray(resources)) {
  console.error("[generate-tauri-variant-config] bundle.resources in tauri.conf.json is missing or invalid.");
  process.exit(1);
}

const filteredResources = {};
for (const [key, value] of Object.entries(resources)) {
  if (normalizedVariant === "vulkan" && key.startsWith("bin/cuda/")) {
    continue;
  }
  if (normalizedVariant === "cuda-lite" && key === "bin/cuda/cublasLt64_13.dll") {
    continue;
  }
  // FFmpeg and Piper are downloaded on-demand for non-offline variants
  if (normalizedVariant !== "cuda-complete" && key.startsWith("bin/ffmpeg/")) {
    continue;
  }
  if (normalizedVariant !== "cuda-complete" && key.startsWith("bin/piper/")) {
    continue;
  }
  filteredResources[key] = value;
}

config.bundle.resources = filteredResources;

const absoluteOutPath = path.isAbsolute(outPath) ? outPath : path.join(repoRoot, outPath);
fs.mkdirSync(path.dirname(absoluteOutPath), { recursive: true });
fs.writeFileSync(absoluteOutPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");

console.log(
  `[generate-tauri-variant-config] Wrote ${path.relative(repoRoot, absoluteOutPath)} (variant=${normalizedVariant}).`,
);
