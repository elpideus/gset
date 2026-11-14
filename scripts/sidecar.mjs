#!/usr/bin/env node
// Builds the gset launcher and puts it where Tauri bundles it (bundle.externalBin in
// app/src-tauri/tauri.conf.json): app/src-tauri/binaries/gset-<target triple>. The app's
// beforeDevCommand and beforeBuildCommand run it, so the launcher always ships with the window.
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const host = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || host;
const cross = triple !== host;
execFileSync('cargo', ['build', '--release', '--locked', '-p', 'gset-cli', ...(cross ? ['--target', triple] : [])], { cwd: root, stdio: 'inherit' });
const built = path.join(root, 'target', ...(cross ? [triple] : []), 'release', 'gset');
const dir = path.join(root, 'app', 'src-tauri', 'binaries');
mkdirSync(dir, { recursive: true });
copyFileSync(built, path.join(dir, `gset-${triple}`));
console.log(`The launcher is at app/src-tauri/binaries/gset-${triple}`);
