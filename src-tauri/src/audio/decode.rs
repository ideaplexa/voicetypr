//! Pure-Rust audio normalization to the canonical 16 kHz / mono / signed-16-bit
//! PCM WAV consumed by Whisper, Parakeet, and the cloud transcription path.
//!
//! All decoding is in-process: every container/codec Symphonia supports is
//! handled directly, and Opus is decoded via libopus (audiopus) since
//! Symphonia 0.5's `all` feature set does not expose its native Opus decoder.
//! No external binary is spawned.

use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use symphonia::core::audio::{AudioBufferRef, Channels};
use symphonia::core::codecs::{CodecParameters, CODEC_TYPE_NULL, CODEC_TYPE_OPUS};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::audio::resampler::StreamingResampler;

const TARGET_RATE: u32 = 16_000;
const TARGET_CHANNELS: u16 = 1;
const TARGET_BITS: u16 = 16;

/// Decode any supported audio/video-container file to the canonical Whisper /
/// Parakeet / cloud WAV: 16 kHz, mono, signed-16-bit PCM.
///
/// Streaming (Phase 3): decodes packet-by-packet into a streaming resampler and
/// s16 writer, so peak memory is bounded by one packet plus one resampler chunk
/// regardless of file length. Opus (webm/mkv/ogg) is decoded via libopus (Phase 2).
///
/// This function NEVER panics to the caller: any panic raised by Symphonia,
/// rubato, or hound on a corrupt or unsupported file is caught and converted to
/// Err("audio decode failed (unsupported or corrupt file)").
///
/// The finished WAV is installed by atomically renaming a sibling temp onto
/// `output`. If `output` already exists it is replaced — but callers must NOT
/// hold an open handle to `output` on Windows, where a rename cannot replace an
/// open file (pass a path, or a closed `TempPath`, not a live `NamedTempFile`).
pub fn normalize_to_wav(input: &Path, output: &Path) -> Result<(), String> {
    let input = input.to_path_buf();
    let output = output.to_path_buf();
    match catch_unwind(AssertUnwindSafe(move || normalize_inner(&input, &output))) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("audio decode failed (unsupported or corrupt file)".to_string()),
    }
}

fn normalize_inner(input: &Path, output: &Path) -> Result<(), String> {
    // 1. WAV passthrough: already canonical (16 kHz / mono / 16-bit int)?
    if try_passthrough_canonical_wav(input, output)? {
        return Ok(());
    }

    // 2. Open the container and pick the first non-null audio track.
    let (mut format, track_id, codec_params) = open_format(input)?;
    let is_opus = codec_params.codec == CODEC_TYPE_OPUS;

    // 3. Stream decode -> downmix -> resample -> s16 into a temp WAV, holding
    //    only one packet plus one resampler chunk at a time. The guard removes
    //    the temp file on any error or panic (catch_unwind drops it mid-unwind).
    let temp_path = temp_path_for(output);
    let guard = TempWavGuard::new(temp_path.clone());

    if is_opus {
        stream_opus(&mut *format, track_id, &codec_params, &temp_path)?;
    } else {
        stream_symphonia(&mut *format, track_id, &codec_params, &temp_path)?;
    }

    // 4. Atomically install the finished WAV.
    let finalized_temp = guard.disarm();
    std::fs::rename(&finalized_temp, output)
        .map_err(|e| format!("Failed to rename temp WAV to output: {e}"))?;
    Ok(())
}

/// If input is a WAV that is already 16 kHz / mono / 16-bit int, byte-copy it
/// to output and return Ok(true). Returns Ok(false) otherwise (including when
/// the file is not a WAV or hound cannot parse it).
fn try_passthrough_canonical_wav(input: &Path, output: &Path) -> Result<bool, String> {
    let is_wav = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("wav"))
        .unwrap_or(false);
    if !is_wav {
        return Ok(false);
    }
    let Ok(reader) = WavReader::open(input) else {
        return Ok(false);
    };
    let spec = reader.spec();
    if spec.sample_rate == TARGET_RATE
        && spec.channels == TARGET_CHANNELS
        && spec.bits_per_sample == TARGET_BITS
        && spec.sample_format == SampleFormat::Int
    {
        std::fs::copy(input, output).map_err(|e| format!("WAV passthrough copy failed: {e}"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// hound writer backed by a buffered file: the incremental s16 sink.
type WavFileWriter = WavWriter<std::io::BufWriter<std::fs::File>>;

/// Probe the container and return the format reader, the first non-null audio
/// track id, and an owned copy of its codec params (so the borrow of `format`
/// via `track` is released before packet iteration begins).
fn open_format(input: &Path) -> Result<(Box<dyn FormatReader>, u32, CodecParameters), String> {
    let file = std::fs::File::open(input).map_err(|e| format!("Failed to open audio file: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("Failed to probe audio format: {e}"))?;
    let format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "no supported audio track".to_string())?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    Ok((format, track_id, codec_params))
}

/// Sibling temp path for the streaming WAV; renamed to `output` on success.
fn temp_path_for(output: &Path) -> PathBuf {
    let dir = output.parent().unwrap_or_else(|| Path::new("."));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    dir.join(format!(".voicetypr_decode_tmp_{stamp}.wav"))
}

/// Open a 16 kHz / mono / s16 WAV writer at `path`.
fn open_writer(path: &Path) -> Result<WavFileWriter, String> {
    let spec = WavSpec {
        channels: TARGET_CHANNELS,
        sample_rate: TARGET_RATE,
        bits_per_sample: TARGET_BITS,
        sample_format: SampleFormat::Int,
    };
    WavWriter::create(path, spec).map_err(|e| format!("WAV create failed: {e}"))
}

/// Quantize normalized f32 to s16 and append to the writer.
fn write_s16_samples(writer: &mut WavFileWriter, samples: &[f32]) -> Result<(), String> {
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        writer
            .write_sample(v)
            .map_err(|e| format!("WAV write failed: {e}"))?;
    }
    Ok(())
}

/// Per-packet decode sink: resample if a resampler is attached, else the mono
/// audio is already at the target rate and is written directly.
fn feed_and_write(
    writer: &mut WavFileWriter,
    resampler: &mut Option<StreamingResampler>,
    mono: &[f32],
) -> Result<(), String> {
    if let Some(r) = resampler.as_mut() {
        let out = r.push(mono)?;
        write_s16_samples(writer, &out)?;
    } else {
        write_s16_samples(writer, mono)?;
    }
    Ok(())
}

/// Symphonia streaming path (Phase 3): decode each packet to a small per-channel
/// plane set, downmix to mono, and feed a streaming resampler -> s16 writer.
/// Only one packet and one resampler chunk are held at a time, so peak memory is
/// independent of file length. Recovers from a mid-stream decoder reset.
fn stream_symphonia(
    format: &mut dyn FormatReader,
    track_id: u32,
    codec_params: &CodecParameters,
    temp_path: &Path,
) -> Result<(), String> {
    let mut writer = open_writer(temp_path)?;
    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &Default::default())
        .map_err(|e| format!("Failed to create audio decoder: {e}"))?;

    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut layout = Channels::empty();

    // Source rate is usually known up front; if not, it is read from the first
    // decoded packet and the resampler is created then.
    let mut resampler: Option<StreamingResampler> = match codec_params.sample_rate {
        Some(r) if r != TARGET_RATE => Some(StreamingResampler::new(r as usize)?),
        _ => None,
    };
    let mut rate_known = codec_params.sample_rate.is_some();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(Error::ResetRequired) => {
                decoder = symphonia::default::get_codecs()
                    .make(codec_params, &Default::default())
                    .map_err(|e| format!("Failed to recreate audio decoder: {e}"))?;
                continue;
            }
            Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => continue,
        };

        let spec = decoded.spec();
        layout |= spec.channels;
        if planes.is_empty() {
            let nch = spec.channels.count().max(1);
            planes = vec![Vec::new(); nch];
        }
        for p in planes.iter_mut() {
            p.clear();
        }
        collect_planar(&decoded, &mut planes);
        let mono = downmix_to_mono(&planes, layout);

        if !rate_known {
            rate_known = true;
            if spec.rate != TARGET_RATE && resampler.is_none() {
                resampler = Some(StreamingResampler::new(spec.rate as usize)?);
            }
        }

        feed_and_write(&mut writer, &mut resampler, &mono)?;
    }

    if let Some(r) = resampler.take() {
        let out = r.finish()?;
        write_s16_samples(&mut writer, &out)?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalize failed: {e}"))?;
    Ok(())
}

/// Opus streaming path (Phase 2 codec + Phase 3 streaming): decode each Opus
/// packet with libopus at 48 kHz, apply pre-skip / end-trim per packet, downmix
/// to mono, and feed a 48 kHz -> 16 kHz streaming resampler -> s16 writer. Opus
/// ALWAYS decodes at 48 kHz regardless of the container's declared rate.
///
/// Channel count and pre-skip come from the OpusHead blob stored in
/// `codec_params.extra_data` (Matroska/WebM/Ogg stash it there but leave
/// `codec_params.channels`/`.delay` unset); the `codec_params` fields are used
/// directly when a demuxer does populate them. Only mono and stereo are
/// supported; surround Opus returns a clean error rather than mis-decoding.
fn stream_opus(
    format: &mut dyn FormatReader,
    track_id: u32,
    codec_params: &CodecParameters,
    temp_path: &Path,
) -> Result<(), String> {
    use audiopus::{coder::Decoder, packet::Packet, Channels as OpusCh, MutSignals, SampleRate};

    let opus_head = codec_params.extra_data.as_deref().and_then(parse_opus_head);

    let ch_count: usize = codec_params
        .channels
        .map(|c| c.count().max(1))
        .or_else(|| opus_head.map(|(ch, _)| ch as usize))
        .unwrap_or(1);
    if ch_count > 2 {
        return Err("multichannel opus not supported".to_string());
    }

    // Pre-skip (drop from front) and end-trim (drop from back) so sample counts
    // match the Opus encoder's intent. Pre-skip lives in OpusHead bytes 10-11
    // for Opus-in-MKV/WebM.
    let pre_skip: usize = codec_params
        .delay
        .map(|d| d as usize)
        .or_else(|| opus_head.map(|(_, ps)| ps as usize))
        .unwrap_or(0);
    let end_trim = codec_params.padding.unwrap_or(0) as usize;

    let stereo = ch_count >= 2;
    let opus_ch = if stereo { OpusCh::Stereo } else { OpusCh::Mono };
    let layout = if stereo {
        Channels::FRONT_LEFT | Channels::FRONT_RIGHT
    } else {
        Channels::FRONT_LEFT
    };

    let mut writer = open_writer(temp_path)?;
    let mut resampler = StreamingResampler::new(48_000)?;
    let mut front = FrontSkipper::new(pre_skip);
    let mut tail = TailTrimmer::new(end_trim);
    let mut decoder = Decoder::new(SampleRate::Hz48000, opus_ch)
        .map_err(|e| format!("opus decoder init failed: {e}"))?;

    // Max Opus frame = 120 ms @ 48 kHz = 5760 samples/channel, interleaved.
    let mut buf = vec![0f32; 5760 * ch_count];
    let mut planes: Vec<Vec<f32>> = vec![Vec::new(); ch_count];

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(_) => break,
        };
        if packet.track_id() != track_id || packet.data.is_empty() {
            continue;
        }

        let opus_packet = Packet::try_from(&packet.data[..])
            .map_err(|e| format!("opus packet wrap failed: {e}"))?;
        // decode_float takes MutSignals by value, so confine buf's mutable borrow
        // to this block; buf is readable again below for de-interleaving.
        let decoded = {
            let out = MutSignals::try_from(&mut buf[..])
                .map_err(|e| format!("opus output wrap failed: {e}"))?;
            decoder.decode_float(Some(opus_packet), out, false)
        };
        let n_per_ch = match decoded {
            Ok(n) => n,
            Err(_) => continue, // skip a bad packet rather than abort the stream
        };

        let total = n_per_ch * ch_count;
        for p in planes.iter_mut() {
            p.clear();
            p.reserve(n_per_ch);
        }
        for (i, &s) in buf[..total].iter().enumerate() {
            planes[i % ch_count].push(s);
        }
        let mono = downmix_to_mono(&planes, layout);

        // Trim pre-skip from the front and hold back end_trim from the back, at
        // 48 kHz before resampling — matches the batch trim_planes ordering.
        let skipped = front.skip(&mono);
        let mut trimmed = Vec::with_capacity(skipped.len());
        tail.push(skipped, &mut trimmed);

        if !trimmed.is_empty() {
            let out = resampler.push(&trimmed)?;
            if !out.is_empty() {
                write_s16_samples(&mut writer, &out)?;
            }
        }
    }

    // The tail trimmer still holds the last `end_trim` samples; they are the
    // trimmed tail and are intentionally discarded. Flush the resampler.
    let out = resampler.finish()?;
    if !out.is_empty() {
        write_s16_samples(&mut writer, &out)?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalize failed: {e}"))?;
    Ok(())
}

/// Parse (channel_count, pre_skip) from an OpusHead blob.
///
/// OpusHead: bytes 0-7 = "OpusHead" magic, byte 8 = version, byte 9 = output
/// channel count, bytes 10-11 = pre-skip (LE u16). Returns None if the blob is
/// too short or is not an OpusHead.
fn parse_opus_head(extra: &[u8]) -> Option<(u8, u16)> {
    if extra.len() < 12 || &extra[..8] != b"OpusHead" {
        return None;
    }
    Some((extra[9], u16::from_le_bytes([extra[10], extra[11]])))
}

macro_rules! accumulate_planar {
    ($planes:expr, $buf:expr, $conv:expr) => {{
        let audio_planes = $buf.planes();
        let chans = audio_planes.planes();
        for (idx, plane) in chans.iter().enumerate() {
            if let Some(out) = $planes.get_mut(idx) {
                out.reserve(plane.len());
                for &s in plane.iter() {
                    out.push($conv(s));
                }
            }
        }
    }};
}

/// Convert a decoded planar frame to normalized f32 and append into the matching
/// channel plane. Handles every Symphonia sample-type variant. Integer formats
/// are normalized by their full-scale range; F32/F64 are clamped to [-1, 1].
fn collect_planar(decoded: &AudioBufferRef, planes: &mut [Vec<f32>]) {
    use symphonia::core::sample::{i24, u24};
    match decoded {
        AudioBufferRef::U8(buf) => {
            accumulate_planar!(planes, buf, |s: u8| (s as f32 - 128.0) / 128.0)
        }
        AudioBufferRef::U16(buf) => {
            accumulate_planar!(planes, buf, |s: u16| (s as f32 - 32_768.0) / 32_768.0)
        }
        AudioBufferRef::U24(buf) => {
            accumulate_planar!(planes, buf, |s: u24| {
                (s.inner() as f32 - 8_388_608.0) / 8_388_608.0
            })
        }
        AudioBufferRef::U32(buf) => {
            accumulate_planar!(planes, buf, |s: u32| {
                (s as f32 - 2_147_483_648.0) / 2_147_483_648.0
            })
        }
        AudioBufferRef::S8(buf) => accumulate_planar!(planes, buf, |s: i8| s as f32 / 128.0),
        AudioBufferRef::S16(buf) => {
            accumulate_planar!(planes, buf, |s: i16| s as f32 / 32_768.0)
        }
        AudioBufferRef::S24(buf) => {
            // i24 wraps an i32 holding the 24-bit signed value; full-scale = 2^23.
            accumulate_planar!(planes, buf, |s: i24| s.inner() as f32 / 8_388_608.0)
        }
        AudioBufferRef::S32(buf) => {
            accumulate_planar!(planes, buf, |s: i32| s as f32 / 2_147_483_648.0)
        }
        AudioBufferRef::F32(buf) => accumulate_planar!(planes, buf, |s: f32| s.clamp(-1.0, 1.0)),
        AudioBufferRef::F64(buf) => {
            accumulate_planar!(planes, buf, |s: f64| s.clamp(-1.0, 1.0) as f32)
        }
    }
}

/// Layout-aware downmix to mono, matching standard -ac 1 downmix semantics: LFE is
/// dropped; remaining channels are mixed with conventional coefficients (front
/// L/R at unity, centre/surround at ~-3 dB / 0.707) and normalized by the sum of
/// used weights. For unrecognized layouts, falls back to an equal-weight average
/// of all channels (LFE excluded when identifiable).
fn downmix_to_mono(planes: &[Vec<f32>], layout: Channels) -> Vec<f32> {
    let nch = planes.len();
    if nch <= 1 {
        return planes.first().cloned().unwrap_or_default();
    }

    // Symphonia orders planes by the Channels bitset, low bit first. Walk the
    // known channel flags in bit order to build per-plane weights.
    const W_FRONT: f32 = 1.0;
    const W_ATTEN: f32 = 0.707;
    const W_LFE: f32 = 0.0;
    let known: &[(Channels, f32)] = &[
        (Channels::FRONT_LEFT, W_FRONT),
        (Channels::FRONT_RIGHT, W_FRONT),
        (Channels::FRONT_CENTRE, W_ATTEN),
        (Channels::LFE1, W_LFE),
        (Channels::REAR_LEFT, W_ATTEN),
        (Channels::REAR_RIGHT, W_ATTEN),
        (Channels::FRONT_LEFT_CENTRE, W_ATTEN),
        (Channels::FRONT_RIGHT_CENTRE, W_ATTEN),
        (Channels::REAR_CENTRE, W_ATTEN),
        (Channels::SIDE_LEFT, W_ATTEN),
        (Channels::SIDE_RIGHT, W_ATTEN),
    ];

    let mut weights: Vec<f32> = Vec::with_capacity(nch);
    for &(flag, w) in known {
        if layout.contains(flag) {
            weights.push(w);
        }
    }

    // If any channel position is unrecognized (e.g. a Top channel we don't
    // model), fall back to equal-weight average; if LFE is identifiable, give
    // it weight 0 so it doesn't pollute the mix.
    if weights.len() != nch {
        weights = vec![1.0; nch];
        if let Some(lfe_idx) = lfe_plane_index(layout) {
            if lfe_idx < nch {
                weights[lfe_idx] = 0.0;
            }
        }
    }

    let wsum: f32 = weights.iter().copied().sum();
    let len = planes.iter().map(|p| p.len()).min().unwrap_or(0);
    let mut out = Vec::with_capacity(len);
    if wsum <= 0.0 {
        // Everything was LFE / silenced — emit silence rather than divide by zero.
        out.resize(len, 0.0);
        return out;
    }
    out.extend((0..len).map(|i| {
        let acc: f32 = weights
            .iter()
            .zip(planes.iter())
            .map(|(&w, p)| w * p[i])
            .sum();
        acc / wsum
    }));
    out
}

/// Index of the LFE plane in Symphonia's plane ordering, if LFE is in the
/// layout. Symphonia orders planes low-bit-first, so LFE's index is the count of
/// channel bits set below it.
fn lfe_plane_index(layout: Channels) -> Option<usize> {
    if !layout.contains(Channels::LFE1) {
        return None;
    }
    let lower_bits = layout.bits() & (Channels::LFE1.bits() - 1);
    Some(lower_bits.count_ones() as usize)
}

/// Drop the first N samples from a stream, in order. Used for Opus pre-skip.
struct FrontSkipper {
    remaining: usize,
}

impl FrontSkipper {
    fn new(remaining: usize) -> Self {
        Self { remaining }
    }

    /// Return the slice with the still-owed front samples removed.
    fn skip<'a>(&mut self, samples: &'a [f32]) -> &'a [f32] {
        if self.remaining == 0 {
            return samples;
        }
        let take = self.remaining.min(samples.len());
        self.remaining -= take;
        &samples[take..]
    }
}

/// Hold back the last N samples of a stream so they can be dropped at EOF.
/// Used for Opus end-trim: emits each sample only once a newer one displaces it,
/// and the final N (the trim tail) are discarded when streaming ends.
struct TailTrimmer {
    buf: VecDeque<f32>,
    cap: usize,
}

impl TailTrimmer {
    fn new(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap.max(1)),
            cap,
        }
    }

    /// Buffer `samples` through the trim window; emit any frames aged out of it.
    fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        if self.cap == 0 {
            out.extend_from_slice(samples);
            return;
        }
        for &s in samples {
            self.buf.push_back(s);
            if self.buf.len() > self.cap {
                out.push(self.buf.pop_front().unwrap());
            }
        }
    }
}

/// RAII guard for the streaming temp WAV: removes the file on drop unless
/// `disarm` consumed it, so a decode error or a caught panic never leaves a
/// half-written file behind.
struct TempWavGuard {
    path: PathBuf,
    armed: bool,
}

impl TempWavGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    /// Mark the file as successfully finalized and return its path for rename.
    fn disarm(mut self) -> PathBuf {
        self.armed = false;
        self.path.clone()
    }
}

impl Drop for TempWavGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Async wrapper: runs the CPU-bound normalize on a blocking thread so it never
/// stalls the tokio runtime. Same `Result<(), String>` shape as the old
/// external-binary normalize, so callers just swap the call and drop the `app` arg.
pub async fn normalize_to_wav_async(
    input: std::path::PathBuf,
    output: std::path::PathBuf,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || normalize_to_wav(&input, &output))
        .await
        .map_err(|e| format!("audio decode task join failed: {e}"))?
}
