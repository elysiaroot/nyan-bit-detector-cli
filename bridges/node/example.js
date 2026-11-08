'use strict';
// Run: node example.js <audio files...>
const { probe } = require('./nyan-bit-detector');

async function main() {
  const files = process.argv.slice(2);
  if (files.length === 0) {
    console.error('usage: node example.js <file.wav|file.flac> ...');
    process.exit(2);
  }
  const infos = await probe(files);
  for (const info of infos) {
    if (info.error) {
      console.log(`✗ ${info.file}: ${info.error}`);
    } else {
      console.log(
        `✓ ${info.file}: ${info.format} ${info.sample_rate_hz} Hz, ` +
        `${info.channels}ch, ${info.bits_per_sample}-bit, ` +
        `${info.duration_seconds.toFixed(2)}s, ${Math.round(info.bitrate_bps / 1000)} kbps`
      );
    }
  }
}

main().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
