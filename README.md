# nyan-bit-detector

A command-line tool that extracts audio metadata from FLAC and WAV files and outputs the results as JSON.

Supports sample rate, channel count, bit depth, duration, and bitrate.

Written entirely in safe Rust, with no `unsafe` code or external crates. Audio files are inspected through bounds-checked header parsing without decoding the audio data.

## Usage

```bash
nyan-bit-detector <file.wav|file.flac> [more files...]
```

Output is a JSON array containing one object per file:

```json
[
  {
    "file": "song.flac",
    "format": "flac",
    "sample_rate_hz": 96000,
    "channels": 2,
    "bits_per_sample": 24,
    "duration_seconds": 2.000000,
    "duration": "00:02",
    "total_samples": 192000,
    "bitrate_bps": 782104,
    "byte_rate_bytes_per_sec": 97763,
    "file_size_bytes": 195526
  }
]
```

### Output Fields

- `bitrate_bps`: Bitrate in bits per second. WAV uses the nominal bitrate (`byte_rate × 8`), while FLAC uses the average bitrate (`file_size × 8 ÷ duration`), consistent with ffprobe.
- `byte_rate_bytes_per_sec`: Bytes per second. WAV uses the nominal byte rate from the file header, while FLAC uses the average (`file_size ÷ duration`). Always matches `bitrate_bps ÷ 8`.
- `duration`: Human-readable duration with zero-padded fields. For example, 234 seconds is represented as `"03:54"`, and durations of one hour or longer use the `"01:00:01"` format. Values are rounded to the nearest second.

### Error Handling

Invalid or corrupted files are reported as individual error objects without terminating the process:

```json
{"file": "broken.flac", "error": "..."}
```

Exit codes:

- `0` — All files processed successfully.
- `1` — One or more files failed.
- `2` — Invalid command-line arguments or usage error.

## Building

Build for the current platform:

```bash
cargo build --release
```

Build for all five supported targets:

- macOS ARM64
- Linux AMD64
- Linux ARM64
- Windows AMD64
- Windows ARM64

```bash
bash .claude/skills/nyan-bit-detector/scripts/build-all.sh
```

Compiled binaries are placed in `dist/`.

Linux binaries are statically linked against musl and can run across Linux distributions without additional runtime dependencies.

## Backend Integration

PHP and Node.js wrappers are available in `bridges/`.

Both wrappers execute the binary directly using argument arrays, without invoking a shell. This prevents shell injection through file names.

The appropriate binary is automatically selected from `dist/` based on the host platform. Set the `NYAN_BIT_DETECTOR_BIN` environment variable to override the binary path.

### Node.js

CommonJS, with no external dependencies.

```js
const { probe, probeOne } = require('./bridges/node/nyan-bit-detector');

const infos = await probe(['song.flac', 'take 1.wav']);
// Returns an array. Failed files contain {file, error}.

const one = await probeOne('song.flac');
// Returns a single object. Throws on failure.
```

### PHP

Requires PHP 7.4 or later. No additional extensions are required.

```php
require 'bridges/php/NyanBitDetector.php';

$det = new NyanBitDetector();

$infos = $det->probe(['song.flac', 'take 1.wav']);
$one = $det->probeOne('song.flac');
```

### Examples

Node.js:

```bash
node bridges/node/example.js <files...>
```

PHP:

```bash
php bridges/php/example.php <files...>
```