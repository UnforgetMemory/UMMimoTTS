//! Audio math — official MiMo-V2.5-TTS output spec:
//! **24 kHz · mono · PCM16LE** (official streaming example writes
//! `sf.write(..., samplerate=24000)`).
//!
//! ADR-006: streaming `pcm16` chunks are decoded and appended as raw PCM,
//! then wrapped in a single WAV header. Duration is byte-exact:
//! `bytes / (24000 × 2)` — no more v3 hardcoded 0.5s durations.

pub const SAMPLE_RATE: u32 = 24_000;
pub const CHANNELS: u16 = 1;
pub const BITS_PER_SAMPLE: u16 = 16;
pub const BYTES_PER_SAMPLE: u32 = 2; // (BITS_PER_SAMPLE / 8) × CHANNELS
pub const WAV_HEADER_LEN: usize = 44;

/// Build a canonical 44-byte RIFF/WAVE header for PCM16LE mono data.
pub fn wav_header(data_len: u32) -> [u8; WAV_HEADER_LEN] {
    let byte_rate = SAMPLE_RATE * BYTES_PER_SAMPLE;
    let block_align = BYTES_PER_SAMPLE as u16;
    let file_size = 36u32 + data_len;
    let mut h = [0u8; WAV_HEADER_LEN];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&file_size.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&CHANNELS.to_le_bytes());
    h[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

/// Wrap a raw PCM16LE mono buffer into a complete WAV file (byte math only).
pub fn wrap_pcm16_to_wav(pcm: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pcm.len() + WAV_HEADER_LEN);
    out.extend_from_slice(&wav_header(pcm.len() as u32));
    out.extend_from_slice(pcm);
    out
}

/// Exact duration of PCM16LE mono data in milliseconds.
pub fn pcm16_duration_ms(data_len: usize) -> u64 {
    (data_len as u64) * 1000 / (SAMPLE_RATE as u64 * BYTES_PER_SAMPLE as u64)
}

/// Find the `data` sub-chunk range inside a RIFF/WAVE byte slice.
/// Returns `(offset, length)` if the file is a valid WAV with a `data` chunk.
/// This is tolerant of chunks before `data` (streaming sources sometimes
/// include LIST/fact chunks after a 44-byte canonical header).
pub fn find_wav_data_range(bytes: &[u8]) -> Option<(usize, usize)> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]])
            as usize;
        let body = pos + 8;
        if id == b"data" {
            let avail = bytes.len().saturating_sub(body);
            return Some((body, avail.min(size)));
        }
        pos = body + size + (size & 1); // chunks are word-aligned
        if pos >= bytes.len() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_field_values() {
        let h = wav_header(48000);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(&h[8..12], b"WAVE");
        assert_eq!(&h[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([h[22], h[23]]), 1); // mono
        assert_eq!(u32::from_le_bytes([h[24], h[25], h[26], h[27]]), 24000);
        assert_eq!(u16::from_le_bytes([h[34], h[35]]), 16);
        assert_eq!(u32::from_le_bytes([h[40], h[41], h[42], h[43]]), 48000);
    }

    #[test]
    fn wrap_and_parse_roundtrip() {
        let pcm = vec![0u8; 48000 * 2]; // 1 second
        let wav = wrap_pcm16_to_wav(&pcm);
        assert_eq!(wav.len(), 48000 * 2 + 44);
        let (off, len) = find_wav_data_range(&wav).unwrap();
        assert_eq!((off, len), (44, 48000 * 2));
    }

    #[test]
    fn duration_is_byte_exact() {
        assert_eq!(pcm16_duration_ms(48000), 1000); // 48000 B/s
        assert_eq!(pcm16_duration_ms(24000), 500);
    }

    #[test]
    fn non_wav_rejected() {
        assert!(find_wav_data_range(b"MP3 frame garbage.....").is_none());
        assert!(find_wav_data_range(b"RIFF\x04\x00\x00\x00WAV").is_none());
    }
}
