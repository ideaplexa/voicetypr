use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub(crate) struct ClipboardArgs {
    #[command(subcommand)]
    command: ClipboardCommand,
}

#[derive(Subcommand, Debug)]
enum ClipboardCommand {
    /// Exercise native clipboard ownership and restoration; no contents are printed.
    Verify {
        #[arg(long, default_value = "slow-reader", value_parser = ["slow-reader"])]
        fixture: String,
        #[arg(long)]
        json: bool,
    },
}

impl ClipboardArgs {
    pub(crate) fn wants_json(&self) -> bool {
        matches!(self.command, ClipboardCommand::Verify { json: true, .. })
    }
}

pub(crate) fn run(args: &ClipboardArgs) -> Result<(), &'static str> {
    #[cfg(target_os = "macos")]
    let result = super::verify_slow_reader()?;
    #[cfg(not(target_os = "macos"))]
    let result = serde_json::json!({
        "platform": std::env::consts::OS, "fixture": "slow-reader", "outcome": "unsupported",
        "passed": false, "target_insertion_verified": false
    });
    if args.wants_json() {
        println!("{result}");
    } else {
        println!("Clipboard verification: {}", result["outcome"].as_str().unwrap_or("failed"));
    }
    if result["outcome"] == "unsupported" {
        return Err("Clipboard receipt verification is unsupported on this platform");
    }
    if result["passed"] != true {
        return Err("Clipboard transaction verification failed");
    }
    Ok(())
}
