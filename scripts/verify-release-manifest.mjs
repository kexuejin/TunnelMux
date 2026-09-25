#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { readFileSync, statSync } from 'node:fs';
import { basename, join, relative, resolve } from 'node:path';
import process from 'node:process';

const distDir = resolve(process.argv[2] ?? 'dist');
const manifestPath = join(distDir, 'tunnelmux-latest.json');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));

if (!manifest.version || !manifest.tag || !Array.isArray(manifest.assets) || manifest.assets.length === 0) {
  throw new Error('release manifest is missing version/tag/assets');
}
if (manifest.tag !== `v${manifest.version}`) {
  throw new Error(`release tag ${manifest.tag} does not match version ${manifest.version}`);
}
if (
  typeof manifest.release_url !== 'string' ||
  !manifest.release_url.endsWith(`/releases/tag/${manifest.tag}`)
) {
  throw new Error(`release URL does not point to ${manifest.tag}`);
}

const checksumText = readFileSync(join(distDir, 'SHA256SUMS'), 'utf8');
const checksums = new Map(
  checksumText
    .split(/\r?\n/)
    .map((line) => line.trim().split(/\s+/))
    .filter((parts) => parts.length >= 2)
    .map(([hash, name]) => [name.replace(/^\*/, ''), hash.toLowerCase()]),
);

for (const asset of manifest.assets) {
  if (!asset.name || basename(asset.name) !== asset.name || asset.name.includes('..')) {
    throw new Error(`unsafe release asset name: ${asset.name}`);
  }
  const expectedUrl = `${manifest.release_url.replace('/releases/tag/', '/releases/download/')}/${asset.name}`;
  if (asset.url !== expectedUrl) {
    throw new Error(`release URL mismatch for ${asset.name}: expected ${expectedUrl}`);
  }
  const assetPath = join(distDir, asset.name);
  if (relative(distDir, assetPath).startsWith('..')) {
    throw new Error(`release asset escapes dist: ${asset.name}`);
  }
  const size = statSync(assetPath).size;
  if (asset.size !== size) {
    throw new Error(`size mismatch for ${asset.name}: expected ${asset.size}, got ${size}`);
  }
  const expected = asset.sha256 ?? checksums.get(asset.name);
  if (!expected || !/^[0-9a-f]{64}$/i.test(expected)) {
    throw new Error(`missing or invalid checksum for ${asset.name}`);
  }
  const actual = createHash('sha256').update(readFileSync(assetPath)).digest('hex');
  if (actual !== expected.toLowerCase()) {
    throw new Error(`checksum mismatch for ${asset.name}`);
  }
  if (asset.kind === 'raw_archive' && !/\.(tar\.gz|zip)$/.test(asset.name)) {
    throw new Error(`raw archive has an unsupported extension: ${asset.name}`);
  }
}

console.log(`verified ${manifest.assets.length} release assets in ${distDir}`);
