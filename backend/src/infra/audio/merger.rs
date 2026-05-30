use crate::shared::error::AppError;
use std::path::Path;
use std::fs;
use std::path::PathBuf;

/// Parsed WAV header information.
struct WavHeader {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    data_size: u32,
}

/// Read and parse the WAV header from a file.
fn read_wav_header(path: &Path) -> Result<WavHeader, AppError> {
    let bytes = fs::read(path)
        .map_err(|e| AppError::Internal(format!("Failed to read WAV file {}: {e}", path.display())))?;

    if bytes.len() < 44 {
        return Err(AppError::InvalidInput(format!(
            "WAV file too small ({} bytes): {}",
            bytes.len(),
            path.display()
        )));
    }

    // Validate RIFF header
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(AppError::InvalidInput(format!(
            "Not a valid WAV file: {}",
            path.display()
        )));
    }

    let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
    let sample_rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let bits_per_sample = u16::from_le_bytes([bytes[34], bytes[35]]);
    let data_size = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);

    Ok(WavHeader {
        sample_rate,
        channels,
        bits_per_sample,
        data_size,
    })
}

/// Extract raw audio data from a WAV file (skip 44-byte header).
fn get_wav_data(path: &Path) -> Result<Vec<u8>, AppError> {
    let bytes = fs::read(path)
        .map_err(|e| AppError::Internal(format!("Failed to read WAV file {}: {e}", path.display())))?;

    if bytes.len() < 44 {
        return Err(AppError::InvalidInput(format!(
            "WAV file too small ({} bytes): {}",
            bytes.len(),
            path.display()
        )));
    }

    Ok(bytes[44..].to_vec())
}

/// Calculate WAV duration in seconds.
fn get_wav_duration(path: &Path) -> Result<f64, AppError> {
    let header = read_wav_header(path)?;
    let bytes_per_second =
        header.sample_rate as f64 * header.channels as f64 * header.bits_per_sample as f64 / 8.0;

    if bytes_per_second <= 0.0 {
        return Err(AppError::InvalidInput("Invalid WAV header parameters".into()));
    }

    Ok(header.data_size as f64 / bytes_per_second)
}

/// Merge multiple WAV chunk files into a single output WAV file.
///
/// Returns (output_path, total_duration_seconds).
pub fn merge_wavs(chunk_paths: &[PathBuf], output_path: &Path) -> Result<(PathBuf, f64), AppError> {
    if chunk_paths.is_empty() {
        return Err(AppError::InvalidInput(
            "Cannot merge empty list of WAV chunks".into(),
        ));
    }

    if chunk_paths.len() == 1 {
        // Single chunk — copy directly
        let src = &chunk_paths[0];
        fs::copy(src, output_path).map_err(|e| {
            AppError::Internal(format!("Failed to copy {} to {}: {e}", src.display(), output_path.display()))
        })?;
        let duration = get_wav_duration(output_path)?;
        return Ok((output_path.to_path_buf(), duration));
    }

    // Multi-chunk merge
    let first_header = read_wav_header(&chunk_paths[0])?;

    // Verify all chunks have compatible headers
    for path in chunk_paths.iter().skip(1) {
        let h = read_wav_header(path)?;
        if h.sample_rate != first_header.sample_rate
            || h.channels != first_header.channels
            || h.bits_per_sample != first_header.bits_per_sample
        {
            return Err(AppError::InvalidInput(format!(
                "Incompatible WAV format in {}: expected {}Hz/{}ch/{}bit, got {}Hz/{}ch/{}bit",
                path.display(),
                first_header.sample_rate,
                first_header.channels,
                first_header.bits_per_sample,
                h.sample_rate,
                h.channels,
                h.bits_per_sample,
            )));
        }
    }

    // Read the first file's full content (header + data)
    let first_bytes = fs::read(&chunk_paths[0])
        .map_err(|e| AppError::Internal(format!("Failed to read {}: {e}", chunk_paths[0].display())))?;

    // Collect data sections from all chunks
    let mut total_data_size: u32 = first_header.data_size;
    let mut all_data = first_bytes[44..].to_vec(); // first chunk's data

    for path in chunk_paths.iter().skip(1) {
        let data = get_wav_data(path)?;
        total_data_size += data.len() as u32;
        all_data.extend_from_slice(&data);
    }

    // Build output: use first chunk's header and update sizes
    let mut output = first_bytes[0..44].to_vec(); // copy first 44 bytes

    // Update RIFF chunk size (bytes 4-7): file_size - 8
    let riff_size = 36 + total_data_size; // 36 = 44 - 8 (header without RIFF marker + size field)
    output[4..8].copy_from_slice(&riff_size.to_le_bytes());

    // Update data sub-chunk size (bytes 40-43)
    output[40..44].copy_from_slice(&total_data_size.to_le_bytes());

    // Append all data
    output.extend_from_slice(&all_data);

    // Write output file
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("Failed to create output directory: {e}")))?;
    }
    fs::write(output_path, &output)
        .map_err(|e| AppError::Internal(format!("Failed to write merged WAV {}: {e}", output_path.display())))?;

    // Calculate total duration
    let bytes_per_sample =
        first_header.sample_rate as f64 * first_header.channels as f64 * first_header.bits_per_sample as f64 / 8.0;
    let total_duration = total_data_size as f64 / bytes_per_sample;

    Ok((output_path.to_path_buf(), total_duration))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Create a synthetic WAV file with a sine wave of the given duration.
    fn create_test_wav(path: &Path, duration_secs: f64) -> PathBuf {
        const SAMPLE_RATE: u32 = 44100;
        const CHANNELS: u16 = 1;
        const BITS_PER_SAMPLE: u16 = 16;
        const BYTES_PER_SAMPLE: u16 = 2; // 16-bit = 2 bytes

        let num_samples = (SAMPLE_RATE as f64 * duration_secs) as usize;
        let data_size = (num_samples * CHANNELS as usize * BYTES_PER_SAMPLE as usize) as u32;
        let file_size = 36 + data_size;

        let mut buf = Vec::with_capacity(44 + data_size as usize);

        // RIFF header
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        // fmt sub-chunk
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        buf.extend_from_slice(&1u16.to_le_bytes());  // PCM
        buf.extend_from_slice(&CHANNELS.to_le_bytes());
        buf.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        let byte_rate = SAMPLE_RATE * CHANNELS as u32 * BYTES_PER_SAMPLE as u32;
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        let block_align = CHANNELS * BYTES_PER_SAMPLE;
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

        // data sub-chunk
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());

        // Sine wave data
        for i in 0..num_samples {
            let sample = (i16::MAX as f64 * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / SAMPLE_RATE as f64).sin()) as i16;
            buf.extend_from_slice(&sample.to_le_bytes());
        }

        let mut file = fs::File::create(path).unwrap();
        file.write_all(&buf).unwrap();
        path.to_path_buf()
    }

    #[test]
    fn test_merge_three_wavs() {
        let dir = std::env::temp_dir().join("test_merge_three_wavs");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let wav1 = create_test_wav(&dir.join("chunk1.wav"), 1.0);
        let wav2 = create_test_wav(&dir.join("chunk2.wav"), 1.0);
        let wav3 = create_test_wav(&dir.join("chunk3.wav"), 1.0);
        let output = dir.join("merged.wav");

        let (result_path, duration) = merge_wavs(&[wav1, wav2, wav3], &output).unwrap();

        assert_eq!(result_path, output);
        assert!(
            (duration - 3.0).abs() < 0.01,
            "Duration should be ~3.0s, got {duration}"
        );

        // Verify merged file is valid
        let header = read_wav_header(&output).unwrap();
        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.channels, 1);
        assert_eq!(header.bits_per_sample, 16);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_merge_single_wav() {
        let dir = std::env::temp_dir().join("test_merge_single_wav");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let wav = create_test_wav(&dir.join("single.wav"), 1.0);
        let output = dir.join("output.wav");

        let (result_path, duration) = merge_wavs(&[wav], &output).unwrap();

        assert_eq!(result_path, output);
        assert!(
            (duration - 1.0).abs() < 0.01,
            "Duration should be ~1.0s, got {duration}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_merge_empty_input() {
        let result = merge_wavs(&[], Path::new("dummy.wav"));
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("empty"), "Error should mention empty: {msg}");
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_read_wav_header_valid() {
        let dir = std::env::temp_dir().join("test_read_wav_header");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let path = create_test_wav(&dir.join("test.wav"), 0.5);
        let header = read_wav_header(&path).unwrap();

        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.channels, 1);
        assert_eq!(header.bits_per_sample, 16);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_get_wav_duration() {
        let dir = std::env::temp_dir().join("test_get_wav_duration");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let path = create_test_wav(&dir.join("dur_test.wav"), 2.0);
        let duration = get_wav_duration(&path).unwrap();

        assert!(
            (duration - 2.0).abs() < 0.01,
            "Duration should be ~2.0s, got {duration}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
