<?php

declare(strict_types=1);

/**
 * Wrapper around the nyan-bit-detector binary. Requires PHP >= 7.4.
 *
 * Uses proc_open with an argv array (no shell), so file names are never
 * interpreted as shell syntax. Binary lookup: constructor argument, then the
 * NYAN_BIT_DETECTOR_BIN env var, then ../../dist/ by platform.
 */
final class NyanBitDetector
{
    private string $binary;
    private int $timeoutSeconds;

    public function __construct(?string $binaryPath = null, int $timeoutSeconds = 30)
    {
        $this->binary = $this->resolveBinary($binaryPath);
        $this->timeoutSeconds = $timeoutSeconds;
    }

    /**
     * Probe one or more FLAC/WAV files. Returns one entry per file; a file
     * that failed to parse comes back as ['file' => ..., 'error' => ...]
     * rather than throwing.
     *
     * @param string[] $files
     * @return array<int, array<string, mixed>>
     */
    public function probe(array $files): array
    {
        if ($files === []) {
            throw new InvalidArgumentException('probe(): pass at least one file path');
        }

        // "--" keeps a file named "-h" from being parsed as an option
        $cmd = array_merge([$this->binary, '--'], array_map('strval', $files));
        $descriptors = [
            0 => ['pipe', 'r'],
            1 => ['pipe', 'w'],
            2 => ['pipe', 'w'],
        ];

        $proc = proc_open($cmd, $descriptors, $pipes);
        if (!\is_resource($proc)) {
            throw new RuntimeException("could not start {$this->binary}");
        }
        fclose($pipes[0]);
        stream_set_blocking($pipes[1], false);
        stream_set_blocking($pipes[2], false);

        $stdout = '';
        $stderr = '';
        $deadline = microtime(true) + $this->timeoutSeconds;
        while (true) {
            $stdout .= (string) stream_get_contents($pipes[1]);
            $stderr .= (string) stream_get_contents($pipes[2]);
            $status = proc_get_status($proc);
            if (!$status['running']) {
                break;
            }
            if (microtime(true) > $deadline) {
                proc_terminate($proc, 9);
                proc_close($proc);
                throw new RuntimeException('nyan-bit-detector timed out');
            }
            usleep(10000);
        }
        $stdout .= (string) stream_get_contents($pipes[1]);
        $stderr .= (string) stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        proc_close($proc);
        $exitCode = $status['exitcode'];

        // exit 1 = some files failed; the JSON still has per-file errors
        if ($exitCode !== 0 && $exitCode !== 1) {
            throw new RuntimeException(
                "nyan-bit-detector exited with code {$exitCode}: " . trim($stderr)
            );
        }

        $data = json_decode($stdout, true);
        if (!\is_array($data)) {
            throw new RuntimeException(
                'could not parse nyan-bit-detector output: ' . json_last_error_msg()
            );
        }
        return $data;
    }

    /**
     * Probe a single file. Throws if it could not be parsed.
     *
     * @return array<string, mixed>
     */
    public function probeOne(string $file): array
    {
        $entry = $this->probe([$file])[0] ?? null;
        if ($entry === null) {
            throw new RuntimeException("no result returned for {$file}");
        }
        if (isset($entry['error'])) {
            throw new RuntimeException("{$file}: {$entry['error']}");
        }
        return $entry;
    }

    private function resolveBinary(?string $binaryPath): string
    {
        $explicit = $binaryPath ?? (getenv('NYAN_BIT_DETECTOR_BIN') ?: null);
        if ($explicit !== null) {
            if (!is_file($explicit)) {
                throw new RuntimeException("nyan-bit-detector binary not found at {$explicit}");
            }
            return $explicit;
        }

        $machine = strtolower(php_uname('m'));
        $arm = \in_array($machine, ['arm64', 'aarch64'], true);
        switch (PHP_OS_FAMILY) {
            case 'Darwin':
                $name = $arm ? 'nyan-bit-detector-macos-arm64' : null;
                break;
            case 'Linux':
                $name = $arm ? 'nyan-bit-detector-linux-arm64' : 'nyan-bit-detector-linux-amd64';
                break;
            case 'Windows':
                $name = $arm ? 'nyan-bit-detector-windows-arm64.exe' : 'nyan-bit-detector-windows-amd64.exe';
                break;
            default:
                $name = null;
        }
        if ($name === null) {
            throw new RuntimeException(
                'no prebuilt binary for ' . PHP_OS_FAMILY . "/{$machine}; set NYAN_BIT_DETECTOR_BIN"
            );
        }

        $candidate = \dirname(__DIR__, 2) . '/dist/' . $name;
        if (!is_file($candidate)) {
            throw new RuntimeException(
                "binary not found at {$candidate}; build it (build-all.sh) or set NYAN_BIT_DETECTOR_BIN"
            );
        }
        return $candidate;
    }
}
