import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import path from "node:path";

const repoRoot = fileURLToPath(new URL("../", import.meta.url));
const resultsDir = path.join(repoRoot, "bench", "results");

function runStep(name, command, cwd = repoRoot) {
  const startedAt = Date.now();
  console.log(`[Release QA] ${name}: ${command}`);
  const result = spawnSync(command, {
    cwd,
    stdio: "inherit",
    shell: true,
    encoding: "utf8",
  });
  const durationMs = Date.now() - startedAt;
  const status = result.status ?? 1;
  return { name, command, status, durationMs };
}

function runOrExit(steps, name, command) {
  const step = runStep(name, command);
  steps.push(step);
  if (step.status !== 0) {
    process.exit(step.status);
  }
}

const startedAtIso = new Date().toISOString();
const startedAtMs = Date.now();
const steps = [];

runOrExit(steps, "build", "npm run build");
runOrExit(steps, "test", "npm test");
runOrExit(
  steps,
  "cargo-test-no-default-features",
  "cargo test --manifest-path src-tauri/Cargo.toml --lib"
);
runOrExit(
  steps,
  "cargo-test-bins-no-default-features",
  "cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --bins"
);
runOrExit(steps, "cargo-build", "cargo build --manifest-path src-tauri/Cargo.toml");
runOrExit(steps, "audit-rust", "npm run audit:rust");

const endedAtIso = new Date().toISOString();
const report = {
  started_at: startedAtIso,
  ended_at: endedAtIso,
  duration_ms: Date.now() - startedAtMs,
  steps,
};

fs.mkdirSync(resultsDir, { recursive: true });
fs.writeFileSync(path.join(resultsDir, "release-qa.latest.json"), `${JSON.stringify(report, null, 2)}\n`);

const stamp = endedAtIso.replace(/[:.]/g, "-");
fs.writeFileSync(path.join(resultsDir, `release-qa.${stamp}.json`), `${JSON.stringify(report, null, 2)}\n`);

console.log("[Release QA] Completed successfully.");
