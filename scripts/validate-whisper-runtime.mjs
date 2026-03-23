import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..");
const binRoot = path.join(repoRoot, "src-tauri", "bin");

const requiredCudaLite = [
  "whisper-cli.exe",
  "whisper-server.exe",
  "whisper.dll",
  "ggml.dll",
  "ggml-base.dll",
  "ggml-cpu.dll",
  "ggml-cuda.dll",
  "cublas64_13.dll",
  "cudart64_13.dll",
];

const requiredCudaComplete = [
  ...requiredCudaLite,
  "cublasLt64_13.dll",
];

const requiredVulkan = [
  "whisper-cli.exe",
  "whisper.dll",
  "ggml.dll",
  "ggml-base.dll",
  "ggml-cpu.dll",
  "ggml-vulkan.dll",
];

const requiredByVariant = {
  vulkan: {
    vulkan: requiredVulkan,
  },
  "vulkan-only": {
    vulkan: requiredVulkan,
  },
  "cuda-lite": {
    cuda: requiredCudaLite,
    vulkan: requiredVulkan,
  },
  "cuda-complete": {
    cuda: requiredCudaComplete,
    vulkan: requiredVulkan,
  },
  // Backward-compatible alias from previous "unified" script.
  unified: {
    cuda: requiredCudaComplete,
    vulkan: requiredVulkan,
  },
};

const args = process.argv.slice(2);
let variant = "cuda-complete";
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === "--variant" && args[i + 1]) {
    variant = args[i + 1].trim().toLowerCase();
    i += 1;
  }
}

if (!requiredByVariant[variant]) {
  const allowed = Object.keys(requiredByVariant).join(", ");
  console.error(`[validate-whisper-runtime] Unknown variant '${variant}'. Allowed: ${allowed}`);
  process.exit(2);
}

const requiredByBackend = requiredByVariant[variant];

function missingFilesForBackend(backend, requiredFiles) {
  const backendDir = path.join(binRoot, backend);
  if (!fs.existsSync(backendDir) || !fs.statSync(backendDir).isDirectory()) {
    return {
      backend,
      backendDir,
      missing: ["<backend directory missing>"],
    };
  }

  const missing = [];
  for (const file of requiredFiles) {
    const filePath = path.join(backendDir, file);
    if (!fs.existsSync(filePath)) {
      missing.push(file);
      continue;
    }
    const stat = fs.statSync(filePath);
    if (!stat.isFile() || stat.size <= 0) {
      missing.push(`${file} (empty or invalid file)`);
    }
  }
  return { backend, backendDir, missing };
}

const reports = Object.entries(requiredByBackend).map(([backend, files]) =>
  missingFilesForBackend(backend, files),
);
const failing = reports.filter((report) => report.missing.length > 0);

if (failing.length > 0) {
  console.error(`[validate-whisper-runtime] Whisper runtime validation failed (variant=${variant}).`);
  for (const report of failing) {
    console.error(`  backend=${report.backend} dir=${report.backendDir}`);
    for (const file of report.missing) {
      console.error(`    missing: ${file}`);
    }
  }
  console.error(
    "  Hint: hydrate/copy the required runtime files into src-tauri/bin/{cuda,vulkan} before installer builds.",
  );
  process.exit(1);
}

console.log(`[validate-whisper-runtime] OK: runtime files are complete for variant=${variant}.`);
