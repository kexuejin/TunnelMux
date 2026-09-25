import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

const scriptPath = new URL('./verify-release-manifest.mjs', import.meta.url).pathname;

test('release manifest verifier checks paths, sizes, and checksums', () => {
  const dir = mkdtempSync(join(tmpdir(), 'tunnelmux-release-manifest-'));
  const asset = 'tunnelmux-9.9.9-x86_64-apple-darwin.tar.gz';
  const body = Buffer.from('release-fixture');
  writeFileSync(join(dir, asset), body);
  const hash = createHash('sha256').update(body).digest('hex');
  writeFileSync(join(dir, 'SHA256SUMS'), `${hash}  ${asset}\n`);
  writeFileSync(
    join(dir, 'tunnelmux-latest.json'),
    JSON.stringify({
      version: '9.9.9',
      tag: 'v9.9.9',
      assets: [{ name: asset, size: body.length, sha256: hash, kind: 'raw_archive' }],
    }),
  );

  const result = spawnSync(process.execPath, [scriptPath, dir], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr || result.stdout);

  writeFileSync(join(dir, asset), Buffer.from('tampered'));
  const tampered = spawnSync(process.execPath, [scriptPath, dir], { encoding: 'utf8' });
  assert.notEqual(tampered.status, 0);
});
