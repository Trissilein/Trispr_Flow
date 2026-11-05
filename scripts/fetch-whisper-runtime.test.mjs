// Run: node --test scripts/fetch-whisper-runtime.test.mjs
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import zlib from "node:zlib";
import { downloadZip, installFromZip, loadVariant, sha256File, verifyDir } from "./fetch-whisper-runtime.mjs";

const sha = (buf) => crypto.createHash("sha256").update(buf).digest("hex");

function makeZip(entries) {
  const parts = [];
  const central = [];
  let offset = 0;
  for (const { name, data, method = 8 } of entries) {
    const nameBuf = Buffer.from(name);
    const comp = method === 8 ? zlib.deflateRawSync(data) : data;
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(method, 8);
    local.writeUInt32LE(comp.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBuf.length, 26);
    parts.push(local, nameBuf, comp);
    const cd = Buffer.alloc(46);
    cd.writeUInt32LE(0x02014b50, 0);
    cd.writeUInt16LE(20, 4);
    cd.writeUInt16LE(20, 6);
    cd.writeUInt16LE(method, 10);
    cd.writeUInt32LE(comp.length, 20);
    cd.writeUInt32LE(data.length, 24);
    cd.writeUInt16LE(nameBuf.length, 28);
    cd.writeUInt32LE(offset, 42);
    central.push(cd, nameBuf);
    offset += local.length + nameBuf.length + comp.length;
  }
  const cdBuf = Buffer.concat(central);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(entries.length, 8);
  eocd.writeUInt16LE(entries.length, 10);
  eocd.writeUInt32LE(cdBuf.length, 12);
  eocd.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, cdBuf, eocd]);
}

const FILES = {
  "a.dll": Buffer.from("alpha-".repeat(5000)),
  "b.exe": Buffer.from("beta"),
  "empty.bin": Buffer.alloc(0),
};
const spec = {
  files: Object.entries(FILES).map(([name, data]) => ({ name, length: data.length, sha256: sha(data) })),
};
const goodEntries = () => Object.entries(FILES).map(([name, data], i) => ({ name, data, method: i === 1 ? 0 : 8 }));

function scratch() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "fetch-rt-test-"));
  return { dir, zip: path.join(dir, "rt.zip"), target: path.join(dir, "bin", "vulkan") };
}

function assertNoStaging(target) {
  const leftovers = fs.readdirSync(path.dirname(target)).filter((n) => n.startsWith("."));
  assert.deepEqual(leftovers, []);
}

test("installs deflate + store + empty entries and validates", async () => {
  const s = scratch();
  fs.writeFileSync(s.zip, makeZip(goodEntries()));
  await installFromZip(s.zip, s.target, spec);
  assert.deepEqual(await verifyDir(s.target, spec), []);
  assertNoStaging(s.target);
});

test("replaces an existing target and drops stale extra files", async () => {
  const s = scratch();
  fs.mkdirSync(s.target, { recursive: true });
  fs.writeFileSync(path.join(s.target, "stale.dll"), "old");
  fs.writeFileSync(s.zip, makeZip(goodEntries()));
  await installFromZip(s.zip, s.target, spec);
  assert.deepEqual(await verifyDir(s.target, spec), []);
  assert.deepEqual(fs.readdirSync(s.target).sort(), ["a.dll", "b.exe", "empty.bin"]);
});

const failureCases = {
  "zip path traversal entry": () => [...goodEntries(), { name: "../evil.dll", data: Buffer.from("x") }],
  "absolute / drive entry": () => [...goodEntries(), { name: "C:\\evil.dll", data: Buffer.from("x") }],
  "subdirectory entry": () => [...goodEntries(), { name: "sub/c.dll", data: Buffer.from("x") }],
  "unexpected extra file": () => [...goodEntries(), { name: "extra.dll", data: Buffer.from("x") }],
  "missing file from list": () => goodEntries().slice(0, 2),
  "wrong file content (hash mismatch)": () => goodEntries().map((e) => (e.name === "b.exe" ? { ...e, data: Buffer.from("BETA") } : e)),
  "wrong length (size bomb)": () => goodEntries().map((e) => (e.name === "a.dll" ? { ...e, data: Buffer.concat([e.data, e.data]) } : e)),
  "duplicate entry": () => [...goodEntries(), goodEntries()[0]],
};
for (const [label, build] of Object.entries(failureCases)) {
  test(`rejects ${label} and leaves existing target + disk clean`, async () => {
    const s = scratch();
    fs.mkdirSync(s.target, { recursive: true });
    fs.writeFileSync(path.join(s.target, "keep.dll"), "previous install");
    fs.writeFileSync(s.zip, makeZip(build()));
    await assert.rejects(installFromZip(s.zip, s.target, spec));
    assert.equal(fs.readFileSync(path.join(s.target, "keep.dll"), "utf8"), "previous install");
    assert.deepEqual(fs.readdirSync(s.target), ["keep.dll"]);
    assertNoStaging(s.target);
    assert.equal(fs.existsSync(path.join(s.dir, "evil.dll")), false);
    assert.equal(fs.existsSync(path.join(s.dir, "bin", "evil.dll")), false);
  });
}

test("rejects a non-zip file", async () => {
  const s = scratch();
  fs.writeFileSync(s.zip, Buffer.from("definitely not a zip"));
  await assert.rejects(installFromZip(s.zip, s.target, spec), /not a zip/);
  assertNoStaging(s.target);
});

test("verifyDir reports missing, mismatching and unexpected files", async () => {
  const s = scratch();
  fs.mkdirSync(s.target, { recursive: true });
  fs.writeFileSync(path.join(s.target, "a.dll"), "tampered");
  fs.writeFileSync(path.join(s.target, "extra.txt"), "x");
  const failures = (await verifyDir(s.target, spec)).join("\n");
  assert.match(failures, /length mismatch: a\.dll/);
  assert.match(failures, /missing: b\.exe/);
  assert.match(failures, /unexpected file: extra\.txt/);
  assert.equal(await sha256File(path.join(s.target, "extra.txt")), sha(Buffer.from("x")));
});

test("loadVariant rejects non-allowlisted URLs and bad hashes", () => {
  const s = scratch();
  const manifest = path.join(s.dir, "m.json");
  const variant = (over) => ({ url: "https://github.com/Trissilein/Trispr_Flow/releases/download/t/x.zip", zip_sha256: "a".repeat(64), zip_size: 10, files: [{ name: "a.dll", length: 1, sha256: "b".repeat(64) }], ...over });
  const write = (over) => fs.writeFileSync(manifest, JSON.stringify({ variants: { v: variant(over) } }));
  write({});
  assert.equal(loadVariant(manifest, "v").files.length, 1);
  write({ url: "https://evil.example/x.zip" });
  assert.throws(() => loadVariant(manifest, "v"), /url must start with/);
  write({ url: "http://github.com/Trissilein/Trispr_Flow/releases/download/t/x.zip" });
  assert.throws(() => loadVariant(manifest, "v"), /url must start with/);
  write({ zip_sha256: "nope" });
  assert.throws(() => loadVariant(manifest, "v"), /zip_sha256/);
  write({ files: [{ name: "../x.dll", length: 1, sha256: "b".repeat(64) }] });
  assert.throws(() => loadVariant(manifest, "v"), /unsafe file name/);
});

function withFetch(bodyFor, fn) {
  const original = globalThis.fetch;
  let calls = 0;
  globalThis.fetch = async () => {
    calls += 1;
    return new Response(bodyFor(calls), { status: 200 });
  };
  const log = console.log;
  console.log = () => {};
  return fn(() => calls).finally(() => {
    globalThis.fetch = original;
    console.log = log;
  });
}

test("download: complete body with matching hash is accepted", async () => {
  const s = scratch();
  const body = Buffer.from("zip-bytes".repeat(100));
  const dlSpec = { url: "https://x", zip_size: body.length, zip_sha256: sha(body) };
  await withFetch(() => body, async () => {
    await downloadZip(dlSpec, s.zip);
  });
  assert.deepEqual(fs.readFileSync(s.zip), body);
  assert.equal(fs.existsSync(`${s.zip}.part`), false);
});

test("download: hash mismatch fails once (no retry) and leaves nothing behind", async () => {
  const s = scratch();
  const body = Buffer.from("zip-bytes".repeat(100));
  const dlSpec = { url: "https://x", zip_size: body.length, zip_sha256: "0".repeat(64) };
  await withFetch(() => body, async (calls) => {
    await assert.rejects(downloadZip(dlSpec, s.zip), /SHA-256 mismatch/);
    assert.equal(calls(), 1);
  });
  assert.deepEqual(fs.readdirSync(s.dir), []);
});

test("download: truncated body is retried 3x, then fails with no .part / zip left", async () => {
  const s = scratch();
  const body = Buffer.from("zip-bytes".repeat(100));
  const dlSpec = { url: "https://x", zip_size: body.length, zip_sha256: sha(body) };
  await withFetch(() => body.subarray(0, 50), async (calls) => {
    await assert.rejects(downloadZip(dlSpec, s.zip), /incomplete/);
    assert.equal(calls(), 3);
  });
  assert.deepEqual(fs.readdirSync(s.dir), []);
});

test("download: oversized body is aborted", async () => {
  const s = scratch();
  const body = Buffer.from("zip-bytes".repeat(100));
  const dlSpec = { url: "https://x", zip_size: 100, zip_sha256: sha(body) };
  await withFetch(() => body, async () => {
    await assert.rejects(downloadZip(dlSpec, s.zip), /exceeds expected size/);
  });
  assert.deepEqual(fs.readdirSync(s.dir), []);
});
