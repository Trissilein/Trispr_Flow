import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

const configFiles = [
  'src-tauri/tauri.conf.json',
  'src-tauri/tauri.conf.vulkan.json',
  'src-tauri/tauri.conf.cuda.analysis.json',
];

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, 'utf8'));
}

function collectBundledBinaries(configPath) {
  const config = readJson(configPath);
  const resources = config?.bundle?.resources;
  const binaries = new Set();

  if (!resources) return binaries;

  const resourceValues = Array.isArray(resources) ? resources : Object.values(resources);

  for (const value of resourceValues) {
    if (typeof value !== 'string') continue;
    const normalized = value.replace(/\\/g, '/');
    const fileName = path.posix.basename(normalized);
    if (/\.(dll|exe)$/i.test(fileName)) {
      binaries.add(fileName);
    }
  }

  return binaries;
}

const repoRoot = process.cwd();
const noticesPath = path.join(repoRoot, 'THIRD_PARTY_NOTICES.md');

if (!existsSync(noticesPath)) {
  console.error('Compliance check failed: THIRD_PARTY_NOTICES.md not found.');
  process.exit(1);
}

const noticesText = readFileSync(noticesPath, 'utf8').toLowerCase();
const expectedBinaries = new Set();

for (const relativePath of configFiles) {
  const absolutePath = path.join(repoRoot, relativePath);
  if (!existsSync(absolutePath)) {
    console.error(`Compliance check failed: missing config ${relativePath}`);
    process.exit(1);
  }

  for (const binary of collectBundledBinaries(absolutePath)) {
    expectedBinaries.add(binary);
  }
}

const missing = [...expectedBinaries]
  .filter((binary) => !noticesText.includes(binary.toLowerCase()))
  .sort((a, b) => a.localeCompare(b));

if (missing.length > 0) {
  console.error('Third-party compliance check failed. Missing bundled binaries in THIRD_PARTY_NOTICES.md:');
  for (const binary of missing) {
    console.error(`- ${binary}`);
  }
  process.exit(1);
}

console.log(`Third-party compliance check passed (${expectedBinaries.size} bundled binaries covered).`);
