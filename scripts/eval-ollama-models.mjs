import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../", import.meta.url));
const resultsDir = path.join(repoRoot, "bench", "results");

function parseArgs(argv) {
  const models = [];
  let warmup = 1;
  let runs = 2;
  let failOnSloMiss = false;
  let requireExactModel = false;

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--model" && argv[i + 1]) {
      models.push(argv[++i]);
      continue;
    }
    if (arg.startsWith("--model=")) {
      models.push(arg.slice("--model=".length));
      continue;
    }
    if (arg === "--warmup" && argv[i + 1]) {
      const parsed = Number(argv[++i]);
      if (Number.isFinite(parsed) && parsed >= 0) warmup = parsed;
      continue;
    }
    if (arg.startsWith("--warmup=")) {
      const parsed = Number(arg.slice("--warmup=".length));
      if (Number.isFinite(parsed) && parsed >= 0) warmup = parsed;
      continue;
    }
    if (arg === "--runs" && argv[i + 1]) {
      const parsed = Number(argv[++i]);
      if (Number.isFinite(parsed) && parsed > 0) runs = parsed;
      continue;
    }
    if (arg.startsWith("--runs=")) {
      const parsed = Number(arg.slice("--runs=".length));
      if (Number.isFinite(parsed) && parsed > 0) runs = parsed;
      continue;
    }
    if (arg === "--fail-on-slo-miss") {
      failOnSloMiss = true;
      continue;
    }
    if (arg === "--require-exact-model") {
      requireExactModel = true;
    }
  }

  if (models.length === 0) {
    throw new Error("Missing models. Use --model <tag> (repeatable).");
  }

  return { models, warmup, runs, failOnSloMiss, requireExactModel };
}

function runBenchmark(model, warmup, runs, failOnSloMiss) {
  const args = [
    "run",
    "benchmark:latency",
    "--",
    "-Warmup",
    String(warmup),
    "-Runs",
    String(runs),
    "-RefinementModel",
    model,
  ];
  if (failOnSloMiss) {
    args.push("-FailOnSloMiss");
  }

  console.log(`[Ollama Eval] Running ${model}`);
  const result = spawnSync("npm", args, {
    cwd: repoRoot,
    stdio: "inherit",
    shell: true,
    encoding: "utf8",
  });
  if ((result.status ?? 1) !== 0) {
    throw new Error(`Benchmark failed for '${model}' with exit code ${result.status ?? 1}`);
  }
}

function safeName(model) {
  return model.replace(/[^A-Za-z0-9._-]/g, "_");
}

function readLatestReport() {
  const latestPath = path.join(resultsDir, "latest.json");
  if (!fs.existsSync(latestPath)) {
    throw new Error(`Missing benchmark report: ${latestPath}`);
  }
  return {
    latestPath,
    report: JSON.parse(fs.readFileSync(latestPath, "utf8")),
  };
}

function main() {
  const { models, warmup, runs, failOnSloMiss, requireExactModel } = parseArgs(process.argv.slice(2));
  fs.mkdirSync(resultsDir, { recursive: true });

  const summary = [];
  for (const model of models) {
    runBenchmark(model, warmup, runs, failOnSloMiss);
    const { latestPath, report } = readLatestReport();
    const modelReportPath = path.join(resultsDir, `latency-${safeName(model)}.json`);
    fs.copyFileSync(latestPath, modelReportPath);
    const resolvedRefinementModels = Array.isArray(report.samples)
      ? [...new Set(report.samples.map((sample) => sample.refinement_model).filter(Boolean))]
      : [];
    summary.push({
      requested_model: model,
      resolved_refinement_models: resolvedRefinementModels,
      p50_ms: report.p50_ms,
      p95_ms: report.p95_ms,
      slo_pass: report.slo_pass,
      warnings: Array.isArray(report.warnings) ? report.warnings.length : 0,
      sample_modes: Array.isArray(report.samples)
        ? [...new Set(report.samples.map((sample) => sample.mode))]
        : [],
      report_path: modelReportPath,
    });
    if (requireExactModel && !resolvedRefinementModels.includes(model)) {
      throw new Error(
        `Requested model '${model}' was not used. Resolved models: ${
          resolvedRefinementModels.length > 0 ? resolvedRefinementModels.join(", ") : "(none)"
        }`
      );
    }
  }

  const summaryPath = path.join(resultsDir, "ollama-model-eval.latest.json");
  fs.writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`);
  console.log(`[Ollama Eval] Summary written: ${summaryPath}`);
}

main();
