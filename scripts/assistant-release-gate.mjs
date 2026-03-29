import { spawnSync, execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../", import.meta.url));
const reportsDir = path.join(repoRoot, "docs", "reports");
const benchResultsDir = path.join(repoRoot, "bench", "results");

function parseArgs(argv) {
  const args = {
    requireSoak: false,
    strictBenchmark: false,
    skipRustLibTests: false,
    soak8Path: null,
    soak24Path: null,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (token === "--require-soak") {
      args.requireSoak = true;
      continue;
    }
    if (token === "--strict-benchmark") {
      args.strictBenchmark = true;
      continue;
    }
    if (token === "--skip-rust-lib-tests") {
      args.skipRustLibTests = true;
      continue;
    }
    if (token === "--soak8") {
      args.soak8Path = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (token === "--soak24") {
      args.soak24Path = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
  }
  return args;
}

function runStep(name, command) {
  const startedMs = Date.now();
  console.log(`[Block U Gate] ${name}: ${command}`);
  const result = spawnSync(command, {
    cwd: repoRoot,
    shell: true,
    stdio: "inherit",
    encoding: "utf8",
  });
  return {
    name,
    command,
    status: result.status ?? 1,
    duration_ms: Date.now() - startedMs,
  };
}

function runOrSkip(steps, name, command, skip = false, skipReason = null) {
  if (skip) {
    const step = {
      name,
      command,
      skipped: true,
      skip_reason: skipReason ?? "skipped_by_option",
      status: null,
      duration_ms: 0,
    };
    steps.push(step);
    console.log(`[Block U Gate] ${name}: skipped (${step.skip_reason})`);
    return step;
  }
  const step = runStep(name, command);
  steps.push(step);
  return step;
}

function readJsonIfExists(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    return null;
  }
}

function readSoakEvidence(filePath) {
  if (!filePath) return null;
  const resolved = path.isAbsolute(filePath) ? filePath : path.join(repoRoot, filePath);
  const data = readJsonIfExists(resolved);
  if (!data) {
    return {
      path: resolved,
      available: false,
      status: "missing_or_invalid",
    };
  }
  return {
    path: resolved,
    available: true,
    status: "provided",
    summary: data.summary ?? null,
    started_at: data.started_at ?? null,
    ended_at: data.ended_at ?? null,
  };
}

function bool(value) {
  return value === true;
}

function toIsoStamp(value) {
  return value.replace(/[:.]/g, "-");
}

function markdownReport(report) {
  const lines = [];
  lines.push("# Block U Release Gate Report");
  lines.push("");
  lines.push(`Generated: ${report.generated_at}`);
  lines.push(`Commit: ${report.git_commit}`);
  lines.push(`Overall gate pass: ${report.gates.overall_pass ? "yes" : "no"}`);
  lines.push("");
  lines.push("## Gate Summary");
  lines.push("");
  lines.push(`- Automated checks: ${report.gates.automated_checks_pass ? "pass" : "fail"}`);
  lines.push(`- Benchmark linkage: ${report.gates.benchmark_linked_pass ? "pass" : "fail"}`);
  lines.push(`- Soak evidence attached: ${report.gates.soak_evidence_attached ? "yes" : "no"}`);
  lines.push(`- Soak required: ${report.options.require_soak ? "yes" : "no"}`);
  lines.push("");
  if (report.warnings.length > 0) {
    lines.push("## Warnings");
    lines.push("");
    report.warnings.forEach((warning) => lines.push(`- ${warning}`));
    lines.push("");
  }
  lines.push("## Steps");
  lines.push("");
  report.steps.forEach((step) => {
    if (step.skipped) {
      lines.push(`- ${step.name}: skipped (${step.skip_reason ?? "n/a"})`);
    } else {
      lines.push(`- ${step.name}: ${step.status === 0 ? "pass" : "fail"} (${step.duration_ms} ms)`);
    }
  });
  lines.push("");
  lines.push("## Bench Reports");
  lines.push("");
  lines.push(`- Latency report present: ${report.benchmarks.latency_present ? "yes" : "no"}`);
  lines.push(`- TTS report present: ${report.benchmarks.tts_present ? "yes" : "no"}`);
  lines.push(`- TTS release gate pass: ${report.benchmarks.tts_release_gate_pass ? "yes" : "no"}`);
  lines.push(`- TTS provider consistency: ${report.benchmarks.tts_provider_consistency_ok ? "yes" : "no"}`);
  lines.push(`- TTS uncategorized failures: ${report.benchmarks.tts_uncategorized_failure_count}`);
  lines.push("");
  lines.push("## Soak Evidence");
  lines.push("");
  lines.push(`- 8h soak: ${report.soak.eight_hour?.available ? "attached" : "missing"}`);
  lines.push(`- 24h soak: ${report.soak.twenty_four_hour?.available ? "attached" : "missing"}`);
  lines.push("");
  return `${lines.join("\n")}\n`;
}

const options = parseArgs(process.argv.slice(2));
const startedAt = new Date().toISOString();
const steps = [];

runOrSkip(steps, "build", "npm run build");
runOrSkip(steps, "test", "npm test");
runOrSkip(
  steps,
  "cargo-test-lib",
  "cargo test --manifest-path src-tauri/Cargo.toml --lib",
  options.skipRustLibTests,
  "skip-rust-lib-tests"
);

const latencyReport = readJsonIfExists(path.join(benchResultsDir, "latest.json"));
const ttsReport = readJsonIfExists(path.join(benchResultsDir, "tts.latest.json"));
const soak8 = readSoakEvidence(options.soak8Path);
const soak24 = readSoakEvidence(options.soak24Path);

const benchmarkLinkedPass = Boolean(
  latencyReport
    && ttsReport
    && bool(ttsReport.release_gate_pass)
    && bool(ttsReport.provider_consistency_ok)
    && Number(ttsReport.uncategorized_failure_count ?? 0) === 0
);
const soakEvidenceAttached = Boolean(soak8?.available && soak24?.available);

const warnings = [];
if (!latencyReport) warnings.push("Latency benchmark report (bench/results/latest.json) missing.");
if (!ttsReport) warnings.push("TTS benchmark report (bench/results/tts.latest.json) missing.");
if (ttsReport && !bool(ttsReport.release_gate_pass)) {
  warnings.push("TTS report indicates release_gate_pass=false.");
}
if (ttsReport && !bool(ttsReport.provider_consistency_ok)) {
  warnings.push("TTS report indicates provider_consistency_ok=false.");
}
if (ttsReport && Number(ttsReport.uncategorized_failure_count ?? 0) !== 0) {
  warnings.push("TTS report has uncategorized failures.");
}
if (!soak8?.available) warnings.push("8h soak evidence not attached.");
if (!soak24?.available) warnings.push("24h soak evidence not attached.");

if (options.strictBenchmark && !benchmarkLinkedPass) {
  console.error("[Block U Gate] strict benchmark mode failed.");
  process.exit(2);
}
if (options.requireSoak && !soakEvidenceAttached) {
  console.error("[Block U Gate] soak evidence required but missing.");
  process.exit(3);
}

let gitCommit = "unknown";
try {
  gitCommit = execSync("git rev-parse HEAD", { cwd: repoRoot, encoding: "utf8" }).trim();
} catch {
  // keep unknown
}

const automatedChecksPass = steps.every((step) => step.skipped || step.status === 0);
const overallPass = automatedChecksPass
  && (options.strictBenchmark ? benchmarkLinkedPass : true)
  && (options.requireSoak ? soakEvidenceAttached : true);

const endedAt = new Date().toISOString();
const report = {
  generated_at: endedAt,
  started_at: startedAt,
  ended_at: endedAt,
  git_commit: gitCommit,
  options: {
    require_soak: options.requireSoak,
    strict_benchmark: options.strictBenchmark,
    skip_rust_lib_tests: options.skipRustLibTests,
    soak8_path: options.soak8Path,
    soak24_path: options.soak24Path,
  },
  steps,
  benchmarks: {
    latency_present: Boolean(latencyReport),
    tts_present: Boolean(ttsReport),
    tts_release_gate_pass: bool(ttsReport?.release_gate_pass),
    tts_provider_consistency_ok: bool(ttsReport?.provider_consistency_ok),
    tts_uncategorized_failure_count: Number(ttsReport?.uncategorized_failure_count ?? 0),
    latency_report_path: path.join("bench", "results", "latest.json"),
    tts_report_path: path.join("bench", "results", "tts.latest.json"),
  },
  soak: {
    eight_hour: soak8,
    twenty_four_hour: soak24,
  },
  gates: {
    automated_checks_pass: automatedChecksPass,
    benchmark_linked_pass: benchmarkLinkedPass,
    soak_evidence_attached: soakEvidenceAttached,
    overall_pass: overallPass,
  },
  warnings,
};

fs.mkdirSync(reportsDir, { recursive: true });
const stamp = toIsoStamp(endedAt);
const latestJson = path.join(reportsDir, "block_u_release_gate.latest.json");
const stampJson = path.join(reportsDir, `block_u_release_gate.${stamp}.json`);
const latestMd = path.join(reportsDir, "block_u_release_gate.latest.md");
const stampMd = path.join(reportsDir, `block_u_release_gate.${stamp}.md`);
fs.writeFileSync(latestJson, `${JSON.stringify(report, null, 2)}\n`);
fs.writeFileSync(stampJson, `${JSON.stringify(report, null, 2)}\n`);
const markdown = markdownReport(report);
fs.writeFileSync(latestMd, markdown);
fs.writeFileSync(stampMd, markdown);

console.log(`[Block U Gate] Report written: ${path.relative(repoRoot, latestJson)}`);
console.log(`[Block U Gate] Overall pass: ${overallPass ? "yes" : "no"}`);
if (!overallPass) {
  process.exit(4);
}
