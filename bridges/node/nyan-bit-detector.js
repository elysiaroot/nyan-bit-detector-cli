'use strict';
/**
 * Wrapper around the nyan-bit-detector binary.
 *
 * Uses execFile (argv array, no shell), so file names are never interpreted
 * as shell syntax. Binary lookup: options.binaryPath, then the
 * NYAN_BIT_DETECTOR_BIN env var, then ../../dist/ by platform.
 */

const { execFile } = require('child_process');
const fs = require('fs');
const path = require('path');

const PLATFORM_MAP = {
  'darwin-arm64': 'nyan-bit-detector-macos-arm64',
  'linux-x64': 'nyan-bit-detector-linux-amd64',
  'linux-arm64': 'nyan-bit-detector-linux-arm64',
  'win32-x64': 'nyan-bit-detector-windows-amd64.exe',
  'win32-arm64': 'nyan-bit-detector-windows-arm64.exe',
};

function resolveBinary(binaryPath) {
  const explicit = binaryPath || process.env.NYAN_BIT_DETECTOR_BIN;
  if (explicit) {
    if (!fs.existsSync(explicit)) {
      throw new Error(`nyan-bit-detector binary not found at ${explicit}`);
    }
    return explicit;
  }
  const key = `${process.platform}-${process.arch}`;
  const name = PLATFORM_MAP[key];
  if (!name) {
    throw new Error(
      `no prebuilt nyan-bit-detector binary for ${key}; ` +
      'set NYAN_BIT_DETECTOR_BIN or pass options.binaryPath'
    );
  }
  const candidate = path.join(__dirname, '..', '..', 'dist', name);
  if (!fs.existsSync(candidate)) {
    throw new Error(
      `binary not found at ${candidate}; build it (build-all.sh) ` +
      'or set NYAN_BIT_DETECTOR_BIN'
    );
  }
  return candidate;
}

/**
 * Probe one or more FLAC/WAV files. Resolves to one entry per file; a file
 * that failed to parse comes back as {file, error} rather than rejecting.
 * @param {string[]} files
 * @param {{binaryPath?: string, timeoutMs?: number}} [options]
 * @returns {Promise<Array<object>>}
 */
function probe(files, options = {}) {
  if (!Array.isArray(files) || files.length === 0) {
    return Promise.reject(new Error('probe(files): pass a non-empty array of paths'));
  }
  const bin = resolveBinary(options.binaryPath);
  return new Promise((resolve, reject) => {
    execFile(
      bin,
      // "--" keeps a file named "-h" from being parsed as an option
      ['--', ...files.map(String)],
      {
        timeout: options.timeoutMs || 30000,
        maxBuffer: 16 * 1024 * 1024,
        windowsHide: true,
      },
      (err, stdout) => {
        // exit 1 = some files failed; the JSON still has per-file errors
        if (err && err.code !== 1) {
          return reject(new Error(`nyan-bit-detector failed: ${err.message}`));
        }
        try {
          resolve(JSON.parse(stdout));
        } catch (e) {
          reject(new Error(`could not parse nyan-bit-detector output: ${e.message}`));
        }
      }
    );
  });
}

/** Probe a single file. Rejects if it could not be parsed. */
async function probeOne(file, options = {}) {
  const [entry] = await probe([file], options);
  if (entry && entry.error) {
    throw new Error(`${file}: ${entry.error}`);
  }
  return entry;
}

module.exports = { probe, probeOne, resolveBinary };
