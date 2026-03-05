import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const cwd = fileURLToPath(new URL("../src-tauri/", import.meta.url));

function run(cmd, args, inherit = true) {
  return spawnSync(cmd, args, {
    cwd,
    stdio: inherit ? "inherit" : "pipe",
    encoding: "utf8",
    shell: true,
  });
}

function hasCargoAuditSubcommand() {
  const check = run("cargo", ["--list"], false);
  if (check.status !== 0) return false;
  const out = `${check.stdout ?? ""}\n${check.stderr ?? ""}`;
  return /\baudit\b/.test(out);
}

function hasCargoAuditBinary() {
  const check = run("cargo-audit", ["--version"], false);
  return check.status === 0;
}

function runAudit() {
  if (hasCargoAuditSubcommand()) {
    return run("cargo", ["audit"]);
  }
  if (hasCargoAuditBinary()) {
    return run("cargo-audit", []);
  }
  return null;
}

let result = runAudit();
if (result) {
  process.exit(result.status ?? 1);
}

console.warn("Rust audit command missing. Attempting one-time install via `cargo install cargo-audit --locked`...");
const install = run("cargo", ["install", "cargo-audit", "--locked"]);
if (install.status === 0) {
  result = runAudit();
  if (result) {
    process.exit(result.status ?? 1);
  }
}

console.warn("Rust audit skipped: `cargo audit` is not available in this shell/toolchain.");
console.warn("Install manually with: cargo install cargo-audit --locked");
process.exit(0);
