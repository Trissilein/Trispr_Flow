// Quick-start launcher behind the "Trispr Flow" desktop shortcut (entry: start-trispr-flow-quick.vbs).
//
// 1. Starts the last built debug exe at once. A running app is only brought to the front
//    (single-instance plugin), no second process stays alive.
// 2. If the sources changed since that build, runs `tauri build --debug --no-bundle` in the
//    background with its own target dir and shows a balloon when the new build is ready.
//
// Why the exe starts from a copy (target-app/run) and not from the build output
// (target-app/debug): Windows will not let cargo replace a running exe, so the file that
// runs must never be the file that gets built. A running copy is never overwritten, nothing
// is killed or deleted; the first launch after the app was closed picks up the new build.
//
// Resources (whisper-server, CUDA/Vulkan DLLs, quantize) are found because the exe
// starts with cwd = repo root and paths.rs falls back to <cwd>/src-tauri/bin; module packages
// and ffmpeg fall back to the compile-time CARGO_MANIFEST_DIR. Nothing sits next to the copy.
//
// Trace: src-tauri/target-app/launch.log (launcher), launch-build.log (last build output).

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
  appendFileSync,
  closeSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
  readFileSync,
  statSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetDir = join(repo, "src-tauri", "target-app");
const builtExe = join(targetDir, "debug", "trispr-flow.exe"); // cargo output, never started
const runExe = join(targetDir, "run", "trispr-flow.exe"); // copy that gets started
const stampFile = join(targetDir, "launch-stamp.txt");
const lockFile = join(targetDir, "launch-build.lock");
const logFile = join(targetDir, "launch.log");
const buildLog = join(targetDir, "launch-build.log");

// Everything the exe is built from: frontend, Rust, config, embedded assets, patched crate.
const INPUTS = [
  "src",
  "public",
  "index.html",
  "overlay.html",
  "vite.config.ts",
  "tsconfig.json",
  "src-tauri/src",
  "src-tauri/assets",
  "src-tauri/capabilities",
  "src-tauri/icons",
  "src-tauri/build.rs",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  "src-tauri/tauri.conf.json",
  "vendor",
];

const log = (msg) => {
  try {
    appendFileSync(logFile, `${new Date().toISOString()} [${process.pid}] ${msg}\n`);
  } catch {}
};

const read = (file) => {
  try {
    return readFileSync(file, "utf8").trim();
  } catch {
    return "";
  }
};

// Hash over path + size + mtime (no file reads, ~270 files, a few ms).
function stampOf() {
  const hash = createHash("sha1");
  const walk = (path) => {
    let stat;
    try {
      stat = statSync(path);
    } catch {
      return;
    }
    if (stat.isDirectory()) {
      for (const name of readdirSync(path).sort()) {
        if (!name.includes(".tmp.")) walk(join(path, name)); // editor/agent temp copies
      }
    } else {
      hash.update(`${relative(repo, path)}|${stat.size}|${Math.trunc(stat.mtimeMs)}\n`);
    }
  };
  for (const input of INPUTS) walk(join(repo, input));
  return hash.digest("hex");
}

// Copy the newest build into the run slot unless an exe from that slot is running.
function promote() {
  if (!existsSync(builtExe)) return;
  const built = statSync(builtExe);
  const run = existsSync(runExe) ? statSync(runExe) : null;
  if (run && run.size === built.size && Math.abs(run.mtimeMs - built.mtimeMs) < 1000) return;
  try {
    mkdirSync(dirname(runExe), { recursive: true });
    if (run) closeSync(openSync(runExe, "r+")); // throws EBUSY while the old copy runs
    copyFileSync(builtExe, runExe);
    utimesSync(runExe, built.atime, built.mtime);
    log("run copy updated");
  } catch (e) {
    log(`run copy kept (${e.code || e.message})`);
  }
}

// detached = no console for the console-subsystem debug exe; no windowsHide, it would hide
// the app window as well (first ShowWindow call follows STARTUPINFO).
function start() {
  const child = spawn(runExe, [], { cwd: repo, detached: true, stdio: "ignore" });
  child.on("error", (e) => log(`start failed: ${e.message}`));
  child.unref();
  log(`started pid=${child.pid} after ${Math.round(process.uptime() * 1000)}ms`);
}

// One build at a time. Stale when its launcher is gone or the lock is over an hour old.
// ponytail: a reused PID keeps a dead lock alive for up to an hour; nothing is deleted.
function lock() {
  try {
    writeFileSync(lockFile, String(process.pid), { flag: "wx" });
    return true;
  } catch {}
  const pid = Number(read(lockFile));
  let alive = false;
  if (pid > 0) {
    try {
      process.kill(pid, 0);
      alive = true;
    } catch (e) {
      alive = e.code === "EPERM";
    }
  }
  if (alive && Date.now() - statSync(lockFile).mtimeMs < 3600e3) return false;
  writeFileSync(lockFile, String(process.pid));
  return true;
}

function build() {
  return new Promise((done) => {
    const out = openSync(buildLog, "w");
    const cli = join(repo, "node_modules", "@tauri-apps", "cli", "tauri.js");
    // Flag needed: npm @tauri-apps/api (2.9.1) and plugin-dialog (2.6.0) lag behind the crates
    // (tauri 2.12.5, plugin-dialog 2.8.4). `tauri dev` runs with that mix, plain `tauri build` refuses.
    const args = [cli, "build", "--debug", "--no-bundle", "--ignore-version-mismatches"];
    const child = spawn(process.execPath, args, {
      cwd: repo,
      env: { ...process.env, CARGO_TARGET_DIR: targetDir },
      stdio: ["ignore", out, out],
      windowsHide: true,
    });
    child.on("error", (e) => {
      log(`build spawn failed: ${e.message}`);
      closeSync(out);
      done(false);
    });
    child.on("exit", (code) => {
      log(`build exit code=${code}`);
      closeSync(out);
      done(code === 0 && existsSync(builtExe));
    });
  });
}

// Windows balloon from a hidden PowerShell (NotifyIcon); text and icon go in via env vars.
function notify(text) {
  log(`balloon: ${text}`);
  const script =
    "Add-Type -AssemblyName System.Windows.Forms,System.Drawing;" +
    "$n = New-Object System.Windows.Forms.NotifyIcon;" +
    "$n.Icon = try { New-Object System.Drawing.Icon $env:TF_ICON } catch { [System.Drawing.SystemIcons]::Information };" +
    "$n.Visible = $true;" +
    "$n.ShowBalloonTip(15000, 'Trispr Flow', $env:TF_TEXT, 'Info');" +
    "Start-Sleep 16; $n.Dispose()";
  spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script], {
    env: { ...process.env, TF_TEXT: text, TF_ICON: join(repo, "src-tauri", "icons", "icon.ico") },
    detached: true,
    stdio: "ignore",
    windowsHide: true,
  }).unref();
}

async function main() {
  mkdirSync(targetDir, { recursive: true });
  promote();
  const have = existsSync(runExe);
  if (have) start();
  else log("no build yet");

  const stamp = stampOf();
  if (stamp === read(stampFile) && existsSync(builtExe)) return log("build is current");
  if (!lock()) return log("build already running");

  if (!have) notify("Trispr Flow is building for the first time. You get a message when it is ready.");
  log(`build start stamp=${stamp.slice(0, 8)}`);
  if (await build()) {
    writeFileSync(stampFile, stamp);
    notify(
      have
        ? "Trispr Flow update ready. Restart the app to use it."
        : "Trispr Flow is ready. Start it with the desktop shortcut.",
    );
  } else {
    notify("Trispr Flow build failed. Details: src-tauri\\target-app\\launch-build.log");
  }
  writeFileSync(lockFile, "0");
}

main().catch((e) => log(`error: ${e.stack || e}`));
