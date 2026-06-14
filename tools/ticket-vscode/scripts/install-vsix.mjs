import { readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const extensionDir = resolve(scriptDir, '..');
const isDryRun = process.argv.includes('--dry-run');

function commandName(base) {
  if (process.platform !== 'win32') {
    return base;
  }

  if (base === 'npm') {
    return 'npm.cmd';
  }

  if (base === 'code') {
    return 'code.cmd';
  }

  return `${base}.cmd`;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: extensionDir,
    stdio: 'inherit',
    shell: process.platform === 'win32',
    ...options,
  });

  if (result.error) {
    throw new Error(`failed to run ${command}: ${result.error.message}`);
  }

  if (typeof result.status === 'number' && result.status !== 0) {
    throw new Error(`${command} exited with code ${result.status}`);
  }
}

function resolveCodeCommand() {
  const candidates = process.platform === 'win32'
    ? ['code.cmd', 'code-insiders.cmd', 'code']
    : ['code', 'code-insiders'];

  for (const candidate of candidates) {
    const probe = spawnSync(candidate, ['--version'], {
      cwd: extensionDir,
      stdio: 'ignore',
      shell: process.platform === 'win32',
    });

    if (!probe.error && probe.status === 0) {
      return candidate;
    }
  }

  throw new Error('could not find the VS Code CLI on PATH; install the `code` shell command first');
}

function newestVsixPath() {
  const candidates = readdirSync(extensionDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith('.vsix'))
    .map((entry) => {
      const path = join(extensionDir, entry.name);
      return {
        path,
        mtimeMs: statSync(path).mtimeMs,
      };
    });

  if (candidates.length === 0) {
    throw new Error('no .vsix file found after packaging');
  }

  candidates.sort((left, right) => left.mtimeMs - right.mtimeMs);
  return candidates[candidates.length - 1].path;
}

function main() {
  const npm = commandName('npm');
  const code = resolveCodeCommand();

  console.log('Packaging VSIX...');
  run(npm, ['run', 'package']);

  const vsixPath = newestVsixPath();
  console.log(`Newest VSIX: ${vsixPath}`);

  if (isDryRun) {
    console.log(`[dry-run] Would execute: ${code} --install-extension ${vsixPath} --force`);
    return;
  }

  console.log('Installing VSIX into VS Code...');
  run(code, ['--install-extension', vsixPath, '--force']);
  console.log('VSIX installed. Reload the VS Code window to activate the new build.');
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}