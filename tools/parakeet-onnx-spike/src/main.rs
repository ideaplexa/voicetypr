use parakeet_rs::{ExecutionConfig, ExecutionProvider, ParakeetTDT, TimestampMode, Transcriber};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const JFK_REF: &str = "And so my fellow Americans, ask not what your country can do for you, ask what you can do for your country.";
#[derive(Deserialize)]
struct Clip {
    file: String,
    lang: String,
    #[serde(rename = "ref")]
    reference: String,
}
struct Args {
    model: PathBuf,
    wav: Option<PathBuf>,
    bench: Option<PathBuf>,
    lang: String,
    threads: Option<usize>,
}
fn usage() -> &'static str {
    "usage: parakeet-onnx-spike --model <dir> (--wav <file> | --bench <clips dir>) [--lang en] [--threads N]"
}
fn parse_args() -> Result<Args, Box<dyn Error>> {
    let (mut model, mut wav, mut bench, mut threads) = (None, None, None, None);
    let mut lang = "en".to_owned();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!("{}", usage());
            std::process::exit(0);
        }
        if !["--model", "--wav", "--bench", "--lang", "--threads"].contains(&arg.as_str()) {
            return Err(format!("unknown argument: {arg}; {}", usage()).into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        match arg.as_str() {
            "--model" => model = Some(PathBuf::from(value)),
            "--wav" => wav = Some(PathBuf::from(value)),
            "--bench" => bench = Some(PathBuf::from(value)),
            "--lang" => lang = value,
            "--threads" => {
                let n: usize = value.parse()?;
                if n == 0 {
                    return Err("--threads must be positive".into());
                }
                threads = Some(n);
            }
            _ => unreachable!(),
        }
    }
    if wav.is_some() == bench.is_some() {
        return Err(usage().into());
    }
    Ok(Args {
        model: model.ok_or("--model is required")?,
        wav,
        bench,
        lang,
        threads,
    })
}

// Decode interleaved PCM/float WAV, downmix, and interpolate to 16 kHz mono.
fn read_wav(path: &Path) -> Result<(Vec<f32>, f64), Box<dyn Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 || spec.bits_per_sample == 0 {
        return Err("invalid WAV format".into());
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1_u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };
    let channels = spec.channels as usize;
    if samples.len() % channels != 0 {
        return Err("incomplete WAV frame".into());
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return Err("empty WAV".into());
    }
    let duration = frames as f64 / spec.sample_rate as f64;
    let mono: Vec<f32> = samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    if spec.sample_rate == 16_000 {
        return Ok((mono, duration));
    }
    let out_len =
        ((frames as u64 * 16_000 + spec.sample_rate as u64 / 2) / spec.sample_rate as u64) as usize;
    let ratio = spec.sample_rate as f64 / 16_000.0;
    let output = (0..out_len)
        .map(|i| {
            let source = i as f64 * ratio;
            let left = (source.floor() as usize).min(frames - 1);
            let right = (left + 1).min(frames - 1);
            mono[left] + (mono[right] - mono[left]) * (source - left as f64) as f32
        })
        .collect();
    Ok((output, duration))
}
fn segments(samples: &[f32]) -> Vec<(usize, usize)> {
    const RATE: usize = 16_000;
    if samples.len() <= 30 * RATE {
        return vec![(0, samples.len())];
    }
    // Split long input at sustained quiet gaps to preserve whole utterances.
    let hop = RATE / 10;
    let quiet: Vec<bool> = samples
        .chunks(hop)
        .map(|window| window.iter().all(|sample| sample.abs() < 0.003))
        .collect();
    let mut boundaries = vec![0];
    let mut start = None;
    for (i, is_quiet) in quiet
        .iter()
        .copied()
        .chain(std::iter::once(false))
        .enumerate()
    {
        if is_quiet && start.is_none() {
            start = Some(i);
        }
        if !is_quiet {
            if let Some(first) = start.take() {
                if i - first >= 8 {
                    let boundary = ((first + i) * hop / 2).min(samples.len());
                    if boundary - boundaries[boundaries.len() - 1] >= 2 * RATE
                        && samples.len() - boundary >= 2 * RATE
                    {
                        boundaries.push(boundary);
                    }
                }
            }
        }
    }
    boundaries.push(samples.len());
    let mut result = Vec::new();
    for pair in boundaries.windows(2) {
        let mut begin = pair[0];
        while pair[1] - begin > 30 * RATE {
            result.push((begin, begin + 15 * RATE));
            begin += 15 * RATE;
        }
        result.push((begin, pair[1]));
    }
    result
}
fn transcribe(model: &mut ParakeetTDT, path: &Path, load_ms: f64) -> Result<Value, Box<dyn Error>> {
    let (samples, seconds) = read_wav(path)?;
    let start = Instant::now();
    let mut text = Vec::new();
    let mut words = Vec::new();
    for (begin, end) in segments(&samples) {
        let result = model.transcribe_samples(
            samples[begin..end].to_vec(),
            16_000,
            1,
            Some(TimestampMode::Words),
        )?;
        if !result.text.is_empty() {
            text.push(result.text);
        }
        let offset = begin as f32 / 16_000.0;
        words.extend(result.tokens.iter().map(|token| {
            json!({"text": token.text, "start": token.start + offset, "end": token.end + offset})
        }));
    }
    let transcribe_ms = start.elapsed().as_secs_f64() * 1000.0;
    let audio_ms = seconds * 1000.0;
    Ok(
        json!({"text": text.join(" "), "words": words, "model_load_ms": load_ms, "transcribe_ms": transcribe_ms, "audio_ms": audio_ms, "rtf": transcribe_ms / audio_ms}),
    )
}
fn normalized_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}
fn wer(reference: &str, hypothesis: &str) -> f64 {
    let reference = normalized_words(reference);
    let hypothesis = normalized_words(hypothesis);
    if reference.is_empty() {
        return if hypothesis.is_empty() { 0.0 } else { 1.0 };
    }
    let mut previous: Vec<usize> = (0..=hypothesis.len()).collect();
    for (i, expected) in reference.iter().enumerate() {
        let mut current = vec![i + 1; hypothesis.len() + 1];
        for (j, actual) in hypothesis.iter().enumerate() {
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + usize::from(expected != actual));
        }
        previous = current;
    }
    previous[hypothesis.len()] as f64 / reference.len() as f64
}
#[cfg(unix)]
fn peak_rss_bytes() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // getrusage writes the whole struct when it succeeds.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    let rss = unsafe { usage.assume_init().ru_maxrss } as u64;
    #[cfg(target_os = "linux")]
    {
        Some(rss * 1024)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Some(rss)
    }
}
#[cfg(not(unix))]
fn peak_rss_bytes() -> Option<u64> {
    None
}
fn bench(model: &mut ParakeetTDT, dir: &Path, load_ms: f64) -> Result<(), Box<dyn Error>> {
    let mut clips: Vec<Clip> = serde_json::from_slice(&fs::read(dir.join("manifest.json"))?)?;
    if dir.join("jfk.wav").exists() && !clips.iter().any(|clip| clip.file == "jfk.wav") {
        clips.insert(
            0,
            Clip {
                file: "jfk.wav".into(),
                lang: "en".into(),
                reference: JFK_REF.into(),
            },
        );
    }
    if clips.is_empty() {
        return Err("manifest contains no clips".into());
    }
    let mut rows = Vec::new();
    println!(
        "{:<17} {:<4} {:>7} {:>13} {:>9}",
        "clip", "lang", "WER %", "transcribe ms", "RTF"
    );
    for clip in clips {
        let result = transcribe(model, &dir.join(&clip.file), load_ms)?;
        let clip_wer = wer(
            &clip.reference,
            result["text"].as_str().ok_or("missing text")?,
        );
        let ms = result["transcribe_ms"].as_f64().ok_or("missing duration")?;
        let rtf = result["rtf"].as_f64().ok_or("missing rtf")?;
        println!(
            "{:<17} {:<4} {:>7.1} {:>13.0} {:>9.3}",
            clip.file,
            clip.lang,
            clip_wer * 100.0,
            ms,
            rtf
        );
        rows.push(json!({"file": clip.file, "lang": clip.lang, "wer": clip_wer, "transcribe_ms": ms, "audio_ms": result["audio_ms"], "rtf": rtf}));
    }
    let count = rows.len() as f64;
    let average = |key: &str| {
        rows.iter()
            .map(|row| row[key].as_f64().unwrap_or(0.0))
            .sum::<f64>()
            / count
    };
    let total_ms: f64 = rows
        .iter()
        .map(|row| row["transcribe_ms"].as_f64().unwrap_or(0.0))
        .sum();
    let total_audio_ms: f64 = rows
        .iter()
        .map(|row| row["audio_ms"].as_f64().unwrap_or(0.0))
        .sum();
    println!(
        "{}",
        json!({"clips": rows, "average_wer": average("wer"), "average_transcribe_ms": average("transcribe_ms"), "average_rtf": average("rtf"), "aggregate_rtf": total_ms / total_audio_ms, "model_load_ms": load_ms, "peak_rss_bytes": peak_rss_bytes()})
    );
    Ok(())
}
fn run() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let config = ExecutionConfig::new()
        .with_execution_provider(ExecutionProvider::Cpu)
        .with_intra_threads(args.threads.unwrap_or(4));
    let start = Instant::now();
    let mut model = ParakeetTDT::from_pretrained(&args.model, Some(config))?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;
    if let Some(dir) = args.bench {
        bench(&mut model, &dir, load_ms)
    } else {
        // TDT v3 auto-detects language; parakeet-rs has no language-hint input.
        let _lang = args.lang;
        println!("{}", transcribe(&mut model, &args.wav.unwrap(), load_ms)?);
        Ok(())
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wer_normalizes_case_and_punctuation() {
        assert_eq!(wer("Ask not, what?", "ask NOT what"), 0.0);
        assert_eq!(wer("eins zwei drei", "eins drei"), 1.0 / 3.0);
        assert_eq!(wer("für München", "für munchen"), 0.5);
    }

    #[test]
    fn long_audio_splits_at_quiet_gap() {
        let mut audio = vec![0.1; 40 * 16_000];
        audio[18 * 16_000..20 * 16_000].fill(0.0);
        let ranges = segments(&audio);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].0, 0);
        assert_eq!(ranges[0].1, ranges[1].0);
        assert_eq!(ranges[1].1, audio.len());
    }
}
