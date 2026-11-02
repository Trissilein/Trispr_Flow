#!/usr/bin/env node
/**
 * Ollama soak / reliability harness for Trispr Flow (Windows only, Node stdlib only).
 *
 * Starts its OWN `ollama serve` processes on ports 11450-11459 (never touches the user's
 * Ollama on :11434, trispr-flow.exe or whisper-server.exe) and measures:
 *   1  soak            N back-to-back refinement requests: latency, RSS/handles/threads/GPU trend, quality
 *   2  load cycles     keep_alive:0 -> reload -> request, cold-start time, GPU memory release
 *   3  server abort    hard kill mid-request, port release, restart-to-ready (mirrors the app's 12 s wait)
 *   4  version switch  0.30.4 -> 0.40.13 on the SAME port with the real binaries
 *   5  parallel        1/2/4/8 concurrent requests, light requests next to a running chat
 *   6  whisper         passive GPU-memory observation next to the user's whisper-server
 *   7  start probe     N server starts, ready time vs GPU discovery vs host load (narrows slow starts)
 *   S  summary         compile results.summary from the phases measured so far (after a final model-store snapshot)
 *   L  logs            start timeline (listening / GPU discovery) parsed from all server logs, no server started
 *
 * The request form mirrors the app (src-tauri/src/ai_fallback/model_verify.rs + provider.rs):
 * /api/chat, stream:false, think:false, keep_alive:-1, options {num_ctx:2048, num_thread,
 * temperature:0.15, num_predict:2048}, system prompt OLLAMA_PROMPT_DE (read from provider.rs).
 *
 * Deliberate differences from the app's spawn (documented in the result file, meta.deviations):
 *   - OLLAMA_NOPRUNE=1: a plain `ollama serve` deletes unreferenced blobs on startup; this harness
 *     must not modify the user's model store.
 *   - stdout/stderr go to files in bench/results/ollama-soak-logs (the app discards them).
 *
 * Usage:  node scripts/ollama-soak.mjs [--phases=1,2,3,4,5,6] [--requests=300] [--cycles=30]
 *         [--cycle-sample-every=1] [--probe-reps=20] [--kill-reps=10] [--kill-load-reps=5] [--kill-orphan-reps=3] [--switch-reps=10]
 *         [--model=gemma4:e4b] [--models-dir=D:\Ollama\models] [--out=<json>] [--sample-every=25]
 *         [--key-suffix=_extended]  (appends to the phase keys, so a re-run does not overwrite the first result)
 * Results are merged into the output JSON phase by phase (default bench/results/ollama-soak-2026-11-02.json).
 * Output goes to the gitignored bench/results/ directory.
 */
import { spawn, spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(fileURLToPath(new URL("../", import.meta.url)));

// ───────────────────────────── configuration ─────────────────────────────
const args = Object.fromEntries(
  process.argv.slice(2).map((a) => {
    const m = a.match(/^--([^=]+)(?:=(.*))?$/);
    return m ? [m[1], m[2] ?? "true"] : [a, "true"];
  }),
);
const num = (key, dflt) => (args[key] !== undefined ? Number(args[key]) : dflt);

const CFG = {
  model: args.model ?? "gemma4:e4b",
  modelsDir: args["models-dir"] ?? "D:\\Ollama\\models",
  runtimeRoot:
    args["runtime-root"] ?? path.join(process.env.LOCALAPPDATA ?? "", "Trispr Flow", "ollama-runtime"),
  vNew: args["v-new"] ?? "0.40.13",
  vOld: args["v-old"] ?? "0.30.4",
  out: path.resolve(repoRoot, args.out ?? "bench/results/ollama-soak-2026-11-02.json"),
  logDir: path.join(repoRoot, "bench", "results", "ollama-soak-logs"),
  pidFile: path.join(repoRoot, "bench", "results", "ollama-soak.pids.json"),
  phases: (args.phases ?? "1,2,3,4,5,6").split(",").map((s) => s.trim()),
  requests: num("requests", 300),
  sampleEvery: num("sample-every", 25),
  cycles: num("cycles", 30),
  killReps: num("kill-reps", 10),
  killLoadReps: num("kill-load-reps", 5),
  killOrphanReps: num("kill-orphan-reps", 3),
  switchReps: num("switch-reps", 10),
  probeReps: num("probe-reps", 20),
  cycleSampleEvery: num("cycle-sample-every", 1), // phase 2: process/GPU sample every n-th cycle (1 = every cycle)
  parallelRounds: num("parallel-rounds", 40), // requests per concurrency level
  requestTimeoutMs: 120_000,
};
const PORTS = { p1: 11450, p2: 11451, p3: 11452, p4: 11453, p5: 11454, p6: 11455, p7: 11456 };
const STARTUP_FOREGROUND_WAIT_MS = 12_000; // src-tauri/src/ollama_runtime.rs
const PORT_RELEASE_WAIT_MS = 10_000; // app waits up to 10 s for the port after stopping
const NUM_THREAD = Math.min(8, Math.max(2, Math.floor(os.availableParallelism() / 2))); // default_ollama_num_thread

// ───────────────────────────── small utilities ─────────────────────────────
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const now = () => performance.now();
const r1 = (v) => (Number.isFinite(v) ? Math.round(v * 10) / 10 : null);
const mb = (bytes) => (Number.isFinite(bytes) ? Math.round((bytes / 1048576) * 10) / 10 : null);
const ns2ms = (ns) => (Number.isFinite(ns) ? r1(ns / 1e6) : null);
const log = (...a) => console.log(`[${new Date().toISOString().slice(11, 19)}]`, ...a);

function pct(sorted, p) {
  return sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1))];
}
/** Nearest-rank percentiles. */
function stats(values) {
  const a = values.filter(Number.isFinite).sort((x, y) => x - y);
  if (!a.length) return null;
  const mean = a.reduce((s, v) => s + v, 0) / a.length;
  const sd = Math.sqrt(a.reduce((s, v) => s + (v - mean) ** 2, 0) / a.length);
  return {
    n: a.length,
    min: r1(a[0]),
    p50: r1(pct(a, 50)),
    p95: r1(pct(a, 95)),
    p99: r1(pct(a, 99)),
    max: r1(a[a.length - 1]),
    mean: r1(mean),
    stddev: r1(sd),
  };
}
function linreg(xs, ys) {
  const n = xs.length;
  if (n < 3) return { slope: null, r2: null };
  const mx = xs.reduce((s, v) => s + v, 0) / n;
  const my = ys.reduce((s, v) => s + v, 0) / n;
  let sxx = 0;
  let sxy = 0;
  let syy = 0;
  for (let i = 0; i < n; i++) {
    sxx += (xs[i] - mx) ** 2;
    sxy += (xs[i] - mx) * (ys[i] - my);
    syy += (ys[i] - my) ** 2;
  }
  const slope = sxx ? sxy / sxx : 0;
  const r2 = sxx && syy ? (sxy * sxy) / (sxx * syy) : 0;
  return { slope, r2 };
}
/** Trend over [{x, v}] points. slope is per 100 x (requests/cycles). */
function trend(points) {
  const p = points.filter((q) => Number.isFinite(q.v));
  if (p.length < 2) return null;
  const { slope, r2 } = linreg(p.map((q) => q.x), p.map((q) => q.v));
  const first = p[0].v;
  const last = p[p.length - 1].v;
  return {
    first: r1(first),
    last: r1(last),
    min: r1(Math.min(...p.map((q) => q.v))),
    max: r1(Math.max(...p.map((q) => q.v))),
    delta: r1(last - first),
    delta_pct: first ? r1(((last - first) / first) * 100) : null,
    slope_per_100: slope === null ? null : r1(slope * 100),
    r2: r2 === null ? null : Math.round(r2 * 100) / 100,
    series: p.map((q) => [q.x, r1(q.v)]),
  };
}
function cpuTimes() {
  return os.cpus().reduce(
    (a, c) => {
      const t = c.times;
      a.idle += t.idle;
      a.total += t.user + t.nice + t.sys + t.idle + t.irq;
      return a;
    },
    { idle: 0, total: 0 },
  );
}
const cpuPct = (a, b) => (b.total - a.total > 0 ? r1(100 * (1 - (b.idle - a.idle) / (b.total - a.total))) : null);

// ───────────────────────────── PowerShell bridge ─────────────────────────────
function ps(script, timeoutMs = 60_000) {
  const enc = Buffer.from(script, "utf16le").toString("base64");
  const r = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", enc],
    { encoding: "utf8", timeout: timeoutMs, windowsHide: true, maxBuffer: 32 * 1024 * 1024 },
  );
  if (r.status !== 0) throw new Error(`powershell failed (${r.status}): ${(r.stderr || r.error || "").toString().slice(0, 400)}`);
  return r.stdout.trim();
}

const SAMPLE_PS = String.raw`
$ErrorActionPreference = 'SilentlyContinue'
$roots = @(__ROOTS__)
$all = @(Get-CimInstance Win32_Process)
$byId = @{}
foreach ($p in $all) { $byId[[int]$p.ProcessId] = $p }
$set = New-Object 'System.Collections.Generic.HashSet[int]'
foreach ($r in $roots) { if ($r -gt 0) { [void]$set.Add([int]$r) } }
do {
  $changed = $false
  foreach ($p in $all) {
    if ($set.Contains([int]$p.ParentProcessId) -and -not $set.Contains([int]$p.ProcessId)) {
      # A real child is younger than its parent. An older process only carries a recycled parent PID
      # (seen: the user's whisper-server showed up as a "child" of a new runner).
      $par = $byId[[int]$p.ParentProcessId]
      if ($par -and $p.CreationDate -and $par.CreationDate -and $p.CreationDate -lt $par.CreationDate) { continue }
      [void]$set.Add([int]$p.ProcessId); $changed = $true
    }
  }
} while ($changed)
$ded = @{}; $shr = @{}
$tries = 0
# The GPU counter occasionally returns no instance for a live process (seen: 0 MB for a loaded runner).
# Retry until every llama-server on the box has an instance.
for ($try = 0; $try -lt 5; $try++) {
  $tries = $try + 1
  $ded = @{}; $shr = @{}
  $cs = $null
  try { $cs = (Get-Counter '\GPU Process Memory(*)\Dedicated Usage','\GPU Process Memory(*)\Shared Usage' -ErrorAction Stop).CounterSamples } catch {}
  foreach ($s in $cs) {
    if ($s.InstanceName -match '^pid_(\d+)_') {
      $k = [int]$Matches[1]
      if ($s.Path -match 'dedicated usage$') { $ded[$k] = [double]$ded[$k] + [double]$s.CookedValue }
      elseif ($s.Path -match 'shared usage$') { $shr[$k] = [double]$shr[$k] + [double]$s.CookedValue }
    }
  }
  $missing = 0
  foreach ($p in $all) { if ($p.Name -eq 'llama-server.exe' -and -not $ded.ContainsKey([int]$p.ProcessId)) { $missing++ } }
  if ($missing -eq 0) { break }
  Start-Sleep -Milliseconds 300
}
function RowFor($id) {
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if (-not $p) { return $null }
  $c = $byId[[int]$id]
  $cmd = ''; $start = ''; $ppid = 0
  if ($c) { $cmd = [string]$c.CommandLine; $ppid = [int]$c.ParentProcessId; if ($c.CreationDate) { $start = $c.CreationDate.ToString('o') } }
  if ($cmd.Length -gt 300) { $cmd = $cmd.Substring(0, 300) }
  return [ordered]@{ pid = [int]$id; name = $p.ProcessName; ppid = $ppid; ws = [double]$p.WorkingSet64; priv = [double]$p.PrivateMemorySize64; handles = [int]$p.HandleCount; threads = [int]$p.Threads.Count; gpu_ded = [double]$ded[[int]$id]; gpu_shared = [double]$shr[[int]$id]; gpu_ok = $ded.ContainsKey([int]$id); start = $start; cmd = $cmd }
}
$mine = @()
foreach ($id in $set) { $row = RowFor $id; if ($row) { $mine += ,$row } }
$others = @()
foreach ($p in $all) {
  if ($set.Contains([int]$p.ProcessId)) { continue }
  if ($p.Name -match '^(ollama|llama-server|whisper-server|trispr-flow)(\.exe)?$') { $row = RowFor $p.ProcessId; if ($row) { $others += ,$row } }
}
$top = @()
foreach ($k in ($ded.Keys | Sort-Object { -[double]$ded[$_] } | Select-Object -First 8)) {
  $pp = Get-Process -Id $k -ErrorAction SilentlyContinue
  $nm = '?'; if ($pp) { $nm = $pp.ProcessName }
  $top += ,([ordered]@{ pid = [int]$k; name = $nm; gpu_ded = [double]$ded[$k] })
}
$nv = $null
try {
  $line = & nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader,nounits 2>$null | Select-Object -First 1
  if ($line) { $parts = $line.Split(','); $nv = [ordered]@{ used_mb = [double]($parts[0].Trim()); total_mb = [double]($parts[1].Trim()) } }
} catch {}
$busy = [ordered]@{}
foreach ($n in 'cargo','rustc','rust-lld','link','cl','vitest','esbuild','tsc','node','cmake','msbuild') {
  $cnt = @(Get-Process -Name $n -ErrorAction SilentlyContinue).Count
  if ($cnt -gt 0) { $busy[$n] = $cnt }
}
$out = [ordered]@{ mine = $mine; others = $others; gpu_top = $top; nvidia = $nv; busy = $busy; gpu_counter_tries = $tries; gpu_counter_missing = $missing }
ConvertTo-Json -InputObject $out -Depth 6 -Compress
`;

const PROTECTED_PS = String.raw`
$ErrorActionPreference = 'SilentlyContinue'
$rows = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(ollama|llama-server|whisper-server|trispr-flow)(\.exe)?$' } | ForEach-Object { [ordered]@{ pid = [int]$_.ProcessId; name = $_.Name; ppid = [int]$_.ParentProcessId; start = $(if ($_.CreationDate) { $_.CreationDate.ToString('o') } else { '' }); exe = [string]$_.ExecutablePath } })
ConvertTo-Json -InputObject $rows -Depth 4 -Compress
`;

function sampleRaw(rootPids) {
  const roots = (Array.isArray(rootPids) ? rootPids : [rootPids]).filter(Boolean).join(",");
  const out = ps(SAMPLE_PS.replace("__ROOTS__", roots || "0"));
  return JSON.parse(out);
}

/** Compact, human-checkable sample of a server tree (server process + descendants = runners). */
function summarize(raw, serverPid) {
  const mine = raw.mine ?? [];
  const server = mine.find((p) => p.pid === serverPid) ?? null;
  const kids = mine.filter((p) => p.pid !== serverPid);
  const agg = (list) => ({
    pids: list.map((p) => p.pid),
    names: list.map((p) => p.name),
    ws_mb: mb(list.reduce((s, p) => s + p.ws, 0)),
    priv_mb: mb(list.reduce((s, p) => s + p.priv, 0)),
    handles: list.reduce((s, p) => s + p.handles, 0),
    threads: list.reduce((s, p) => s + p.threads, 0),
    // null when a llama-server in the list had no GPU counter instance even after retries
    gpu_dedicated_mb: list.filter((p) => /llama-server/.test(p.name)).every((p) => p.gpu_ok !== false) ? mb(list.reduce((s, p) => s + p.gpu_ded, 0)) : null,
    gpu_shared_mb: list.filter((p) => /llama-server/.test(p.name)).every((p) => p.gpu_ok !== false) ? mb(list.reduce((s, p) => s + p.gpu_shared, 0)) : null,
  });
  return {
    server: server ? agg([server]) : null,
    runners: agg(kids),
    others: (raw.others ?? []).map((p) => ({
      pid: p.pid,
      name: p.name,
      ws_mb: mb(p.ws),
      handles: p.handles,
      threads: p.threads,
      gpu_dedicated_mb: mb(p.gpu_ded),
    })),
    gpu_top: (raw.gpu_top ?? []).map((p) => ({ pid: p.pid, name: p.name, gpu_dedicated_mb: mb(p.gpu_ded) })),
    nvidia_smi: raw.nvidia ?? null,
    busy_processes: raw.busy ?? {},
    gpu_counter: { tries: raw.gpu_counter_tries ?? null, still_missing: raw.gpu_counter_missing ?? null },
  };
}

// ───────────────────────────── protected processes / PID file ─────────────────────────────
let protectedPids = new Set();
const owned = new Map(); // pid -> {tag, child}
function loadProtected() {
  const rows = JSON.parse(ps(PROTECTED_PS) || "[]");
  protectedPids = new Set(rows.map((r) => r.pid));
  return rows;
}
function writePidFile() {
  const live = [...owned.entries()].map(([pid, o]) => ({ pid, tag: o.tag, version: o.version, port: o.port, started: o.started }));
  fs.mkdirSync(path.dirname(CFG.pidFile), { recursive: true });
  fs.writeFileSync(CFG.pidFile, JSON.stringify({ node_pid: process.pid, updated: new Date().toISOString(), live }, null, 2));
}

// ───────────────────────────── HTTP ─────────────────────────────
function request(port, method, urlPath, body, timeoutMs = CFG.requestTimeoutMs) {
  return new Promise((resolve) => {
    const t0 = now();
    const payload = body === undefined ? null : Buffer.from(JSON.stringify(body));
    let done = false;
    const finish = (r) => {
      if (done) return;
      done = true;
      r.ms = now() - t0;
      resolve(r);
    };
    const req = http.request(
      {
        host: "127.0.0.1",
        port,
        method,
        path: urlPath,
        agent: false,
        headers: payload ? { "content-type": "application/json", "content-length": payload.length } : {},
      },
      (res) => {
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () => {
          const text = Buffer.concat(chunks).toString("utf8");
          let json = null;
          try {
            json = JSON.parse(text);
          } catch {
            /* not json */
          }
          finish({ status: res.statusCode, ok: res.statusCode === 200, text, json, error: null });
        });
        res.on("error", (e) => finish({ status: res.statusCode, ok: false, error: e.code ?? e.message }));
        res.on("close", () => {
          if (!res.complete) finish({ status: res.statusCode, ok: false, error: "response aborted (connection closed)" });
        });
      },
    );
    req.setTimeout(timeoutMs, () => req.destroy(Object.assign(new Error("timeout"), { code: "TIMEOUT" })));
    req.on("error", (e) => finish({ status: null, ok: false, error: e.code ?? e.message }));
    if (payload) req.write(payload);
    req.end();
  });
}

function tcpInUse(port, timeoutMs = 500) {
  return new Promise((resolve) => {
    const s = net.connect({ host: "127.0.0.1", port });
    const t = setTimeout(() => {
      s.destroy();
      resolve(false);
    }, timeoutMs);
    s.on("connect", () => {
      clearTimeout(t);
      s.destroy();
      resolve(true);
    });
    s.on("error", () => {
      clearTimeout(t);
      resolve(false);
    });
  });
}

/** ms until nothing accepts connections on the port (polling every `stepMs`), measured from `fromT`. */
async function waitPortFree(port, fromT, maxMs = 30_000, stepMs = 20) {
  while (now() - fromT < maxMs) {
    if (!(await tcpInUse(port))) return r1(now() - fromT);
    await sleep(stepMs);
  }
  return null;
}

// ───────────────────────────── request form of the app ─────────────────────────────
function loadPrompt() {
  const src = fs.readFileSync(path.join(repoRoot, "src-tauri", "src", "ai_fallback", "provider.rs"), "utf8");
  const m = src.match(/pub const OLLAMA_PROMPT_DE: &str = "((?:[^"\\]|\\.)*)";/);
  if (!m) throw new Error("OLLAMA_PROMPT_DE not found in provider.rs");
  return m[1].replace(/\\(["\\])/g, "$1").replace(/\\n/g, "\n");
}
function loadSentences() {
  const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, "bench", "fixtures", "tts-de-kitten", "manifest.json"), "utf8"));
  // lower case, no punctuation (a decimal comma between digits stays: "4,5")
  return manifest.map((e) => e.text.toLowerCase().replace(/(?<!\d),|,(?!\d)|[.!?;:]/g, "").replace(/\s+/g, " ").trim());
}
const PROMPT_DE = loadPrompt();
const SENTENCES = loadSentences();
const LONG_TEXT = SENTENCES.join(" ");

function chatBody(text, { keep_alive = -1, model = CFG.model } = {}) {
  return {
    model,
    stream: false,
    think: false,
    messages: [
      { role: "system", content: PROMPT_DE },
      { role: "user", content: `<transcript>\n${text}\n</transcript>` },
    ],
    options: { num_ctx: 2048, num_thread: NUM_THREAD, temperature: 0.15, num_predict: 2048 },
    keep_alive,
  };
}

// port of evaluate_verify_output (model_verify.rs); truncated-tag handling simplified
const META_PREFIXES = [
  "hier ist", "hier sind", "hier der", "hier die", "hier das", "here is", "here's", "here are", "sure", "certainly",
  "of course", "natürlich", "gerne", "selbstverständlich", "okay", "der korrigierte", "korrigierter text",
  "korrigierte version", "the corrected", "corrected text",
];
const normWords = (t) =>
  t.split(/\s+/).map((w) => w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "").toLowerCase()).filter(Boolean);
function metaPrefix(answer) {
  const lowered = answer.replace(/^[^\p{L}\p{N}]+/u, "").toLowerCase();
  return META_PREFIXES.find((p) => lowered.startsWith(p) && !/[\p{L}\p{N}]/u.test(lowered.charAt(p.length) || " ")) ?? null;
}
function evaluate(input, raw) {
  let t = raw ?? "";
  if (t.includes("<think>")) t = t.replace(/<think>[\s\S]*?<\/think>\s*/g, "").trim();
  let b = t.trim();
  let stripped = false;
  if (/<\/transcript>$/i.test(b)) { b = b.replace(/<\/transcript>$/i, "").trimEnd(); stripped = true; }
  if (/^<transcript>/i.test(b)) { b = b.replace(/^<transcript>/i, "").trimStart(); stripped = true; }
  const cleaned = (stripped ? b : t).trim();
  const fail = (reason, extra = {}) => ({ pass: false, reason, ...extra });
  if (cleaned.includes("<think>") || cleaned.includes("</think>")) return fail("unterminated thinking block");
  if (!cleaned) return fail((raw ?? "").trim() ? "only a thinking block" : "empty answer", { empty: true });
  const mp = metaPrefix(cleaned);
  if (mp) return fail(`commentary prefix "${mp}"`);
  const inW = normWords(input);
  const outW = normWords(cleaned);
  const set = new Set(outW);
  const kept = inW.filter((w) => set.has(w)).length;
  const keptPct = Math.round((100 * kept) / Math.max(1, inW.length));
  if (kept * 100 < inW.length * 90) return fail(`only ${kept}/${inW.length} input words kept (${keptPct}%)`, { kept_pct: keptPct });
  if (outW.length * 2 > inW.length * 3 + 4) return fail(`extra text: ${outW.length} words for a ${inW.length}-word input`, { kept_pct: keptPct });
  return { pass: true, kept_pct: keptPct };
}

/** One chat request in the app's form. Never throws. */
async function chat(port, text, opts = {}) {
  const c0 = cpuTimes();
  const t0 = now();
  const r = await request(port, "POST", "/api/chat", chatBody(text, opts), opts.timeoutMs ?? CFG.requestTimeoutMs);
  const c1 = cpuTimes();
  const out = { ms: r1(r.ms), status: r.status, error: r.error ?? null, cpu_pct: cpuPct(c0, c1), t_end: t0 + r.ms };
  if (r.json) {
    out.load_ms = ns2ms(r.json.load_duration);
    out.total_ms = ns2ms(r.json.total_duration);
    out.prompt_eval_count = r.json.prompt_eval_count ?? null;
    out.eval_count = r.json.eval_count ?? null;
    out.eval_ms = ns2ms(r.json.eval_duration);
    out.done_reason = r.json.done_reason ?? null;
    out.content = r.json.message?.content ?? null;
    if (r.json.error) out.server_error = String(r.json.error).slice(0, 300);
  } else if (r.text) {
    out.body_head = r.text.slice(0, 300);
  }
  out.ok = r.ok && out.content !== null && out.content !== undefined && !out.server_error;
  out.quality = out.ok ? evaluate(text, out.content) : { pass: false, reason: out.error ?? out.server_error ?? `HTTP ${r.status}` };
  return out;
}

// ───────────────────────────── server management ─────────────────────────────
const live = new Set();
const isDir = (p) => {
  try {
    return fs.statSync(p).isDirectory();
  } catch {
    return false;
  }
};
function binPath(version) {
  return path.join(CFG.runtimeRoot, version, "ollama.exe");
}
function assertPort(port) {
  if (port < 11450 || port > 11459) throw new Error(`port ${port} outside the allowed range 11450-11459`);
}

function buildEnv(version, port) {
  const runtimeDir = path.dirname(binPath(version));
  const libDir = path.join(runtimeDir, "lib", "ollama");
  const depDirs = [runtimeDir, libDir, path.join(libDir, "cuda_v13"), path.join(libDir, "cuda_v12"), path.join(libDir, "vulkan")].filter(isDir);
  const env = {
    ...process.env,
    OLLAMA_HOST: `127.0.0.1:${port}`,
    OLLAMA_NO_CLOUD: "1",
    OLLAMA_KEEP_ALIVE: "90s", // app default server keep-alive (requests carry keep_alive:-1)
    OLLAMA_MODELS: CFG.modelsDir,
    OLLAMA_NOPRUNE: "1", // never let the test server delete blobs from the user's store
    PATH: [...depDirs, process.env.PATH ?? ""].join(path.delimiter),
  };
  if (isDir(libDir)) env.OLLAMA_RUNNERS_DIR = libDir;
  return { env, runtimeDir };
}

/** Spawns `ollama serve` like the app does (cwd = runtime dir, hidden window). */
function startServer(version, port, tag, { logTag } = {}) {
  assertPort(port);
  const bin = binPath(version);
  if (!fs.existsSync(bin)) throw new Error(`missing binary ${bin}`);
  const { env, runtimeDir } = buildEnv(version, port);
  fs.mkdirSync(CFG.logDir, { recursive: true });
  const logPath = path.join(CFG.logDir, `${logTag ?? tag}-${version}.log`);
  const fd = fs.openSync(logPath, "a");
  fs.writeSync(fd, `\n===== ${new Date().toISOString()} start ${version} port ${port} (${tag}) =====\n`);
  const srv = { version, port, tag, logPath, exit: null, spawnedAt: now(), startedIso: new Date().toISOString() };
  const child = spawn(bin, ["serve"], { cwd: runtimeDir, env, stdio: ["ignore", fd, fd], windowsHide: true });
  srv.child = child;
  srv.pid = child.pid;
  srv.exitPromise = new Promise((resolve) => {
    child.on("exit", (code, signal) => {
      srv.exit = { code, signal, at: now() };
      owned.delete(child.pid);
      live.delete(srv);
      try { fs.closeSync(fd); } catch { /* already closed */ }
      writePidFile();
      resolve(srv.exit);
    });
    child.on("error", (e) => {
      srv.exit = { code: -1, error: String(e), at: now() };
      resolve(srv.exit);
    });
  });
  owned.set(child.pid, { tag, version, port, started: srv.startedIso, child });
  live.add(srv);
  writePidFile();
  return srv;
}

/**
 * Model of the app's foreground wait (ollama_runtime.rs). Attempt k runs at about k*500 ms:
 *   ping_ollama_quick (300 ms timeouts; a failure is cached for 3 s) || (k % 4 == 3 && ping_ollama)
 * ping_ollama uses the shared agent with a 60 s read timeout. These servers accept connections after ~0.1 s but
 * hold /api/tags until GPU discovery has finished (phase 7 measures this), so the full ping of attempt 3 (t = 1.5 s)
 * blocks until the server answers: the app notices readiness at max(real ready, 1.5 s). Nothing bounds that wait by
 * the 12 s deadline, because the deadline is only checked after an unsuccessful probe.
 * app_ready_failfast_model_ms is the older, more pessimistic model (the full ping fails immediately), kept for comparison.
 */
function appReadySim(readyAtMs) {
  const blocking = Math.max(readyAtMs, 1500);
  let lastProbe = -Infinity;
  let failfast = null;
  for (let k = 0; failfast === null; k++) {
    const t = k * 500;
    let reachable = false;
    if (t - lastProbe >= 3000) { reachable = t >= readyAtMs; lastProbe = t; }
    if (!reachable && k % 4 === 3) reachable = t >= readyAtMs;
    if (reachable) failfast = t;
    if (t > 120_000) break;
  }
  return {
    app_ready_ms: Math.round(blocking),
    within_foreground_wait: blocking <= STARTUP_FOREGROUND_WAIT_MS,
    app_ready_failfast_model_ms: failfast,
  };
}

/** Polls /api/tags (300 ms timeout like ping_ollama_quick) every 100 ms. */
async function waitReady(srv, deadlineMs = 60_000) {
  let attempts = 0;
  for (;;) {
    if (srv.exit) return { ready: false, exited: true, exit: srv.exit, ms: r1(now() - srv.spawnedAt) };
    attempts++;
    const r = await request(srv.port, "GET", "/api/tags", undefined, 300);
    if (r.ok) {
      const ms = now() - srv.spawnedAt;
      return { ready: true, ms: r1(ms), attempts, ...appReadySim(ms), models: r.json?.models?.length ?? null };
    }
    if (now() - srv.spawnedAt > deadlineMs) return { ready: false, timed_out: true, ms: r1(now() - srv.spawnedAt), attempts };
    await sleep(100);
  }
}

const killLog = []; // every process this run ended on purpose: proof that only own PIDs were touched
async function taskkill(pid, tree = true) {
  if (protectedPids.has(pid)) throw new Error(`refusing to kill protected pid ${pid}`);
  if (!owned.has(pid)) throw new Error(`refusing to kill pid ${pid}: not spawned by this run`);
  killLog.push({ at: new Date().toISOString(), pid, tree, tag: owned.get(pid).tag, version: owned.get(pid).version, port: owned.get(pid).port });
  const t0 = now();
  return new Promise((resolve) => {
    const a = ["/PID", String(pid), ...(tree ? ["/T"] : []), "/F"];
    const p = spawn("taskkill", a, { windowsHide: true, stdio: "ignore" });
    p.on("exit", (code) => resolve({ code, ms: r1(now() - t0) }));
    p.on("error", (e) => resolve({ code: -1, error: String(e), ms: r1(now() - t0) }));
  });
}

async function bringUp(version, port, tag, opts) {
  if (await tcpInUse(port)) throw new Error(`port ${port} already in use before start`);
  const srv = startServer(version, port, tag, opts);
  const ready = await waitReady(srv);
  if (!ready.ready) throw new Error(`server ${version} on ${port} did not become ready: ${JSON.stringify(ready)}`);
  const ver = await request(port, "GET", "/api/version", undefined, 2000);
  return { srv, ready, version: ver.json?.version ?? null };
}

async function stopServer(srv) {
  if (srv.exit) return { already_exited: true };
  const t0 = now();
  const kill = await taskkill(srv.pid, true);
  await srv.exitPromise;
  const portFreeMs = await waitPortFree(srv.port, t0);
  return { taskkill_ms: kill.ms, exit_ms: r1(srv.exit.at - t0), port_free_ms: portFreeMs };
}

function cleanupAllSync() {
  for (const srv of [...live]) {
    if (srv.exit || !srv.pid || protectedPids.has(srv.pid)) continue;
    spawnSync("taskkill", ["/PID", String(srv.pid), "/T", "/F"], { windowsHide: true, stdio: "ignore" });
  }
  for (const [pid] of owned) {
    if (protectedPids.has(pid)) continue;
    spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], { windowsHide: true, stdio: "ignore" });
  }
}
process.on("exit", cleanupAllSync);
process.on("SIGINT", () => { cleanupAllSync(); process.exit(130); });
process.on("unhandledRejection", (e) => { console.error("unhandledRejection", e); cleanupAllSync(); process.exit(1); });

async function loadedModels(port) {
  const r = await request(port, "GET", "/api/ps", undefined, 3000);
  return r.ok ? r.json?.models ?? [] : null;
}
async function unloadModel(port, model = CFG.model) {
  const t0 = now();
  const r = await request(port, "POST", "/api/chat", { model, keep_alive: 0, messages: [], stream: false }, 30_000);
  let goneMs = null;
  while (now() - t0 < 30_000) {
    const m = await loadedModels(port);
    if (m && m.length === 0) { goneMs = r1(now() - t0); break; }
    await sleep(50);
  }
  return { http_ok: r.ok, http_ms: r1(r.ms), gone_ms: goneMs, error: r.error ?? null };
}
const modelInfo = (m) => (m ? { name: m.name ?? m.model ?? null, size_mb: mb(m.size), size_vram_mb: mb(m.size_vram), context_length: m.context_length ?? null, expires_at: m.expires_at ?? null } : null);

const sample = (srv, label, extra = {}) => {
  const t0 = now();
  const raw = sampleRaw(srv.pid);
  return { label, ...extra, sample_ms: r1(now() - t0), ...summarize(raw, srv.pid) };
};

// ───────────────────────────── model store snapshot ─────────────────────────────
function snapshotStore(dir) {
  const files = {};
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { files[path.relative(dir, p) + "\\"] = { dir: true }; walk(p); }
      else { const st = fs.statSync(p); files[path.relative(dir, p)] = { size: st.size, mtime_ms: Math.round(st.mtimeMs) }; }
    }
  };
  walk(dir);
  const blobs = Object.entries(files).filter(([k, v]) => k.startsWith("blobs\\") && !v.dir);
  return {
    taken: new Date().toISOString(),
    blobs: { count: blobs.length, bytes: blobs.reduce((s, [, v]) => s + v.size, 0) },
    file_count: Object.values(files).filter((v) => !v.dir).length,
    files,
  };
}
function diffStore(a, b) {
  const added = [];
  const removed = [];
  const changed = [];
  for (const k of Object.keys(b.files)) {
    if (!(k in a.files)) added.push(k);
    else if (!b.files[k].dir && (a.files[k].size !== b.files[k].size || a.files[k].mtime_ms !== b.files[k].mtime_ms)) {
      changed.push({ file: k, size: [a.files[k].size, b.files[k].size], mtime_changed: a.files[k].mtime_ms !== b.files[k].mtime_ms });
    }
  }
  for (const k of Object.keys(a.files)) if (!(k in b.files)) removed.push(k);
  return { added, removed, changed, blobs_before: a.blobs, blobs_after: b.blobs };
}
const storeBrief = (s) => ({ taken: s.taken, blobs: s.blobs, file_count: s.file_count });

// ───────────────────────────── results file ─────────────────────────────
let results = {};
function loadResults() {
  try { results = JSON.parse(fs.readFileSync(CFG.out, "utf8")); } catch { results = {}; }
  results.phases ??= {};
  results.runs ??= [];
}
function saveResults() {
  fs.mkdirSync(path.dirname(CFG.out), { recursive: true });
  const tmp = `${CFG.out}.tmp`;
  fs.writeFileSync(tmp, `${JSON.stringify(results, null, 2)}\n`);
  fs.renameSync(tmp, CFG.out);
}
async function runPhase(key, fn) {
  log(`=== phase ${key} start`);
  const t0 = now();
  const started = new Date().toISOString();
  let res;
  try { res = await fn(); } catch (e) { res = { aborted: true, error: String(e?.stack ?? e) }; log(`phase ${key} ABORTED: ${e?.message}`); }
  results.phases[key] = { started, duration_s: r1((now() - t0) / 1000), ...res };
  saveResults();
  log(`=== phase ${key} done in ${r1((now() - t0) / 1000)} s`);
}

const reqRecord = (i, sIdx, r) => {
  const rec = { i, s: sIdx, ms: r.ms, ok: r.ok, pass: r.quality.pass, load_ms: r.load_ms ?? null, eval_n: r.eval_count ?? null, pe_n: r.prompt_eval_count ?? null, cpu: r.cpu_pct };
  if (!r.quality.pass) { rec.why = r.quality.reason; rec.out = (r.content ?? r.body_head ?? "").slice(0, 300); if (r.error) rec.error = r.error; }
  else if (i <= SENTENCES.length) { rec.in = SENTENCES[sIdx]; rec.out = (r.content ?? "").slice(0, 300); rec.kept_pct = r.quality.kept_pct; } // one sample per sentence
  return rec;
};
const slim = (r) => ({ ms: r.ms, cpu_pct: r.cpu_pct, ok: r.ok, pass: r.quality.pass, why: r.quality.pass ? undefined : r.quality.reason, load_ms: r.load_ms ?? null, total_ms: r.total_ms ?? null, error: r.error ?? r.server_error ?? null, out: r.ok && !r.quality.pass ? (r.content ?? "").slice(0, 200) : undefined });

// ───────────────────────────── phase 1: soak ─────────────────────────────
async function phase1() {
  const { srv, ready, version } = await bringUp(CFG.vNew, PORTS.p1, "p1");
  const res = { port: PORTS.p1, version_reported: version, server_ready: ready, requests: [], samples: [] };
  res.samples.push(sample(srv, "before first request", { at_request: 0 }));
  const recs = res.requests;
  const lat = [];
  for (let i = 1; i <= CFG.requests; i++) {
    const sIdx = (i - 1) % SENTENCES.length;
    const r = await chat(PORTS.p1, SENTENCES[sIdx], { keep_alive: -1 });
    recs.push(reqRecord(i, sIdx, r));
    lat.push(r.ms);
    if (i === 1 || i % CFG.sampleEvery === 0 || i === CFG.requests) {
      const models = await loadedModels(PORTS.p1);
      res.samples.push(sample(srv, `after request ${i}`, { at_request: i, model: modelInfo(models?.[0]), ps_entries: models?.length ?? null, ps_raw_first: i === 1 ? models : undefined, last_request_ms: r.ms }));
      log(`p1 ${i}/${CFG.requests} last=${r.ms} ms ok=${recs.filter((x) => x.pass).length}/${recs.length}`);
    }
    if (!r.ok && recs.slice(-5).every((x) => !x.ok)) { res.stopped_early = `5 consecutive failures at request ${i}`; break; }
  }
  const warm = recs.slice(1);
  const warmMs = warm.map((x) => x.ms);
  const p50 = stats(warmMs)?.p50 ?? 0;
  res.latency_ms_all = stats(recs.map((x) => x.ms));
  res.latency_ms_warm_excl_first = stats(warmMs);
  res.first_request = recs[0];
  res.outliers = {
    over_10s: recs.filter((x) => x.ms > 10_000).map((x) => ({ i: x.i, ms: x.ms, load_ms: x.load_ms, cpu: x.cpu })),
    over_3x_p50_warm: warm.filter((x) => x.ms > 3 * p50).map((x) => ({ i: x.i, ms: x.ms, load_ms: x.load_ms, eval_n: x.eval_n, cpu: x.cpu })),
    top5_slowest_warm: [...warm].sort((a, b) => b.ms - a.ms).slice(0, 5).map((x) => ({ i: x.i, ms: x.ms, load_ms: x.load_ms, eval_n: x.eval_n, pe_n: x.pe_n, cpu: x.cpu })),
    reloads_after_first: warm.filter((x) => (x.load_ms ?? 0) > 500).map((x) => ({ i: x.i, ms: x.ms, load_ms: x.load_ms })),
  };
  const total = recs.length;
  const errors = recs.filter((x) => !x.ok);
  const empty = recs.filter((x) => x.ok && /empty|only a thinking/.test(x.why ?? ""));
  res.quality = {
    total, errors: errors.length, empty: empty.length,
    quality_fail: recs.filter((x) => x.ok && !x.pass).length,
    pass: recs.filter((x) => x.pass).length,
    pass_pct: r1((100 * recs.filter((x) => x.pass).length) / total),
    failures: recs.filter((x) => !x.pass).slice(0, 20),
  };
  const ser = (fn) => res.samples.filter((s) => s.at_request >= 1).map((s) => ({ x: s.at_request, v: fn(s) }));
  const warmSer = (fn) => res.samples.filter((s) => s.at_request >= 25).map((s) => ({ x: s.at_request, v: fn(s) }));
  const metrics = {
    server_rss_mb: (s) => s.server?.ws_mb, server_private_mb: (s) => s.server?.priv_mb,
    server_handles: (s) => s.server?.handles, server_threads: (s) => s.server?.threads,
    runner_rss_mb: (s) => s.runners.ws_mb, runner_private_mb: (s) => s.runners.priv_mb,
    runner_handles: (s) => s.runners.handles, runner_threads: (s) => s.runners.threads,
    runner_gpu_dedicated_mb: (s) => s.runners.gpu_dedicated_mb, runner_gpu_shared_mb: (s) => s.runners.gpu_shared_mb,
    nvidia_smi_used_mb: (s) => s.nvidia_smi?.used_mb,
  };
  res.trend_from_request_1 = Object.fromEntries(Object.entries(metrics).map(([k, fn]) => [k, trend(ser(fn))]));
  res.trend_from_request_25 = Object.fromEntries(Object.entries(metrics).map(([k, fn]) => [k, trend(warmSer(fn))]));
  res.runner_pid_sets = res.samples.map((s) => ({ at: s.at_request, pids: s.runners.pids }));
  res.runner_restarted = new Set(res.samples.filter((s) => s.at_request >= 1).map((s) => s.runners.pids.join(","))).size > 1;
  res.teardown = await stopServer(srv);
  return res;
}

// ───────────────────────────── phase 2: load / unload cycles ─────────────────────────────
/** GPU memory of llama-server runners that are NOT part of this harness's server tree (the user's app runner). */
const foreignLlamaGpu = (s) => r1(s.others.filter((o) => /llama-server/.test(o.name)).reduce((a, o) => a + (o.gpu_dedicated_mb ?? 0), 0));

async function phase2() {
  const { srv, ready, version } = await bringUp(CFG.vNew, PORTS.p2, "p2");
  const res = { port: PORTS.p2, version_reported: version, server_ready: ready, cycles: [] };
  res.baseline_no_model = sample(srv, "server up, no model");
  const first = await chat(PORTS.p2, SENTENCES[0]);
  res.initial_load = { ...slim(first), runner_after: sample(srv, "initial load").runners };
  const nvBase = res.baseline_no_model.nvidia_smi?.used_mb ?? null;
  for (let c = 1; c <= CFG.cycles; c++) {
    const cyc = { cycle: c };
    const doSample = CFG.cycleSampleEvery <= 1 || c === 1 || c === CFG.cycles || c % CFG.cycleSampleEvery === 0;
    cyc.unload = await unloadModel(PORTS.p2);
    const afterUnload = doSample ? sample(srv, "after unload") : null;
    if (afterUnload) cyc.after_unload = { foreign_llama_gpu_mb: foreignLlamaGpu(afterUnload), runner_pids: afterUnload.runners.pids, runner_gpu_dedicated_mb: afterUnload.runners.gpu_dedicated_mb, runner_rss_mb: afterUnload.runners.ws_mb, server_rss_mb: afterUnload.server?.ws_mb, server_handles: afterUnload.server?.handles, server_threads: afterUnload.server?.threads, nvidia_used_mb: afterUnload.nvidia_smi?.used_mb, busy: afterUnload.busy_processes };
    const r = await chat(PORTS.p2, SENTENCES[c % SENTENCES.length]);
    cyc.load = slim(r);
    const models = await loadedModels(PORTS.p2);
    const afterLoad = doSample ? sample(srv, "after load") : null;
    if (afterLoad) cyc.after_load = { foreign_llama_gpu_mb: foreignLlamaGpu(afterLoad), model: modelInfo(models?.[0]), runner_pids: afterLoad.runners.pids, runner_gpu_dedicated_mb: afterLoad.runners.gpu_dedicated_mb, runner_rss_mb: afterLoad.runners.ws_mb, runner_handles: afterLoad.runners.handles, runner_threads: afterLoad.runners.threads, server_rss_mb: afterLoad.server?.ws_mb, server_handles: afterLoad.server?.handles, server_threads: afterLoad.server?.threads, nvidia_used_mb: afterLoad.nvidia_smi?.used_mb };
    res.cycles.push(cyc);
    log(`p2 cycle ${c}/${CFG.cycles} unload_gone=${cyc.unload.gone_ms} ms cold=${r.ms} ms load=${r.load_ms} ok=${r.quality.pass}`);
  }
  const cs = res.cycles;
  const sampled = cs.filter((c) => c.after_unload && c.after_load);
  res.cycles_sampled = sampled.length;
  res.cold_start_ms = stats(cs.map((c) => c.load.ms));
  res.cold_start_load_duration_ms = stats(cs.map((c) => c.load.load_ms));
  res.unload_ms = stats(cs.map((c) => c.unload.gone_ms));
  res.errors = cs.filter((c) => !c.load.ok || !c.unload.http_ok || c.unload.gone_ms === null).length;
  res.quality_fail = cs.filter((c) => c.load.ok && !c.load.pass).length;
  res.after_unload_runner_present = sampled.filter((c) => c.after_unload.runner_pids.length > 0).length;
  res.after_unload_runner_gpu_mb = stats(sampled.map((c) => c.after_unload.runner_gpu_dedicated_mb));
  res.nvidia_smi_baseline_no_model_mb = nvBase;
  res.nvidia_smi_after_unload_minus_baseline_mb = stats(sampled.map((c) => (c.after_unload.nvidia_used_mb ?? NaN) - nvBase));
  res.nvidia_smi_after_load_minus_baseline_mb = stats(sampled.map((c) => (c.after_load.nvidia_used_mb ?? NaN) - nvBase));
  const ser = (fn) => sampled.map((c) => ({ x: c.cycle, v: fn(c) }));
  res.trend = {
    runner_gpu_dedicated_after_load_mb: trend(ser((c) => c.after_load.runner_gpu_dedicated_mb)),
    runner_rss_after_load_mb: trend(ser((c) => c.after_load.runner_rss_mb)),
    runner_handles_after_load: trend(ser((c) => c.after_load.runner_handles)),
    runner_threads_after_load: trend(ser((c) => c.after_load.runner_threads)),
    server_rss_mb: trend(ser((c) => c.after_load.server_rss_mb)),
    server_handles: trend(ser((c) => c.after_load.server_handles)),
    server_threads: trend(ser((c) => c.after_load.server_threads)),
    nvidia_used_after_unload_mb: trend(ser((c) => c.after_unload.nvidia_used_mb)),
    nvidia_used_after_load_mb: trend(ser((c) => c.after_load.nvidia_used_mb)),
    // total minus the other llama-server runners (user's app): what is left is desktop + whisper + my own runner
    nvidia_used_after_unload_excl_foreign_runner_mb: trend(ser((c) => c.after_unload.nvidia_used_mb - c.after_unload.foreign_llama_gpu_mb)),
    nvidia_used_after_load_excl_foreign_runner_mb: trend(ser((c) => c.after_load.nvidia_used_mb - c.after_load.foreign_llama_gpu_mb)),
    cold_start_ms: trend(cs.map((c) => ({ x: c.cycle, v: c.load.ms }))),
  };
  res.teardown = await stopServer(srv);
  return res;
}

// ───────────────────────────── phase 3: server abort ─────────────────────────────
async function descendantsOf(pid) {
  const raw = sampleRaw(pid);
  return raw.mine.filter((p) => p.pid !== pid).map((p) => ({ pid: p.pid, name: p.name, start: p.start }));
}
function pidAlive(pid) {
  try { process.kill(pid, 0); return true; } catch { return false; }
}
/** Kills leftover processes recorded earlier; only if pid + creation date still match and not protected. */
function killRecorded(list) {
  const killed = [];
  for (const p of list) {
    if (protectedPids.has(p.pid) || !pidAlive(p.pid)) continue;
    const out = ps(`$p = Get-CimInstance Win32_Process -Filter "ProcessId=${p.pid}"; if ($p -and $p.CreationDate) { $p.CreationDate.ToString('o') }`);
    if (out && out === p.start) {
      killLog.push({ at: new Date().toISOString(), pid: p.pid, tree: false, tag: "orphan-of-own-server", name: p.name });
      spawnSync("taskkill", ["/PID", String(p.pid), "/F"], { windowsHide: true, stdio: "ignore" });
      killed.push(p.pid);
    }
  }
  return killed;
}

async function restartAfterKill(version, port, tag, waitForPortFirst) {
  const row = {};
  if (waitForPortFirst) {
    const t0 = now();
    row.port_wait_before_spawn_ms = (await waitPortFree(port, t0, PORT_RELEASE_WAIT_MS, 300)) ?? ">10000";
  }
  const srv = startServer(version, port, tag);
  const ready = await waitReady(srv);
  row.ready = ready;
  if (!ready.ready) {
    // bind failure (port still held) or slow start: retry after the port is free, as the app would on its next start
    row.first_attempt_failed = { exit: srv.exit ?? null, ready_ms: ready.ms };
    if (!srv.exit) await stopServer(srv);
    await waitPortFree(port, now(), PORT_RELEASE_WAIT_MS, 100);
    const srv2 = startServer(version, port, tag);
    row.ready = await waitReady(srv2);
    return { srv: srv2, row };
  }
  return { srv, row };
}

async function phase3() {
  const port = PORTS.p3;
  const res = { port, kill_mid_generation: [], kill_during_load: [], kill_server_only_orphan_check: [] };
  let { srv, ready, version } = await bringUp(CFG.vNew, port, "p3");
  res.initial = { ready, version };
  // calibrate the long request so the kill lands inside the generation
  const KILL_TEXT = [LONG_TEXT, LONG_TEXT, LONG_TEXT].join(" ");
  await chat(port, SENTENCES[0]);
  const calib = [];
  for (let i = 0; i < 3; i++) calib.push((await chat(port, KILL_TEXT)).ms);
  const tLong = calib.sort((a, b) => a - b)[1];
  const killDelay = Math.max(150, Math.round(0.3 * tLong));
  res.calibration = { kill_text_words: KILL_TEXT.split(/\s+/).length, kill_request_warm_ms: tLong, kill_delay_ms: killDelay };
  res.discarded_attempts = [];

  // A: warm server, kill (taskkill /T /F like the app) mid-generation. An attempt only counts when the
  // in-flight request really was cut off; otherwise it is recorded under discarded_attempts and repeated.
  for (let rep = 1; rep <= CFG.killReps; rep++) {
   for (let attempt = 1; attempt <= 3; attempt++) {
    const row = { rep, attempt, variant: "mid_generation" };
    const desc = await descendantsOf(srv.pid);
    row.runner_pids_before = desc.map((d) => d.pid);
    const inflight = chat(port, KILL_TEXT, { timeoutMs: 60_000 });
    await sleep(killDelay);
    const tKill = now();
    const k = await taskkill(srv.pid, true);
    await srv.exitPromise;
    row.taskkill_ms = k.ms;
    row.server_exit_ms = r1(srv.exit.at - tKill);
    row.port_free_ms = await waitPortFree(port, tKill);
    const infl = await inflight;
    row.inflight = { ok: infl.ok, error: infl.error, status: infl.status, ms_until_failure: infl.ms, partial_body: infl.body_head ?? null };
    row.orphans_after_kill = desc.filter((d) => pidAlive(d.pid)).map((d) => d.pid);
    if (row.orphans_after_kill.length) row.orphans_killed_by_harness = killRecorded(desc);
    const wait = rep % 2 === 0; // alternate: app-style wait for the port vs spawn immediately
    row.spawn_mode = wait ? "wait_for_port_free_then_spawn (app logic)" : "spawn_immediately_after_exit";
    const rs = await restartAfterKill(CFG.vNew, port, "p3", wait);
    srv = rs.srv;
    Object.assign(row, { restart: rs.row });
    const rep1 = await chat(port, KILL_TEXT);
    row.repeat_request = slim(rep1);
    row.recovery_total_ms = r1(now() - tKill);
    log(`p3 A${rep}.${attempt}: portfree=${row.port_free_ms} taskkill=${row.taskkill_ms} ready=${rs.row.ready.ms} (app~${rs.row.ready.app_ready_ms}) repeat=${rep1.ms} ok=${rep1.quality.pass} inflight=${infl.error}`);
    if (!infl.ok) { res.kill_mid_generation.push(row); break; }
    row.discarded_reason = "in-flight request finished before the kill took effect";
    res.discarded_attempts.push(row);
   }
  }

  // B: kill during the cold model load
  for (let rep = 1; rep <= CFG.killLoadReps; rep++) {
    const row = { rep, variant: "during_model_load" };
    const un = await unloadModel(port);
    row.unload_before = un.gone_ms;
    const desc = await descendantsOf(srv.pid);
    const inflight = chat(port, SENTENCES[rep % SENTENCES.length], { timeoutMs: 60_000 });
    await sleep(800);
    const tKill = now();
    const k = await taskkill(srv.pid, true);
    await srv.exitPromise;
    row.taskkill_ms = k.ms;
    row.port_free_ms = await waitPortFree(port, tKill);
    const infl = await inflight;
    row.inflight = { ok: infl.ok, error: infl.error, ms_until_failure: infl.ms };
    row.orphans_after_kill = desc.filter((d) => pidAlive(d.pid)).map((d) => d.pid);
    if (row.orphans_after_kill.length) row.orphans_killed_by_harness = killRecorded(desc);
    const rs = await restartAfterKill(CFG.vNew, port, "p3", true);
    srv = rs.srv;
    row.restart = rs.row;
    const rep1 = await chat(port, SENTENCES[rep % SENTENCES.length]);
    row.repeat_request = slim(rep1);
    row.recovery_total_ms = r1(now() - tKill);
    res.kill_during_load.push(row);
    log(`p3 B${rep}: portfree=${row.port_free_ms} ready=${rs.row.ready.ms} repeat=${rep1.ms} ok=${rep1.quality.pass}`);
  }

  // C: kill only the server process (no /T): does the runner linger, hold the GPU, block the restart?
  for (let rep = 1; rep <= CFG.killOrphanReps; rep++) {
    const row = { rep, variant: "server_only_kill (no /T)" };
    await chat(port, SENTENCES[0]);
    const desc = await descendantsOf(srv.pid);
    row.runner_pids_before = desc.map((d) => d.pid);
    const tKill = now();
    const k = await taskkill(srv.pid, false);
    await srv.exitPromise;
    row.taskkill_ms = k.ms;
    row.port_free_ms = await waitPortFree(port, tKill);
    const watch = [];
    let runnerGoneMs = null;
    for (let t = 0; t < 15_000; t += 500) {
      const alive = desc.filter((d) => pidAlive(d.pid)).map((d) => d.pid);
      watch.push([t, alive.length]);
      if (!alive.length) { runnerGoneMs = r1(now() - tKill); break; }
      await sleep(500);
    }
    row.runner_survived_server_kill = desc.some((d) => pidAlive(d.pid));
    row.runner_gone_ms_after_kill = runnerGoneMs;
    if (row.runner_survived_server_kill) {
      const orph = sampleRaw(desc.map((d) => d.pid).filter((p) => pidAlive(p)));
      row.orphan_runner = orph.mine.map((p) => ({ pid: p.pid, name: p.name, ws_mb: mb(p.ws), gpu_dedicated_mb: mb(p.gpu_ded) }));
      row.nvidia_used_mb_with_orphan = orph.nvidia?.used_mb ?? null;
      // restart while the orphan still exists: does the new server work next to it?
      const rs = await restartAfterKill(CFG.vNew, port, "p3", true);
      srv = rs.srv;
      row.restart_with_orphan = rs.row;
      const r = await chat(port, SENTENCES[1]);
      row.repeat_request_with_orphan = slim(r);
      const m = await loadedModels(port);
      row.model_with_orphan = modelInfo(m?.[0]);
      row.orphans_killed_by_harness = killRecorded(desc);
    } else {
      const rs = await restartAfterKill(CFG.vNew, port, "p3", true);
      srv = rs.srv;
      row.restart = rs.row;
      row.repeat_request = slim(await chat(port, SENTENCES[1]));
    }
    res.kill_server_only_orphan_check.push(row);
    log(`p3 C${rep}: runner_survived=${row.runner_survived_server_kill} gone=${runnerGoneMs}`);
  }

  const A = res.kill_mid_generation;
  const B = res.kill_during_load;
  const all = [...A, ...B];
  res.summary = {
    port_free_ms_after_kill_A: stats(A.map((r) => r.port_free_ms)),
    port_free_ms_after_kill_A_and_B: stats(all.map((r) => r.port_free_ms)),
    taskkill_ms: stats(all.map((r) => r.taskkill_ms)),
    restart_ready_ms_real_A: stats(A.map((r) => r.restart.ready.ms)),
    restart_ready_ms_real_B: stats(B.map((r) => r.restart.ready.ms)),
    restart_ready_ms_app_equivalent_A: stats(A.map((r) => r.restart.ready.app_ready_ms)),
    recovery_total_ms_A: stats(A.map((r) => r.recovery_total_ms)),
    recovery_total_ms_B: stats(B.map((r) => r.recovery_total_ms)),
    repeat_request_ms_A: stats(A.map((r) => r.repeat_request.ms)),
    repeat_request_ok_A: A.filter((r) => r.repeat_request.pass).length,
    repeat_request_ok_B: B.filter((r) => r.repeat_request.pass).length,
    reps_A: A.length,
    reps_B: B.length,
    over_foreground_wait_12s: all.filter((r) => !r.restart.ready.within_foreground_wait).length,
    first_attempt_failures: all.filter((r) => r.restart.first_attempt_failed).length,
    orphans_after_taskkill_tree: all.filter((r) => r.orphans_after_kill?.length).length,
    inflight_errors_A: A.map((r) => r.inflight.error).reduce((m, e) => ((m[e ?? "none"] = (m[e ?? "none"] ?? 0) + 1), m), {}),
    inflight_errors_B: B.map((r) => r.inflight.error).reduce((m, e) => ((m[e ?? "none"] = (m[e ?? "none"] ?? 0) + 1), m), {}),
  };
  res.teardown = await stopServer(srv);
  return res;
}

// ───────────────────────────── phase 4: version switch on the same port ─────────────────────────────
async function secondStartOnBusyPort(version, port, rep) {
  const t0 = now();
  const srv = startServer(version, port, "p4-second", { logTag: `p4-second-${rep}` });
  const exitInfo = await Promise.race([srv.exitPromise, sleep(15_000).then(() => null)]);
  const row = { binary: version, exited: !!exitInfo, exit_code: exitInfo?.code ?? null, exit_after_ms: exitInfo ? r1(exitInfo.at - t0) : null };
  if (!exitInfo) { row.had_to_be_killed = true; await stopServer(srv); }
  let tail = "";
  try { tail = fs.readFileSync(srv.logPath, "utf8").split("\n").filter(Boolean).slice(-6).join(" | ").slice(-500); } catch { /* no log */ }
  row.log_tail = tail;
  row.bind_error_logged = /only one usage|address already in use|bind:/i.test(tail);
  return row;
}

async function phase4() {
  const port = PORTS.p4;
  const res = { port, iterations: [] };
  for (let rep = 1; rep <= CFG.switchReps; rep++) {
    const it = { rep };
    if (await tcpInUse(port)) throw new Error(`port ${port} busy at iteration ${rep}`);
    // old
    const A = startServer(CFG.vOld, port, "p4", { logTag: "p4-old" });
    it.old = { ready: await waitReady(A) };
    if (!it.old.ready.ready) { it.old.failed = true; await stopServer(A); res.iterations.push(it); continue; }
    it.old.version = (await request(port, "GET", "/api/version", undefined, 2000)).json?.version ?? null;
    const tags = await request(port, "GET", "/api/tags", undefined, 2000);
    it.old.tags = { ok: tags.ok, ms: r1(tags.ms), models: tags.json?.models?.length ?? null };
    const oc = await chat(port, SENTENCES[rep % SENTENCES.length]);
    it.old.chat = slim(oc);
    // stop old like the app (taskkill /T /F), then start the new one on the same port
    const tKill = now();
    await taskkill(A.pid, true);
    await A.exitPromise;
    it.old.exit_ms = r1(A.exit.at - tKill);
    it.port_in_use_right_after_exit = await tcpInUse(port);
    const immediate = rep % 2 === 1;
    let B;
    if (immediate) {
      it.spawn_mode = "immediately after old process exit";
      B = startServer(CFG.vNew, port, "p4", { logTag: "p4-new" });
    } else {
      it.spawn_mode = "after port-free poll (app logic)";
      it.port_free_ms_after_kill = await waitPortFree(port, tKill, PORT_RELEASE_WAIT_MS, 20);
      B = startServer(CFG.vNew, port, "p4", { logTag: "p4-new" });
    }
    let ready = await waitReady(B);
    if (!ready.ready) {
      it.new_first_attempt = { failed: true, exit: B.exit ?? null, ready };
      if (!B.exit) await stopServer(B);
      await waitPortFree(port, now(), PORT_RELEASE_WAIT_MS, 100);
      B = startServer(CFG.vNew, port, "p4", { logTag: "p4-new" });
      ready = await waitReady(B);
    }
    it.new = { ready };
    if (!ready.ready) { res.iterations.push(it); await stopServer(B); continue; }
    it.new.version = (await request(port, "GET", "/api/version", undefined, 2000)).json?.version ?? null;
    const t2 = await request(port, "GET", "/api/tags", undefined, 2000);
    it.new.tags = { ok: t2.ok, ms: r1(t2.ms), models: t2.json?.models?.length ?? null };
    const nc = await chat(port, SENTENCES[rep % SENTENCES.length]);
    it.new.chat = slim(nc);
    it.switch_total_ms = r1(now() - tKill);
    // a second start on the busy port must fail cleanly and leave the first server alone
    it.second_start = await secondStartOnBusyPort(rep % 2 === 0 ? CFG.vOld : CFG.vNew, port, rep);
    const after = await request(port, "GET", "/api/version", undefined, 2000);
    it.second_start.first_server_still_answers = after.ok;
    it.second_start.version_unchanged = after.json?.version === it.new.version;
    it.second_start.first_server_alive = !B.exit;
    it.teardown = await stopServer(B);
    res.iterations.push(it);
    log(`p4 #${rep}: old ready=${it.old.ready.ms} v=${it.old.version} chat=${oc.ms}/${oc.quality.pass ? "ok" : oc.quality.reason} | new ready=${ready.ms} v=${it.new.version} chat=${nc.ms}/${nc.quality.pass ? "ok" : "fail"} | 2nd exit=${it.second_start.exit_code}`);
  }
  const its = res.iterations;
  res.summary = {
    iterations: its.length,
    old_ready_ms: stats(its.map((i) => i.old?.ready?.ms)),
    new_ready_ms: stats(its.map((i) => i.new?.ready?.ms)),
    new_ready_app_equivalent_ms: stats(its.map((i) => i.new?.ready?.app_ready_ms)),
    switch_total_ms: stats(its.map((i) => i.switch_total_ms)),
    port_free_ms_after_kill_wait_mode: stats(its.filter((i) => i.spawn_mode.startsWith("after")).map((i) => i.port_free_ms_after_kill)),
    versions_old: [...new Set(its.map((i) => i.old?.version))],
    versions_new: [...new Set(its.map((i) => i.new?.version))],
    old_chat_ok: its.filter((i) => i.old?.chat?.pass).length,
    old_chat_errors: [...new Set(its.filter((i) => i.old?.chat && !i.old.chat.pass).map((i) => i.old.chat.why ?? i.old.chat.error))],
    new_chat_ok: its.filter((i) => i.new?.chat?.pass).length,
    old_chat_ms: stats(its.map((i) => i.old?.chat?.ms)),
    new_chat_ms: stats(its.map((i) => i.new?.chat?.ms)),
    immediate_spawn_bind_failures: its.filter((i) => i.new_first_attempt?.failed).length,
    second_start_clean_failures: its.filter((i) => i.second_start?.exited && i.second_start.exit_code !== 0 && i.second_start.first_server_still_answers).length,
    second_start_not_clean: its.filter((i) => i.second_start && !(i.second_start.exited && i.second_start.exit_code !== 0 && i.second_start.first_server_still_answers)).length,
    second_start_exit_ms: stats(its.map((i) => i.second_start?.exit_after_ms)),
  };
  return res;
}

// ───────────────────────────── phase 5: parallel requests ─────────────────────────────
async function phase5() {
  const port = PORTS.p5;
  const { srv, ready, version } = await bringUp(CFG.vNew, port, "p5");
  const res = { port, version_reported: version, server_ready: ready, levels: {}, server_config_from_log: null };
  try {
    const logText = fs.readFileSync(srv.logPath, "utf8");
    const m = logText.match(/server config.*env="?map\[([^\n]*)\]/i) ?? logText.match(/OLLAMA_NUM_PARALLEL:?(\S*)/);
    res.server_config_from_log = m ? m[0].slice(0, 1500) : null;
  } catch { /* no log */ }
  await chat(port, SENTENCES[0]); // warm
  res.warm_model = modelInfo((await loadedModels(port))?.[0]);
  let idx = 0;
  for (const n of [1, 2, 4, 8]) {
    const rounds = Math.max(1, Math.round(CFG.parallelRounds / n));
    const lat = [];
    const walls = [];
    let fails = 0;
    let qfail = 0;
    const reloads = [];
    const failList = [];
    for (let r = 0; r < rounds; r++) {
      const t0 = now();
      const batch = await Promise.all(Array.from({ length: n }, (_, k) => { const s = (idx + k) % SENTENCES.length; return chat(port, SENTENCES[s]); }));
      idx += n;
      walls.push(now() - t0);
      for (const b of batch) {
        lat.push(b.ms);
        if (!b.ok) { fails++; failList.push(b.error ?? b.server_error ?? `HTTP ${b.status}`); }
        else if (!b.quality.pass) { qfail++; failList.push(b.quality.reason); }
        if ((b.load_ms ?? 0) > 500) reloads.push({ ms: b.ms, load_ms: b.load_ms });
      }
    }
    res.levels[n] = {
      concurrency: n, rounds, requests: lat.length,
      latency_ms: stats(lat), round_wall_ms: stats(walls),
      throughput_req_per_s: r1((lat.length / walls.reduce((s, v) => s + v, 0)) * 1000),
      errors: fails, quality_fail: qfail, fail_reasons: [...new Set(failList)].slice(0, 5),
      model_reloads: reloads, model_after: modelInfo((await loadedModels(port))?.[0]),
    };
    log(`p5 N=${n}: p50=${res.levels[n].latency_ms.p50} p95=${res.levels[n].latency_ms.p95} max=${res.levels[n].latency_ms.max} err=${fails} qfail=${qfail}`);
  }
  // light requests (what a model-update check and the UI send) next to running chats
  const light = async () => {
    const out = [];
    for (const p of ["/api/tags", "/api/version", "/api/ps"]) { const r = await request(port, "GET", p, undefined, 10_000); out.push({ p, ms: r.ms, ok: r.ok }); }
    return out;
  };
  const idle = [];
  for (let i = 0; i < 20; i++) idle.push(...(await light()));
  const during = [];
  const chatStats = [];
  for (let r = 0; r < 10; r++) {
    let running = true;
    const batch = Promise.all(Array.from({ length: 4 }, (_, k) => chat(port, SENTENCES[(idx + k) % SENTENCES.length]))).then((b) => { running = false; return b; });
    idx += 4;
    while (running) { during.push(...(await light())); await sleep(30); }
    chatStats.push(...(await batch).map((b) => b.ms));
  }
  const byEp = (rows) => Object.fromEntries(["/api/tags", "/api/version", "/api/ps"].map((p) => [p, stats(rows.filter((x) => x.p === p).map((x) => x.ms))]));
  res.light_requests = {
    idle_ms: byEp(idle),
    during_4_parallel_chats_ms: byEp(during),
    failures_idle: idle.filter((x) => !x.ok).length,
    failures_during: during.filter((x) => !x.ok).length,
    chat_latency_ms_in_this_scenario: stats(chatStats),
  };
  // cold parallel start: model unloaded, 4 requests at once
  res.cold_parallel_4 = [];
  for (let k = 0; k < 3; k++) {
    await unloadModel(port);
    const t0 = now();
    const batch = await Promise.all(Array.from({ length: 4 }, (_, j) => chat(port, SENTENCES[(k + j) % SENTENCES.length])));
    res.cold_parallel_4.push({ wall_ms: r1(now() - t0), latencies_ms: batch.map((b) => b.ms), ok: batch.filter((b) => b.quality.pass).length, load_ms: batch.map((b) => b.load_ms), model_after: modelInfo((await loadedModels(port))?.[0]) });
  }
  res.teardown = await stopServer(srv);
  return res;
}

// ───────────────────────────── phase 6: whisper neighbourhood (observation only) ─────────────────────────────
async function phase6() {
  const port = PORTS.p6;
  const { srv, ready, version } = await bringUp(CFG.vNew, port, "p6");
  const res = { port, version_reported: version, server_ready: ready, states: [] };
  const snap = (label, extra = {}) => {
    const s = sample(srv, label, extra);
    const ollamaOthers = s.others.filter((o) => /ollama|llama-server/.test(o.name));
    const whisper = s.others.filter((o) => /whisper-server/.test(o.name));
    return {
      label, ...extra,
      nvidia_smi: s.nvidia_smi,
      headroom_mb: s.nvidia_smi ? s.nvidia_smi.total_mb - s.nvidia_smi.used_mb : null,
      my_server_gpu_mb: s.server?.gpu_dedicated_mb, my_runner_gpu_mb: s.runners.gpu_dedicated_mb, my_runner_rss_mb: s.runners.ws_mb,
      whisper_server: whisper, user_ollama_and_runners: ollamaOthers, gpu_top: s.gpu_top, busy_processes: s.busy_processes,
    };
  };
  res.whisper_running = snap("probe").whisper_server.length > 0;
  res.states.push(snap("A: my server up, no model loaded"));
  const r = await chat(port, SENTENCES[0]);
  res.first_load = slim(r);
  res.ps_raw_loaded = await loadedModels(port);
  res.model_loaded = modelInfo(res.ps_raw_loaded?.[0]);
  res.states.push(snap("B: gemma loaded in my server", { model: res.model_loaded }));
  const r2 = await chat(port, SENTENCES[1]);
  res.states.push(snap("C: after one more request", { last_ms: r2.ms }));
  res.unload = await unloadModel(port);
  res.states.push(snap("D: after unload"));
  const w = (label) => res.states.find((s) => s.label.startsWith(label))?.whisper_server?.[0];
  res.whisper_gpu_mb_per_state = Object.fromEntries(res.states.map((s) => [s.label.slice(0, 1), s.whisper_server?.[0]?.gpu_dedicated_mb ?? null]));
  res.whisper_unchanged_by_ollama_load = w("A") && w("B") ? Math.abs((w("A").gpu_dedicated_mb ?? 0) - (w("B").gpu_dedicated_mb ?? 0)) < 50 : null;
  try {
    // /api/ps of this runtime reports odd sizes, so take the placement from the runner's own log
    const lines = fs.readFileSync(srv.logPath, "utf8").split("\n");
    const start = lines.map((l, i) => (/start 0\.40\.13 port/.test(l) ? i : -1)).filter((i) => i >= 0).pop() ?? 0;
    res.runner_log_evidence = lines.slice(start).filter((l) => /offloaded \d+\/\d+ layers|model buffer size|KV buffer size|compute buffer size|n_ctx\s*=|draft/i.test(l)).slice(0, 24).map((l) => l.trim().slice(0, 200));
  } catch { res.runner_log_evidence = null; }
  res.teardown = await stopServer(srv);
  return res;
}

// ───────────────────────────── server start timeline (from the server logs) ─────────────────────────────
/** One entry per `=====` marker in a harness log: when the server listened and when GPU discovery finished. */
function parseStartTimeline(logPath) {
  const out = [];
  let cur = null;
  const ts = (line) => {
    const m = line.match(/^time=(\S+)/);
    return m ? Date.parse(m[1]) : null;
  };
  for (const line of fs.readFileSync(logPath, "utf8").split("\n")) {
    const mk = line.match(/^===== (\S+) start (\S+) port (\d+) \((\S+)\) =====/);
    if (mk) {
      cur = { marker_ms: Date.parse(mk[1]), version: mk[2], port: Number(mk[3]), tag: mk[4], listening_ms: null, gpu_discovery_start_ms: null, gpu_discovery_done_ms: null };
      out.push(cur);
      continue;
    }
    if (!cur || !line.startsWith("time=")) continue;
    const t = ts(line);
    if (t === null) continue;
    if (cur.listening_ms === null && /msg="Listening on/.test(line)) cur.listening_ms = t - cur.marker_ms;
    if (cur.gpu_discovery_start_ms === null && /msg="discovering available GPUs/.test(line)) cur.gpu_discovery_start_ms = t - cur.marker_ms;
    if (cur.gpu_discovery_done_ms === null && /msg="(inference compute|no compatible GPUs)/.test(line)) cur.gpu_discovery_done_ms = t - cur.marker_ms;
  }
  return out.map((e) => ({
    version: e.version, port: e.port, tag: e.tag,
    listening_ms: e.listening_ms, gpu_discovery_done_ms: e.gpu_discovery_done_ms,
    gpu_discovery_ms: e.gpu_discovery_done_ms !== null && e.gpu_discovery_start_ms !== null ? e.gpu_discovery_done_ms - e.gpu_discovery_start_ms : null,
  }));
}

async function phaseLogs() {
  const res = { files: {} };
  const all = [];
  for (const f of fs.existsSync(CFG.logDir) ? fs.readdirSync(CFG.logDir).filter((n) => n.endsWith(".log")) : []) {
    const tl = parseStartTimeline(path.join(CFG.logDir, f));
    res.files[f] = tl.length;
    for (const e of tl) all.push({ file: f, ...e });
  }
  const by = (v) => all.filter((e) => e.version === v);
  res.starts_total = all.length;
  res.by_version = Object.fromEntries([CFG.vNew, CFG.vOld].map((v) => [v, {
    starts: by(v).length,
    listening_ms: stats(by(v).map((e) => e.listening_ms)),
    gpu_discovery_ms: stats(by(v).map((e) => e.gpu_discovery_ms)),
    gpu_discovery_done_after_start_ms: stats(by(v).map((e) => e.gpu_discovery_done_ms)),
  }]));
  res.slowest_gpu_discovery = [...all].filter((e) => e.gpu_discovery_ms !== null).sort((a, b) => b.gpu_discovery_ms - a.gpu_discovery_ms).slice(0, 8);
  res.histogram_gpu_discovery_ms = (() => {
    const bins = { "<1500": 0, "1500-2500": 0, "2500-4000": 0, "4000-8000": 0, ">=8000": 0 };
    for (const e of all) {
      if (e.gpu_discovery_ms === null) continue;
      const v = e.gpu_discovery_ms;
      bins[v < 1500 ? "<1500" : v < 2500 ? "1500-2500" : v < 4000 ? "2500-4000" : v < 8000 ? "4000-8000" : ">=8000"]++;
    }
    return bins;
  })();
  return res;
}

// ───────────────────────────── phase 7: restart probe (narrows slow starts) ─────────────────────────────
async function phase7() {
  const port = PORTS.p7;
  const res = { port, reps: [] };
  for (let i = 1; i <= CFG.probeReps; i++) {
    if (await tcpInUse(port)) throw new Error(`port ${port} busy`);
    const pre = sampleRaw([]);
    const c0 = cpuTimes();
    const srv = startServer(CFG.vNew, port, "p7", { logTag: "p7" });
    // does /api/tags (and /api/version) answer, hang or fail while the server is still starting?
    const probe = (async () => {
      const t0 = srv.spawnedAt;
      while (now() - t0 < 20_000 && !(await tcpInUse(port, 200))) await sleep(5);
      const listening = r1(now() - t0);
      const one = (p) => request(port, "GET", p, undefined, 25_000).then((r) => ({ status: r.status, error: r.error ?? null, answered_at_ms_since_spawn: r1(now() - t0), took_ms: r1(r.ms) }));
      const [tags, version] = await Promise.all([one("/api/tags"), one("/api/version")]);
      return { first_accepted_connection_ms: listening, tags, version };
    })();
    const ready = await waitReady(srv, 60_000);
    const c1 = cpuTimes();
    const blockingProbe = await probe;
    const row = { blocking_probe: blockingProbe, rep: i, ready_ms: ready.ready ? ready.ms : null, app_ready_ms: ready.app_ready_ms ?? null, host_cpu_pct_during_start: cpuPct(c0, c1), busy_before: pre.busy, nvidia_used_before_mb: pre.nvidia?.used_mb ?? null };
    row.teardown = await stopServer(srv);
    res.reps.push(row);
    log(`p7 ${i}/${CFG.probeReps}: ready=${row.ready_ms} ms cpu=${row.host_cpu_pct_during_start}% busy=${JSON.stringify(pre.busy)}`);
  }
  const tl = parseStartTimeline(path.join(CFG.logDir, `p7-${CFG.vNew}.log`));
  const mine = tl.slice(-res.reps.length);
  res.reps.forEach((r, k) => { r.listening_ms = mine[k]?.listening_ms ?? null; r.gpu_discovery_ms = mine[k]?.gpu_discovery_ms ?? null; r.gpu_discovery_done_ms = mine[k]?.gpu_discovery_done_ms ?? null; });
  res.ready_ms = stats(res.reps.map((r) => r.ready_ms));
  res.app_ready_ms = stats(res.reps.map((r) => r.app_ready_ms));
  res.gpu_discovery_ms = stats(res.reps.map((r) => r.gpu_discovery_ms));
  res.over_5s = res.reps.filter((r) => r.ready_ms > 5000).map((r) => ({ rep: r.rep, ready_ms: r.ready_ms, gpu_discovery_ms: r.gpu_discovery_ms, cpu: r.host_cpu_pct_during_start, busy: r.busy_before }));
  res.over_10s = res.reps.filter((r) => r.ready_ms > 10_000).length;
  res.blocking_probe_summary = {
    tags_answer_minus_gpu_discovery_done_ms: stats(res.reps.map((r) => (r.blocking_probe.tags.answered_at_ms_since_spawn ?? NaN) - (r.gpu_discovery_done_ms ?? NaN))),
    tags_status_counts: res.reps.reduce((m, r) => ((m[r.blocking_probe.tags.status ?? r.blocking_probe.tags.error] = (m[r.blocking_probe.tags.status ?? r.blocking_probe.tags.error] ?? 0) + 1), m), {}),
    version_answered_at_ms_since_spawn: stats(res.reps.map((r) => r.blocking_probe.version.answered_at_ms_since_spawn)),
    tags_answered_at_ms_since_spawn: stats(res.reps.map((r) => r.blocking_probe.tags.answered_at_ms_since_spawn)),
    first_accepted_connection_ms: stats(res.reps.map((r) => r.blocking_probe.first_accepted_connection_ms)),
  };
  res.corr_ready_vs_gpu_discovery = (() => {
    const p = res.reps.filter((r) => r.ready_ms !== null && r.gpu_discovery_ms !== null);
    const { r2 } = linreg(p.map((r) => r.gpu_discovery_ms), p.map((r) => r.ready_ms));
    return p.length > 2 ? { n: p.length, r2: Math.round(r2 * 100) / 100 } : null;
  })();
  return res;
}

// ───────────────────────────── summary of everything measured so far ─────────────────────────────
function buildSummary() {
  const P = results.phases;
  const tb = (t) => (t ? { first: t.first, last: t.last, delta: t.delta, delta_pct: t.delta_pct, slope_per_100: t.slope_per_100, r2: t.r2 } : null);
  const soak = (p) => p && {
    requests: p.requests?.length, latency_ms_warm_excl_first: p.latency_ms_warm_excl_first, first_request_ms: p.first_request?.ms,
    errors: p.quality?.errors, empty: p.quality?.empty, quality_fail: p.quality?.quality_fail, pass_pct: p.quality?.pass_pct,
    over_10s: p.outliers?.over_10s?.length, over_3x_p50: p.outliers?.over_3x_p50_warm?.length, model_reloads_after_first: p.outliers?.reloads_after_first?.length, runner_restarted: p.runner_restarted,
    trend_from_request_25: Object.fromEntries(Object.entries(p.trend_from_request_25 ?? {}).map(([k, t]) => [k, tb(t)])),
  };
  const cyc = (p) => p && {
    cycles: p.cycles?.length, sampled: p.cycles_sampled, errors: p.errors, quality_fail: p.quality_fail,
    cold_start_ms: p.cold_start_ms, cold_start_load_duration_ms: p.cold_start_load_duration_ms, unload_ms: p.unload_ms,
    cycles_over_5s: p.cycles?.filter((c) => c.load.ms > 5000).length, cycles_over_10s: p.cycles?.filter((c) => c.load.ms > 10_000).length,
    runner_present_after_unload_of_sampled: p.after_unload_runner_present, runner_gpu_after_unload_mb: p.after_unload_runner_gpu_mb,
    trend: Object.fromEntries(Object.entries(p.trend ?? {}).map(([k, t]) => [k, tb(t)])),
    server_handles_series: p.cycles?.filter((c) => c.after_load).map((c) => [c.cycle, c.after_load.server_handles]),
  };
  const abort = (p) => p && { ...p.summary, calibration: p.calibration, server_only_kill: p.kill_server_only_orphan_check?.map((x) => ({ port_free_ms: x.port_free_ms, runner_survived: x.runner_survived_server_kill, runner_gone_ms: x.runner_gone_ms_after_kill })) };
  const S = {
    note: "Compiled by scripts/ollama-soak.mjs --phases=S from the phase results above. Keys with suffix _run2/_extended/_100cycles are repeats; unsuffixed phase 3/4 used the older fail-fast app-wait model for app_ready_ms.",
    p1_soak_300: soak(P["p1_soak_0.40.13"]),
    p1_soak_extended_3000: soak(P["p1_soak_0.40.13_extended_3000"]),
    p2_cycles_30_run1: cyc(P.p2_load_unload_cycles),
    p2_cycles_30_run2: cyc(P.p2_load_unload_cycles_run2),
    p2_cycles_100: cyc(P.p2_load_unload_cycles_100cycles),
    p3_abort_run1: abort(P.p3_server_abort),
    p3_abort_run2: abort(P.p3_server_abort_run2),
    p4_switch_run1: P.p4_version_switch_same_port?.summary,
    p4_switch_run2: P.p4_version_switch_same_port_run2?.summary,
    p5_parallel: P.p5_parallel_requests && { levels: Object.fromEntries(Object.entries(P.p5_parallel_requests.levels).map(([n, l]) => [n, { latency_ms: l.latency_ms, throughput_req_per_s: l.throughput_req_per_s, errors: l.errors, quality_fail: l.quality_fail, model_reloads: l.model_reloads.length }])), light_requests: P.p5_parallel_requests.light_requests, cold_parallel_4: P.p5_parallel_requests.cold_parallel_4?.map((c) => ({ wall_ms: c.wall_ms, ok: c.ok })) },
    p6_whisper: P.p6_whisper_neighbourhood && { whisper_running: P.p6_whisper_neighbourhood.whisper_running, whisper_gpu_mb_per_state: P.p6_whisper_neighbourhood.whisper_gpu_mb_per_state, whisper_unchanged_by_ollama_load: P.p6_whisper_neighbourhood.whisper_unchanged_by_ollama_load, states: P.p6_whisper_neighbourhood.states?.map((x) => ({ label: x.label, nvidia_smi: x.nvidia_smi, headroom_mb: x.headroom_mb, my_runner_gpu_mb: x.my_runner_gpu_mb })) },
    p7_start_probe: { run1: { ready_ms: P.p7_start_probe?.ready_ms, gpu_discovery_ms: P.p7_start_probe?.gpu_discovery_ms, corr: P.p7_start_probe?.corr_ready_vs_gpu_discovery }, run2: { ready_ms: P.p7_start_probe_run2?.ready_ms, gpu_discovery_ms: P.p7_start_probe_run2?.gpu_discovery_ms, corr: P.p7_start_probe_run2?.corr_ready_vs_gpu_discovery, blocking: P.p7_start_probe_run2?.blocking_probe_summary } },
  };
  // start time vs host CPU over all probe starts
  const reps = [...(P.p7_start_probe?.reps ?? []), ...(P.p7_start_probe_run2?.reps ?? [])].filter((r) => r.ready_ms !== null && r.host_cpu_pct_during_start !== null);
  const hi = reps.filter((r) => r.host_cpu_pct_during_start >= 85);
  const lo = reps.filter((r) => r.host_cpu_pct_during_start < 60);
  S.start_time_vs_host_cpu = { starts: reps.length, cpu_ge_85: { n: hi.length, ready_ms: stats(hi.map((r) => r.ready_ms)) }, cpu_lt_60: { n: lo.length, ready_ms: stats(lo.map((r) => r.ready_ms)) }, r_cpu_vs_ready: (() => { const { r2 } = linreg(reps.map((r) => r.host_cpu_pct_during_start), reps.map((r) => r.ready_ms)); return Math.round(Math.sqrt(r2) * 100) / 100; })() };
  // cold load time vs host cpu over the 100-cycle run
  const c100 = P.p2_load_unload_cycles_100cycles?.cycles ?? [];
  const hiC = c100.filter((c) => c.load.cpu_pct >= 60);
  const loC = c100.filter((c) => c.load.cpu_pct < 60);
  S.cold_load_vs_host_cpu_100cycles = { cpu_ge_60: { n: hiC.length, ms: stats(hiC.map((c) => c.load.ms)) }, cpu_lt_60: { n: loC.length, ms: stats(loC.map((c) => c.load.ms)) } };
  // model store: what changed between the very first snapshot and the last one, and who wrote it
  const d = results.model_store?.diff_first_before_vs_last_after;
  if (d) {
    const tagged = (n) => /trispr-(live|backup)/i.test(n);
    S.model_store = {
      first_before: results.model_store.first_before && { taken: results.model_store.first_before.taken, blobs: results.model_store.first_before.blobs, file_count: results.model_store.first_before.file_count },
      last_after: results.model_store.last_after && { taken: results.model_store.last_after.taken, blobs: results.model_store.last_after.blobs, file_count: results.model_store.last_after.file_count },
      added: d.added, removed: d.removed, changed_files: d.changed.length,
      changed_are_only_mtime_with_same_size: d.changed.every((c) => c.size[0] === c.size[1]),
      entries_named_trispr_live_or_backup: [...d.added, ...d.removed].filter(tagged),
      runs_with_store_changes: results.runs.map((r, i) => ({ run: i + 1, started: r.started, phases: r.phases, added: r.store_diff.added.length, removed: r.store_diff.removed.length, changed: r.store_diff.changed.length })).filter((r) => r.added || r.removed || r.changed),
      attribution: "This harness starts servers with OLLAMA_NOPRUNE=1 and only reads/chats. Store changes appear only in runs that overlapped live tests of another agent (src-tauri/src/ai_fallback/model_verify.rs live_* tests create/copy/delete trispr-live-* and trispr-backup-trispr-live-* tags; /api/copy touches blob mtimes).",
    };
  }
  return S;
}

// ───────────────────────────── main ─────────────────────────────
async function main() {
  if (process.platform !== "win32") throw new Error("Windows only");
  for (const v of [CFG.vNew, CFG.vOld]) if (!fs.existsSync(binPath(v))) throw new Error(`missing ${binPath(v)}`);
  if (!isDir(CFG.modelsDir)) throw new Error(`models dir not found: ${CFG.modelsDir}`);
  const protectedRows = loadProtected();
  loadResults();
  const before = snapshotStore(CFG.modelsDir);
  results.meta ??= {};
  Object.assign(results.meta, {
    script: "scripts/ollama-soak.mjs",
    node: process.version,
    host: { name: os.hostname(), os: `${os.type()} ${os.release()}`, cpu: os.cpus()[0]?.model, logical_cores: os.cpus().length, ram_gb: r1(os.totalmem() / 2 ** 30) },
    gpu: (() => { try { return ps("nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader"); } catch { return null; } })(),
    model: CFG.model, models_dir: CFG.modelsDir, versions: { new: CFG.vNew, old: CFG.vOld },
    binaries: Object.fromEntries([CFG.vNew, CFG.vOld].map((v) => [v, { path: binPath(v), size: fs.statSync(binPath(v)).size, sha256: crypto.createHash("sha256").update(fs.readFileSync(binPath(v))).digest("hex") }])),
    request_form: { endpoint: "/api/chat", stream: false, think: false, keep_alive: -1, options: { num_ctx: 2048, num_thread: NUM_THREAD, temperature: 0.15, num_predict: 2048 }, system_prompt: "OLLAMA_PROMPT_DE (read from provider.rs)", user_message: "<transcript>\\n{text}\\n</transcript> (wrap_transcript_in_xml)" },
    sentences: SENTENCES,
    inherited_ollama_env: Object.fromEntries(Object.entries(process.env).filter(([k]) => /^OLLAMA_/.test(k))),
    spawn_env: { OLLAMA_NO_CLOUD: "1", OLLAMA_KEEP_ALIVE: "90s", OLLAMA_NOPRUNE: "1", OLLAMA_MODELS: CFG.modelsDir, OLLAMA_RUNNERS_DIR: "<runtime>\\lib\\ollama", PATH: "runtime dependency dirs prepended" },
    deviations: [
      "OLLAMA_NOPRUNE=1 set (a plain serve prunes unreferenced blobs on start; the user's store must stay untouched)",
      "options include num_thread (app's ollama_runner_defining_options adds it) in addition to num_ctx/temperature/num_predict",
      "server stdout/stderr written to bench/results/ollama-soak-logs (app discards them)",
      "ready probe polls every 100 ms; app_ready_ms models the app's loop (see appReadySim: blocking full ping at attempt 3). Result keys WITHOUT suffix _run2 for phases 3/4 were produced with the older fail-fast model, where app_ready_ms equals app_ready_failfast_model_ms; real ready times (ready.ms) are model-independent",
      "process kill = taskkill /PID <pid> /T /F, identical to terminate_child_process in lib.rs (the app additionally owns a kill-on-close job object)",
    ],
    protected_processes_at_start: protectedRows,
    spawn_ports: PORTS,
    ps_sampling: "powershell Get-Process (WorkingSet64=RSS, PrivateMemorySize64, HandleCount, Threads.Count) + Get-Counter '\\GPU Process Memory(*)\\Dedicated Usage' per pid of the server tree; nvidia-smi as side value",
  });
  const run = { started: new Date().toISOString(), phases: CFG.phases, args: { ...args }, store_before: storeBrief(before) };
  results.model_store ??= {};
  results.model_store.first_before ??= before;
  saveResults();

  const table = { 1: ["p1_soak_0.40.13", phase1], 2: ["p2_load_unload_cycles", phase2], 3: ["p3_server_abort", phase3], 4: ["p4_version_switch_same_port", phase4], 5: ["p5_parallel_requests", phase5], 6: ["p6_whisper_neighbourhood", phase6], 7: ["p7_start_probe", phase7], L: ["logs_server_start_timeline", phaseLogs] };
  for (const p of CFG.phases) {
    if (p === "S") continue; // handled after the final store snapshot below
    if (!table[p]) continue;
    await runPhase(table[p][0] + (args["key-suffix"] ?? ""), table[p][1]);
  }
  const after = snapshotStore(CFG.modelsDir);
  run.finished = new Date().toISOString();
  run.kill_log = killLog;
  run.store_after = storeBrief(after);
  run.store_diff = diffStore(before, after);
  results.runs.push(run);
  results.model_store.last_after = after;
  results.model_store.diff_first_before_vs_last_after = diffStore(results.model_store.first_before, after);
  if (CFG.phases.includes("S")) results.summary = buildSummary();
  // end-of-run safety checks
  const leftovers = [];
  for (let port = 11450; port <= 11459; port++) if (await tcpInUse(port)) leftovers.push(port);
  const protectedNow = JSON.parse(ps(PROTECTED_PS) || "[]");
  run.end_check = {
    own_ports_still_listening: leftovers,
    own_processes_still_alive: [...owned.keys()],
    protected_pids_still_alive: protectedRows.filter((r) => protectedNow.some((n) => n.pid === r.pid && n.start === r.start)).map((r) => r.pid),
    protected_pids_missing: protectedRows.filter((r) => !protectedNow.some((n) => n.pid === r.pid && n.start === r.start)).map((r) => r.pid),
    llama_server_not_in_protected_start_set: protectedNow.filter((n) => !protectedPids.has(n.pid)).map((n) => ({ pid: n.pid, name: n.name })),
  };
  saveResults();
  log("store diff:", JSON.stringify({ added: run.store_diff.added, removed: run.store_diff.removed, changed: run.store_diff.changed.length, blobs: run.store_diff.blobs_after }));
  log("end check:", JSON.stringify(run.end_check));
  log(`results: ${CFG.out}`);
}

main().then(
  () => process.exit(0),
  (e) => { console.error(e); cleanupAllSync(); process.exit(1); },
);
