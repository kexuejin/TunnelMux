import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, chmodSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

test('package-local-release-archive creates the current raw archive layout from local binaries', () => {
  const repoRoot = fileURLToPath(new URL('../', import.meta.url));
  const scriptPath = fileURLToPath(new URL('./package-local-release-archive.sh', import.meta.url));
  const workDir = mkdtempSync(path.join(tmpdir(), 'tunnelmux-local-release-'));
  const releaseDir = path.join(workDir, 'release');
  const distDir = path.join(workDir, 'dist');

  mkdirSync(releaseDir, { recursive: true });
  for (const name of ['tunnelmuxd', 'tunnelmux-cli', 'tunnelmux-gui']) {
    const binaryPath = path.join(releaseDir, name);
    writeFileSync(binaryPath, `#!/usr/bin/env bash\necho ${name}\n`);
    chmodSync(binaryPath, 0o755);
  }

  const result = spawnSync('bash', [scriptPath, distDir], {
    cwd: repoRoot,
    env: {
      ...process.env,
      TUNNELMUX_RELEASE_DIR: releaseDir,
      TUNNELMUX_TARGET: 'test-target',
      TUNNELMUX_VERSION: '9.9.9-test',
    },
    encoding: 'utf8',
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);

  const archivePath = path.join(distDir, 'tunnelmux-9.9.9-test-test-target.tar.gz');
  const checksumPath = path.join(distDir, 'SHA256SUMS');
  assert.equal(existsSync(archivePath), true);
  assert.equal(existsSync(checksumPath), true);

  const listing = spawnSync('tar', ['-tzf', archivePath], { encoding: 'utf8' });
  assert.equal(listing.status, 0, listing.stderr || listing.stdout);
  assert.match(listing.stdout, /tunnelmux-9\.9\.9-test-test-target\/tunnelmuxd/);
  assert.match(listing.stdout, /tunnelmux-9\.9\.9-test-test-target\/tunnelmux-cli/);
  assert.match(listing.stdout, /tunnelmux-9\.9\.9-test-test-target\/tunnelmux-gui/);
  assert.match(listing.stdout, /tunnelmux-9\.9\.9-test-test-target\/README\.md/);
  assert.match(listing.stdout, /tunnelmux-9\.9\.9-test-test-target\/README\.zh-CN\.md/);
  assert.match(listing.stdout, /tunnelmux-9\.9\.9-test-test-target\/LICENSE/);
  assert.match(listing.stdout, /tunnelmux-9\.9\.9-test-test-target\/CHANGELOG\.md/);
});

test('package-local-release-archive trims the derived host target in the archive name', () => {
  const repoRoot = fileURLToPath(new URL('../', import.meta.url));
  const scriptPath = fileURLToPath(new URL('./package-local-release-archive.sh', import.meta.url));
  const workDir = mkdtempSync(path.join(tmpdir(), 'tunnelmux-local-release-'));
  const releaseDir = path.join(workDir, 'release');
  const distDir = path.join(workDir, 'dist');
  const hostTarget = spawnSync('rustc', ['-vV'], { encoding: 'utf8' }).stdout
    .split('\n')
    .find((line) => line.startsWith('host:'))
    .split(':')[1]
    .trim();

  mkdirSync(releaseDir, { recursive: true });
  for (const name of ['tunnelmuxd', 'tunnelmux-cli', 'tunnelmux-gui']) {
    const binaryPath = path.join(releaseDir, name);
    writeFileSync(binaryPath, `#!/usr/bin/env bash\necho ${name}\n`);
    chmodSync(binaryPath, 0o755);
  }

  const result = spawnSync('bash', [scriptPath, distDir], {
    cwd: repoRoot,
    env: {
      ...process.env,
      TUNNELMUX_RELEASE_DIR: releaseDir,
      TUNNELMUX_VERSION: '9.9.9-host',
    },
    encoding: 'utf8',
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.equal(
    existsSync(path.join(distDir, `tunnelmux-9.9.9-host-${hostTarget}.tar.gz`)),
    true,
  );
});
