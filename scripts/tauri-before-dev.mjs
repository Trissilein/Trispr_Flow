import { spawn } from "node:child_process";

const DEV_URL = process.env.TRISPR_DEV_URL || "http://localhost:1420";
const APP_TITLE_MARKER = "<title>Trispr Flow</title>";

async function probeDevUrl(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 1500);
  try {
    const response = await fetch(url, { signal: controller.signal });
    const body = await response.text();
    return { ok: response.ok, body };
  } catch {
    return null;
  } finally {
    clearTimeout(timeout);
  }
}

function spawnViteDevServer() {
  const child = spawn("npm run dev:web", {
    stdio: "inherit",
    shell: true,
    env: process.env,
  });

  const forwardSignal = (signal) => {
    if (!child.killed) child.kill(signal);
  };
  process.on("SIGINT", () => forwardSignal("SIGINT"));
  process.on("SIGTERM", () => forwardSignal("SIGTERM"));

  child.on("exit", (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });
}

async function main() {
  const probe = await probeDevUrl(DEV_URL);
  if (probe?.ok) {
    if (!probe.body.includes(APP_TITLE_MARKER)) {
      console.error(
        `[tauri-before-dev] Port in use by another app on ${DEV_URL}. Stop it or change dev port.`
      );
      process.exit(1);
    }
    console.log(`[tauri-before-dev] Reusing existing dev server at ${DEV_URL}.`);
    return;
  }

  console.log(`[tauri-before-dev] No existing dev server detected, starting Vite on ${DEV_URL}.`);
  spawnViteDevServer();
}

void main();
