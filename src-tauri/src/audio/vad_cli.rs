//! Standalone measurement adapter. No desktop, microphone, settings or network.
use std::{fs::File, io::BufReader, path::PathBuf};

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct VadArgs {
    #[command(subcommand)]
    command: VadCommand,
}

#[derive(Subcommand, Debug)]
enum VadCommand {
    /// Analyze a PCM16 WAV without gating, filtering or writing audio.
    Analyze(AnalyzeArgs),
}

#[derive(Args)]
struct AnalyzeArgs {
    #[arg(long)]
    file: PathBuf,
    /// Required acknowledgement: results are advisory, never an engine gate.
    #[arg(long, required = true)]
    shadow: bool,
    #[arg(long)]
    json: bool,
    /// Repeat in this process to compare first and subsequent runtime costs.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=20))]
    runs: u32,
}
impl std::fmt::Debug for AnalyzeArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalyzeArgs")
            .field("shadow", &self.shadow)
            .finish_non_exhaustive()
    }
}

impl VadArgs {
    pub fn wants_json(&self) -> bool {
        match &self.command {
            VadCommand::Analyze(args) => args.json,
        }
    }
}

pub fn run(args: &VadArgs) -> Result<(), &'static str> {
    let VadCommand::Analyze(args) = &args.command;
    let mut runs = Vec::new();
    for _ in 0..args.runs {
        let file = File::open(&args.file).map_err(|_| "Cannot open VAD input")?;
        runs.push(super::silero_vad::analyze(
            BufReader::new(file),
            super::silero_vad::BUDGET,
        ));
    }
    let output = serde_json::json!({
        "mode": "shadow",
        "model": "ggml-silero-v6.2.0",
        "model_sha256": super::silero_vad::MODEL_SHA256,
        "budget_ms": super::silero_vad::BUDGET.as_millis() as u64,
        "runs": runs,
    });
    // The same metadata is useful in human and JSON mode; never echo input paths.
    println!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(flatten)]
        args: VadArgs,
    }
    #[test]
    fn measurement_requires_shadow_and_redacts_input_path() {
        assert!(
            TestCli::try_parse_from(["vad", "analyze", "--file", "/private/capture.wav"]).is_err()
        );
        let cli = TestCli::try_parse_from([
            "vad",
            "analyze",
            "--file",
            "/private/capture.wav",
            "--shadow",
            "--json",
        ])
        .unwrap();
        assert!(cli.args.wants_json());
        assert!(!format!("{cli:?}").contains("/private/capture.wav"));
    }
}
