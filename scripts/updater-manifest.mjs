#!/usr/bin/env node
// Writes latest.json, which installed copies of GSet read to learn of a new version
// (`plugins.updater.endpoints` in app/src-tauri/tauri.conf.json points at the latest release's
// copy of it). It names, for each kind of installation, the file to download from this release
// and that file's signature.
//
//   node scripts/updater-manifest.mjs <version> <folder> <notes-file> > latest.json
//
// The folder holds the signed release files and their .sig files, as the release workflow's sign
// job leaves them.
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { SEMVER } from './version.mjs';

const REPO = 'https://github.com/elpideus/gset';

/** The release's files, by the updater's target names `{os}-{arch}-{installer}`. */
export const UPDATES = {
  'linux-x86_64-appimage': 'GSet-x86_64.AppImage',
  'linux-x86_64-deb': 'gset_amd64.deb',
  'linux-x86_64-rpm': 'gset.x86_64.rpm',
};

export function manifest(version, folder, notes, date = new Date()) {
  const platforms = {};
  for (const [target, file] of Object.entries(UPDATES)) {
    const signature = path.join(folder, `${file}.sig`);
    if (!existsSync(signature)) throw new Error(`${file}.sig is missing: the release would not update ${target}`);
    platforms[target] = {
      url: `${REPO}/releases/download/v${version}/${file}`,
      signature: readFileSync(signature, 'utf8').trim(),
    };
  }
  return { version, notes, pub_date: date.toISOString(), platforms };
}

if (process.argv[1]?.endsWith('updater-manifest.mjs')) {
  const [version, folder, notesFile] = process.argv.slice(2);
  if (!version || !SEMVER.test(version) || !folder || !notesFile) {
    console.error('Usage: node scripts/updater-manifest.mjs <version> <folder> <notes-file>');
    process.exit(1);
  }
  const notes = readFileSync(notesFile, 'utf8').trim();
  process.stdout.write(`${JSON.stringify(manifest(version, folder, notes), null, 2)}\n`);
}
