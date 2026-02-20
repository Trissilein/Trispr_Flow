import { existsSync, readdirSync } from 'node:fs';
import path from 'node:path';

const allowedRootMarkdown = new Set([
  'README.md',
  'ROADMAP.md',
  'CHANGELOG.md',
  'CONTRIBUTING.md',
  'CLAUDE.md',
  'THIRD_PARTY_NOTICES.md',
]);

const root = process.cwd();
const rootEntries = readdirSync(root, { withFileTypes: true });
const rootMarkdownFiles = rootEntries
  .filter((entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.md'))
  .map((entry) => entry.name)
  .sort();

const unexpectedRootMarkdown = rootMarkdownFiles.filter((file) => !allowedRootMarkdown.has(file));
const missingRequiredRootMarkdown = [...allowedRootMarkdown].filter((file) => !rootMarkdownFiles.includes(file));

const canonicalDocsGuide = path.join(root, 'docs', 'README.md');
const hasDocsGuide = existsSync(canonicalDocsGuide);

if (unexpectedRootMarkdown.length > 0 || missingRequiredRootMarkdown.length > 0 || !hasDocsGuide) {
  console.error('Documentation governance check failed.');

  if (unexpectedRootMarkdown.length > 0) {
    console.error('\nUnexpected root markdown files:');
    for (const file of unexpectedRootMarkdown) {
      console.error(`- ${file}`);
    }
    console.error('\nMove these files under docs/ (or docs/archive/) and link from canonical docs.');
  }

  if (missingRequiredRootMarkdown.length > 0) {
    console.error('\nMissing required root markdown files:');
    for (const file of missingRequiredRootMarkdown) {
      console.error(`- ${file}`);
    }
  }

  if (!hasDocsGuide) {
    console.error('\nMissing canonical docs guide: docs/README.md');
  }

  process.exit(1);
}

console.log('Documentation governance check passed.');
