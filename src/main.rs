// Prints FLAC/WAV metadata as JSON. Header parsing only, no decoding.
#![forbid(unsafe_code)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read;
use std::process::ExitCode;

const MAX_HEADER_BYTES: usize = 16 * 1024 * 1024; // refuse to scan headers past 16 MiB

struct AudioInfo {
    format: &'static str,
    sample_rate_hz: u32,
    channels: u32,
    bits_per_sample: u32,
    total_samples: u64,
    duration_seconds: f64,
    bitrate_bps: u64,
    byte_rate_bytes_per_sec: u64,
    file_size_bytes: u64,
}

fn main() -> ExitCode {
    // args_os: file names aren't guaranteed to be valid UTF-8.
    // "--" ends option parsing so a file literally named "-h" still works.
    let mut files: Vec<OsString> = Vec::new();
    let mut opts_done = false;
    let mut want_help = false;
    for a in env::args_os().skip(1) {
        if !opts_done {
            if a == "--" {
                opts_done = true;
                continue;
            }
            if a == "-h" || a == "--help" {
                want_help = true;
                continue;
            }
        }
        files.push(a);
    }
    if want_help || files.is_empty() {
        eprintln!("usage: nyan-bit-detector [--] <file.wav|file.flac> [more files...]");
        eprintln!("Prints a JSON array with sample rate, channels, bit depth, duration and bitrate.");
        return ExitCode::from(2);
    }

    let mut out = String::from("[\n");
    let mut any_error = false;

    for (i, path) in files.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        let name = path.to_string_lossy();
        match probe_file(path) {
            Ok(info) => write_info_json(&mut out, &name, &info),
            Err(msg) => {
                any_error = true;
                out.push_str("  {\n    \"file\": ");
                write_json_string(&mut out, &name);
                out.push_str(",\n    \"error\": ");
                write_json_string(&mut out, &msg);
                out.push_str("\n  }");
            }
        }
    }
    out.push_str("\n]");
    println!("{out}");

    if any_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn probe_file(path: &OsStr) -> Result<AudioInfo, String> {
    let mut f = File::open(path).map_err(|e| format!("cannot open: {e}"))?;
    let file_size = f
        .metadata()
        .map_err(|e| format!("cannot stat: {e}"))?
        .len();

    let to_read = file_size.min(MAX_HEADER_BYTES as u64) as usize;
    let mut buf = vec![0u8; to_read];
    f.read_exact(&mut buf)
        .map_err(|e| format!("cannot read: {e}"))?;

    if buf.len() >= 12 && &buf[0..4] == b"RIFF" && &buf[8..12] == b"WAVE" {
        parse_wav(&buf, file_size)
    } else if flac_start(&buf).is_some() {
        parse_flac(&buf, file_size)
    } else {
        Err("not a WAV or FLAC file (unrecognized signature)".to_string())
    }
}

// ---------- WAV (RIFF) ----------

fn parse_wav(buf: &[u8], file_size: u64) -> Result<AudioInfo, String> {
    let mut pos: usize = 12; // past RIFF....WAVE
    let mut fmt: Option<(u16, u16, u32, u32, u16)> = None; // tag, channels, rate, byte_rate, bits
    let mut data_size: Option<u64> = None;
    let mut data_offset: u64 = 0;

    while pos + 8 <= buf.len() {
        let id = &buf[pos..pos + 4];
        let size = u32::from_le_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]) as u64;
        let body = pos + 8;

        match id {
            b"fmt " => {
                if size < 16 || body + 16 > buf.len() {
                    return Err("malformed fmt chunk".to_string());
                }
                let b = &buf[body..body + 16];
                fmt = Some((
                    u16::from_le_bytes([b[0], b[1]]),
                    u16::from_le_bytes([b[2], b[3]]),
                    u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                    u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
                    u16::from_le_bytes([b[14], b[15]]),
                ));
            }
            b"data" => {
                data_offset = body as u64;
                // streamed files sometimes write 0 or 0xFFFFFFFF here
                let claimed = size;
                let available = file_size.saturating_sub(body as u64);
                data_size = Some(if claimed == 0 || claimed == 0xFFFF_FFFF || claimed > available {
                    available
                } else {
                    claimed
                });
                if fmt.is_some() {
                    break;
                }
            }
            _ => {}
        }

        // chunks are word-aligned, odd sizes carry a pad byte
        let advance = size.checked_add(size & 1).and_then(|s| s.checked_add(8));
        match advance {
            Some(a) if (pos as u64).checked_add(a).is_some() => {
                let next = pos as u64 + a;
                if next > buf.len() as u64 {
                    break;
                }
                pos = next as usize;
            }
            _ => break,
        }
    }

    let (tag, channels, sample_rate, byte_rate, bits) =
        fmt.ok_or_else(|| "no fmt chunk found".to_string())?;
    let data_size = data_size.ok_or_else(|| "no data chunk found".to_string())?;
    let _ = data_offset;

    if channels == 0 || sample_rate == 0 {
        return Err("fmt chunk reports zero channels or sample rate".to_string());
    }
    // 1 = PCM, 3 = IEEE float, 0xFFFE = extensible (wraps PCM/float)
    if !matches!(tag, 1 | 3 | 0xFFFE) {
        return Err(format!("unsupported WAV codec (format tag {tag}); only PCM/float WAV is supported"));
    }

    let effective_byte_rate = if byte_rate > 0 {
        byte_rate as u64
    } else {
        sample_rate as u64 * channels as u64 * (bits as u64 / 8).max(1)
    };
    let duration = data_size as f64 / effective_byte_rate as f64;
    let block = (channels as u64) * (bits as u64 / 8).max(1);
    let total_samples = if block > 0 { data_size / block } else { 0 };

    Ok(AudioInfo {
        format: "wav",
        sample_rate_hz: sample_rate,
        channels: channels as u32,
        bits_per_sample: bits as u32,
        total_samples,
        duration_seconds: duration,
        bitrate_bps: effective_byte_rate * 8,
        byte_rate_bytes_per_sec: effective_byte_rate,
        file_size_bytes: file_size,
    })
}

// ---------- FLAC ----------

/// Returns the offset of the "fLaC" magic, skipping a leading ID3v2 tag if present.
fn flac_start(buf: &[u8]) -> Option<usize> {
    if buf.len() >= 4 && &buf[0..4] == b"fLaC" {
        return Some(0);
    }
    // ID3v2 header: "ID3" ver(2) flags(1) size(4, syncsafe)
    if buf.len() >= 10 && &buf[0..3] == b"ID3" {
        let size = ((buf[6] as usize & 0x7F) << 21)
            | ((buf[7] as usize & 0x7F) << 14)
            | ((buf[8] as usize & 0x7F) << 7)
            | (buf[9] as usize & 0x7F);
        let off = 10usize.checked_add(size)?;
        if buf.len() >= off + 4 && &buf[off..off + 4] == b"fLaC" {
            return Some(off);
        }
    }
    None
}

fn parse_flac(buf: &[u8], file_size: u64) -> Result<AudioInfo, String> {
    let start = flac_start(buf).ok_or_else(|| "not a FLAC file".to_string())?;
    let mut pos = start + 4;

    // Walk metadata blocks looking for STREAMINFO (type 0).
    loop {
        if pos + 4 > buf.len() {
            return Err("truncated FLAC metadata".to_string());
        }
        let header = buf[pos];
        let is_last = header & 0x80 != 0;
        let block_type = header & 0x7F;
        let len = ((buf[pos + 1] as usize) << 16) | ((buf[pos + 2] as usize) << 8) | buf[pos + 3] as usize;
        let body = pos + 4;

        if block_type == 0 {
            if len < 34 || body + 34 > buf.len() {
                return Err("malformed STREAMINFO block".to_string());
            }
            let b = &buf[body..body + 34];
            // Bytes 10..18 pack: sample_rate(20) channels-1(3) bits-1(5) total_samples(36)
            let sample_rate = ((b[10] as u32) << 12) | ((b[11] as u32) << 4) | ((b[12] as u32) >> 4);
            let channels = (((b[12] >> 1) & 0x07) as u32) + 1;
            let bits = ((((b[12] & 0x01) as u32) << 4) | ((b[13] as u32) >> 4)) + 1;
            let total_samples = (((b[13] & 0x0F) as u64) << 32)
                | ((b[14] as u64) << 24)
                | ((b[15] as u64) << 16)
                | ((b[16] as u64) << 8)
                | (b[17] as u64);

            if sample_rate == 0 {
                return Err("STREAMINFO reports zero sample rate".to_string());
            }
            let duration = total_samples as f64 / sample_rate as f64;
            // FLAC is VBR, so this is the whole-file average
            let bitrate = if duration > 0.0 {
                ((file_size as f64 * 8.0) / duration).round() as u64
            } else {
                0
            };
            let byte_rate = if duration > 0.0 {
                (file_size as f64 / duration).round() as u64
            } else {
                0
            };

            return Ok(AudioInfo {
                format: "flac",
                sample_rate_hz: sample_rate,
                channels,
                bits_per_sample: bits,
                total_samples,
                duration_seconds: duration,
                bitrate_bps: bitrate,
                byte_rate_bytes_per_sec: byte_rate,
                file_size_bytes: file_size,
            });
        }

        if is_last {
            return Err("FLAC file has no STREAMINFO block".to_string());
        }
        pos = body
            .checked_add(len)
            .ok_or_else(|| "malformed FLAC block length".to_string())?;
    }
}

// ---------- JSON output ----------

/// 234s -> "03:54", 3601s -> "01:00:01"
fn format_duration(secs: f64) -> String {
    let total = if secs.is_finite() && secs > 0.0 {
        secs.round() as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn write_info_json(out: &mut String, path: &str, info: &AudioInfo) {
    out.push_str("  {\n    \"file\": ");
    write_json_string(out, path);
    let _ = write!(
        out,
        ",\n    \"format\": \"{}\",\n    \"sample_rate_hz\": {},\n    \"channels\": {},\n    \"bits_per_sample\": {},\n    \"duration_seconds\": {:.6},\n    \"duration\": \"{}\",\n    \"total_samples\": {},\n    \"bitrate_bps\": {},\n    \"byte_rate_bytes_per_sec\": {},\n    \"file_size_bytes\": {}\n  }}",
        info.format,
        info.sample_rate_hz,
        info.channels,
        info.bits_per_sample,
        info.duration_seconds,
        format_duration(info.duration_seconds),
        info.total_samples,
        info.bitrate_bps,
        info.byte_rate_bytes_per_sec,
        info.file_size_bytes
    );
}

fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
