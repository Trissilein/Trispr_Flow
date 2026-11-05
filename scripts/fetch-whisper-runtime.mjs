#!/usr/bin/env node
// Download, verify and unpack the pinned whisper.cpp runtime zips into src-tauri/bin/{cuda,vulkan}.
//
// Source of truth: src-tauri/runtime-manifests/whisper-runtime-<version>.json
//   variants.<name>.url / zip_sha256 / zip_size  -> the zip asset (GitHub release, SHA-256 pinned)
//   variants.<name>.files[{name,length,sha256}]  -> exact content of the unpacked directory
//
// Safety: only manifest-listed flat file names are extracted (no path traversal), the zip hash is
// checked before anything is unpacked, every unpacked file is checked by length + SHA-256, and the
// target directory is only replaced (atomic rename) after all checks passed. A failure never leaves
// a partial target behind.
//
// Usage:
//   node scripts/fetch-whisper-runtime.mjs [--variants cuda,vulkan] [--bin-dir <dir>]
//        [--manifest <path>] [--zip <local.zip>] [--verify-only] [--force]

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath, pathToFileURL } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEFAULT_MANIFEST = path.join(repoRoot, "src-tauri", "runtime-manifests", "whisper-runtime-1.9.5.json");
const DEFAULT_BIN_DIR = path.join(repoRoot, "src-tauri", "bin");
const ALLOWED_URL_PREFIX = "https://github.com/Trissilein/Trispr_Flow/releases/download/";
const MAX_ATTEMPTS = 3;

const log = (msg) => console.log(`[fetch-whisper-runtime] ${msg}`);

// ---------------------------------------------------------------- manifest

export function assertPlainName(name) {
  if (
    typeof name !== "string" ||
    name === "" ||
    name === "." ||
    name === ".." ||
    /[\\/:\0]/.test(name)
  ) {
    throw new Error(`unsafe file name '${name}' (must be a plain file name)`);
  }
}

export function loadVariant(manifestPath, variantName) {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const spec = manifest?.variants?.[variantName];
  if (!spec) {
    throw new Error(`variant '${variantName}' not found in ${manifestPath}`);
  }
  if (typeof spec.url !== "string" || !spec.url.startsWith(ALLOWED_URL_PREFIX)) {
    throw new Error(`variant '${variantName}': url must start with ${ALLOWED_URL_PREFIX}`);
  }
  if (!/^[0-9a-f]{64}$/.test(String(spec.zip_sha256 || ""))) {
    throw new Error(`variant '${variantName}': zip_sha256 is missing or not a SHA-256 hex digest`);
  }
  if (!Number.isInteger(spec.zip_size) || spec.zip_size <= 0) {
    throw new Error(`variant '${variantName}': zip_size is missing`);
  }
  if (!Array.isArray(spec.files) || spec.files.length === 0) {
    throw new Error(`variant '${variantName}': files[] is missing or empty`);
  }
  const seen = new Set();
  for (const file of spec.files) {
    assertPlainName(file.name);
    if (seen.has(file.name)) throw new Error(`variant '${variantName}': duplicate file '${file.name}'`);
    seen.add(file.name);
    if (!Number.isInteger(file.length) || file.length < 0 || !/^[0-9a-f]{64}$/.test(String(file.sha256 || ""))) {
      throw new Error(`variant '${variantName}': invalid length/sha256 for '${file.name}'`);
    }
  }
  return spec;
}

// ---------------------------------------------------------------- hashing / verification

export async function sha256File(filePath) {
  const hash = crypto.createHash("sha256");
  for await (const chunk of fs.createReadStream(filePath)) hash.update(chunk);
  return hash.digest("hex");
}

/** Returns a list of failure strings (empty = directory matches spec exactly). */
export async function verifyDir(dir, spec) {
  if (!fs.existsSync(dir) || !fs.statSync(dir).isDirectory()) return [`directory missing: ${dir}`];
  const failures = [];
  const expected = new Set(spec.files.map((f) => f.name));
  for (const file of spec.files) {
    const filePath = path.join(dir, file.name);
    if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
      failures.push(`missing: ${file.name}`);
      continue;
    }
    const length = fs.statSync(filePath).size;
    if (length !== file.length) {
      failures.push(`length mismatch: ${file.name} expected=${file.length} actual=${length}`);
      continue;
    }
    const actual = await sha256File(filePath);
    if (actual !== file.sha256) {
      failures.push(`sha256 mismatch: ${file.name} expected=${file.sha256} actual=${actual}`);
    }
  }
  for (const entry of fs.readdirSync(dir)) {
    if (!expected.has(entry)) failures.push(`unexpected file: ${entry}`);
  }
  return failures;
}

// ---------------------------------------------------------------- download

function hashingTap({ limit, onChunk } = {}) {
  const hash = crypto.createHash("sha256");
  let bytes = 0;
  const stream = new Transform({
    transform(chunk, _enc, cb) {
      bytes += chunk.length;
      if (limit !== undefined && bytes > limit) {
        cb(new Error(`stream exceeds expected size of ${limit} bytes`));
        return;
      }
      hash.update(chunk);
      if (onChunk) onChunk(bytes);
      cb(null, chunk);
    },
  });
  return { stream, digest: () => hash.digest("hex"), bytes: () => bytes };
}

async function downloadOnce(spec, destPath) {
  const partPath = `${destPath}.part`;
  fs.rmSync(partPath, { force: true });
  const res = await fetch(spec.url, { redirect: "follow" });
  if (!res.ok || !res.body) throw new Error(`download failed: HTTP ${res.status} for ${spec.url}`);
  let lastPct = -1;
  const tap = hashingTap({
    limit: spec.zip_size,
    onChunk: (bytes) => {
      const pct = Math.floor((bytes / spec.zip_size) * 100);
      if (pct >= lastPct + 10) {
        lastPct = pct - (pct % 10);
        log(`  ${lastPct}% (${(bytes / 1048576).toFixed(0)} MB)`);
      }
    },
  });
  try {
    await pipeline(Readable.fromWeb(res.body), tap.stream, fs.createWriteStream(partPath));
  } catch (err) {
    fs.rmSync(partPath, { force: true });
    throw err;
  }
  if (tap.bytes() !== spec.zip_size) {
    fs.rmSync(partPath, { force: true });
    throw new Error(`download incomplete: got ${tap.bytes()} of ${spec.zip_size} bytes`);
  }
  const digest = tap.digest();
  if (digest !== spec.zip_sha256) {
    fs.rmSync(partPath, { force: true });
    const err = new Error(`zip SHA-256 mismatch: expected=${spec.zip_sha256} actual=${digest}`);
    err.retryable = false;
    throw err;
  }
  fs.renameSync(partPath, destPath);
}

export async function downloadZip(spec, destPath) {
  for (let attempt = 1; ; attempt += 1) {
    try {
      log(`downloading ${spec.url}`);
      await downloadOnce(spec, destPath);
      return;
    } catch (err) {
      if (err.retryable === false || attempt >= MAX_ATTEMPTS) throw err;
      log(`attempt ${attempt} failed (${err.message}); retrying`);
    }
  }
}

// ---------------------------------------------------------------- zip reading (store + deflate, no zip64)

async function listZip(fh, size) {
  const tailLen = Math.min(size, 65557);
  const tail = Buffer.alloc(tailLen);
  await fh.read(tail, 0, tailLen, size - tailLen);
  let eocd = -1;
  for (let i = tailLen - 22; i >= 0; i -= 1) {
    if (tail.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error("not a zip file (end-of-central-directory record missing)");
  const total = tail.readUInt16LE(eocd + 10);
  const cdSize = tail.readUInt32LE(eocd + 12);
  const cdOffset = tail.readUInt32LE(eocd + 16);
  if (total === 0xffff || cdSize === 0xffffffff || cdOffset === 0xffffffff) {
    throw new Error("zip64 archives are not supported");
  }
  if (cdOffset + cdSize > size) throw new Error("corrupt zip: central directory out of range");
  const cd = Buffer.alloc(cdSize);
  await fh.read(cd, 0, cdSize, cdOffset);
  const entries = [];
  let p = 0;
  for (let n = 0; n < total; n += 1) {
    if (p + 46 > cd.length || cd.readUInt32LE(p) !== 0x02014b50) throw new Error("corrupt zip central directory");
    const nameLen = cd.readUInt16LE(p + 28);
    const extraLen = cd.readUInt16LE(p + 30);
    const commentLen = cd.readUInt16LE(p + 32);
    const entry = {
      flags: cd.readUInt16LE(p + 8),
      method: cd.readUInt16LE(p + 10),
      compSize: cd.readUInt32LE(p + 20),
      size: cd.readUInt32LE(p + 24),
      localOffset: cd.readUInt32LE(p + 42),
      name: cd.toString("utf8", p + 46, p + 46 + nameLen),
    };
    if (entry.compSize === 0xffffffff || entry.size === 0xffffffff) throw new Error("zip64 archives are not supported");
    entries.push(entry);
    p += 46 + nameLen + extraLen + commentLen;
  }
  return entries;
}

async function extractEntry(fh, entry, destPath, expected) {
  if (entry.flags & 1) throw new Error(`encrypted zip entry '${entry.name}' is not supported`);
  if (entry.method !== 0 && entry.method !== 8) throw new Error(`unsupported zip method ${entry.method} for '${entry.name}'`);
  if (entry.size !== expected.length) {
    throw new Error(`size mismatch for '${entry.name}': zip=${entry.size} manifest=${expected.length}`);
  }
  const header = Buffer.alloc(30);
  await fh.read(header, 0, 30, entry.localOffset);
  if (header.readUInt32LE(0) !== 0x04034b50) throw new Error(`corrupt zip local header for '${entry.name}'`);
  const dataStart = entry.localOffset + 30 + header.readUInt16LE(26) + header.readUInt16LE(28);
  const tap = hashingTap({ limit: expected.length });
  const out = fs.createWriteStream(destPath);
  if (entry.compSize === 0) {
    await pipeline(Readable.from([]), tap.stream, out);
  } else {
    const src = fh.createReadStream({ start: dataStart, end: dataStart + entry.compSize - 1, autoClose: false });
    const stages = entry.method === 8 ? [src, zlib.createInflateRaw(), tap.stream, out] : [src, tap.stream, out];
    await pipeline(...stages);
  }
  if (tap.bytes() !== expected.length) {
    throw new Error(`'${entry.name}' unpacked to ${tap.bytes()} bytes, expected ${expected.length}`);
  }
  if (tap.digest() !== expected.sha256) throw new Error(`sha256 mismatch after unpacking '${entry.name}'`);
}

/**
 * Unpacks zipPath into targetDir (replacing it) iff the zip content matches spec.files exactly.
 * On any error targetDir is left untouched and no staging directory remains.
 */
export async function installFromZip(zipPath, targetDir, spec) {
  const parent = path.dirname(targetDir);
  const base = path.basename(targetDir);
  fs.mkdirSync(parent, { recursive: true });
  for (const leftover of fs.readdirSync(parent)) {
    if (leftover.startsWith(`.${base}.staging-`) || leftover.startsWith(`.${base}.old-`)) {
      fs.rmSync(path.join(parent, leftover), { recursive: true, force: true });
    }
  }
  const staging = path.join(parent, `.${base}.staging-${process.pid}`);
  fs.mkdirSync(staging);
  const fh = await fs.promises.open(zipPath, "r");
  try {
    const entries = await listZip(fh, (await fh.stat()).size);
    const expected = new Map(spec.files.map((f) => [f.name, f]));
    const seen = new Set();
    for (const entry of entries) {
      assertPlainName(entry.name);
      if (!expected.has(entry.name)) throw new Error(`zip contains unexpected entry '${entry.name}'`);
      if (seen.has(entry.name)) throw new Error(`zip contains duplicate entry '${entry.name}'`);
      seen.add(entry.name);
    }
    for (const name of expected.keys()) {
      if (!seen.has(name)) throw new Error(`zip is missing '${name}'`);
    }
    for (const entry of entries) {
      await extractEntry(fh, entry, path.join(staging, entry.name), expected.get(entry.name));
    }
  } catch (err) {
    await fh.close();
    fs.rmSync(staging, { recursive: true, force: true });
    throw err;
  }
  await fh.close();

  // Atomic-ish swap: old dir is moved aside first, restored if the final rename fails.
  const old = path.join(parent, `.${base}.old-${process.pid}`);
  const hadOld = fs.existsSync(targetDir);
  try {
    if (hadOld) fs.renameSync(targetDir, old);
    fs.renameSync(staging, targetDir);
  } catch (err) {
    if (hadOld && !fs.existsSync(targetDir) && fs.existsSync(old)) fs.renameSync(old, targetDir);
    fs.rmSync(staging, { recursive: true, force: true });
    throw err;
  }
  if (hadOld) fs.rmSync(old, { recursive: true, force: true });
}

// ---------------------------------------------------------------- CLI

function parseArgs(argv) {
  const args = { variants: ["cuda", "vulkan"], binDir: DEFAULT_BIN_DIR, manifest: DEFAULT_MANIFEST, zip: "", verifyOnly: false, force: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = () => {
      if (i + 1 >= argv.length) throw new Error(`missing value for ${arg}`);
      i += 1;
      return argv[i];
    };
    if (arg === "--variants") args.variants = next().split(",").map((v) => v.trim()).filter(Boolean);
    else if (arg === "--bin-dir") args.binDir = path.resolve(next());
    else if (arg === "--manifest") args.manifest = path.resolve(next());
    else if (arg === "--zip") args.zip = path.resolve(next());
    else if (arg === "--verify-only") args.verifyOnly = true;
    else if (arg === "--force") args.force = true;
    else throw new Error(`unknown argument ${arg}`);
  }
  if (args.zip && args.variants.length !== 1) throw new Error("--zip needs exactly one --variants entry");
  return args;
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (err) {
    console.error(`[fetch-whisper-runtime] ${err.message}`);
    console.error("Usage: node scripts/fetch-whisper-runtime.mjs [--variants cuda,vulkan] [--bin-dir <dir>] [--manifest <path>] [--zip <local.zip>] [--verify-only] [--force]");
    return 2;
  }
  for (const variant of args.variants) {
    const spec = loadVariant(args.manifest, variant);
    const targetDir = path.join(args.binDir, variant);
    if (!args.force || args.verifyOnly) {
      const failures = await verifyDir(targetDir, spec);
      if (failures.length === 0) {
        log(`OK: ${variant} already matches manifest (${spec.files.length} files) at ${targetDir}`);
        continue;
      }
      if (args.verifyOnly) {
        console.error(`[fetch-whisper-runtime] ${variant} validation failed (${targetDir}):`);
        for (const failure of failures) console.error(`  - ${failure}`);
        return 1;
      }
    }
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "trispr-runtime-"));
    try {
      let zipPath = args.zip;
      if (zipPath) {
        const digest = await sha256File(zipPath);
        if (digest !== spec.zip_sha256) throw new Error(`zip SHA-256 mismatch: expected=${spec.zip_sha256} actual=${digest}`);
        log(`local zip SHA-256 OK: ${digest}`);
      } else {
        zipPath = path.join(tmp, path.basename(new URL(spec.url).pathname));
        await downloadZip(spec, zipPath);
        log(`zip SHA-256 OK: ${spec.zip_sha256}`);
      }
      await installFromZip(zipPath, targetDir, spec);
      const failures = await verifyDir(targetDir, spec);
      if (failures.length > 0) throw new Error(`post-install validation failed: ${failures.join("; ")}`);
      log(`OK: ${variant} unpacked and validated (${spec.files.length} files, per-file SHA-256 match) -> ${targetDir}`);
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  }
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().then(
    (code) => process.exit(code),
    (err) => {
      console.error(`[fetch-whisper-runtime] FAILED: ${err.message}`);
      process.exit(1);
    },
  );
}
