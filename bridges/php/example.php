<?php
// Run: php example.php <audio files...>
require __DIR__ . '/NyanBitDetector.php';

$files = array_slice($argv, 1);
if ($files === []) {
    fwrite(STDERR, "usage: php example.php <file.wav|file.flac> ...\n");
    exit(2);
}

$det = new NyanBitDetector();
foreach ($det->probe($files) as $info) {
    if (isset($info['error'])) {
        echo "✗ {$info['file']}: {$info['error']}\n";
    } else {
        printf(
            "✓ %s: %s %d Hz, %dch, %d-bit, %.2fs, %d kbps\n",
            $info['file'],
            $info['format'],
            $info['sample_rate_hz'],
            $info['channels'],
            $info['bits_per_sample'],
            $info['duration_seconds'],
            (int) round($info['bitrate_bps'] / 1000)
        );
    }
}
