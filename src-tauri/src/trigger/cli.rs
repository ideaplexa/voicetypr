//! Headless, metadata-only hotkey diagnostics. Never creates taps or registers keys.
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;

#[derive(Args, Debug)]
pub(crate) struct HotkeyArgs {
    #[command(subcommand)]
    command: HotkeyCommand,
}
#[derive(Subcommand, Debug)]
enum HotkeyCommand {
    /// Read only the public macOS Secure Input flag, without owner identification.
    Diagnose {
        #[arg(long)]
        json: bool,
    },
    /// Exercise the production dispatcher with injected mode changes, not OS input.
    Verify {
        #[arg(long, value_enum)]
        fixture: Fixture,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Fixture {
    SecureInput,
}
impl HotkeyArgs {
    pub(crate) fn wants_json(&self) -> bool {
        match self.command {
            HotkeyCommand::Diagnose { json } | HotkeyCommand::Verify { json, .. } => json,
        }
    }
}

pub(crate) fn run(args: &HotkeyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let value = match args.command {
        HotkeyCommand::Diagnose { .. } => json!({
            "platform": std::env::consts::OS,
            "secure_input": if cfg!(target_os = "macos") { Some(keytrigger::backend::secure_input_enabled()) } else { None },
            "registration": "not_probed",
            "fallback": if cfg!(target_os = "macos") { "command_control_option_plus_space_letter_digit_f1_f20_optional_shift" } else { "not_applicable" },
            "unsupported": "Use tray or pointer start and island stop/cancel",
            "windows_limit": "Normal hooks cannot cross UIPI/elevated-target or secure-desktop boundaries",
        }),
        HotkeyCommand::Verify {
            fixture: Fixture::SecureInput,
            ..
        } => {
            let events = verify_fixture()?;
            json!({"fixture":"secure-input","passed":true,"events":events,"hardware_verified":false})
        }
    };
    if args.wants_json() {
        println!("{}", serde_json::to_string(&value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

fn verify_fixture() -> Result<usize, Box<dyn std::error::Error>> {
    use keytrigger::{
        engine::MockSource, Control, KeyPhase, KeySpec, Modifier, Msg, NamedKey, RawKeyEvent, Side,
        Trigger, TriggerEngine, TriggerEvent,
    };
    use std::{
        sync::{mpsc, Arc, Mutex},
        time::Duration,
    };
    let raw = |key, side, down| {
        Msg::Raw(RawKeyEvent {
            key: KeySpec::Named(key),
            side,
            down,
            is_repeat: false,
        })
    };
    let press = TriggerEvent {
        id: "primary".into(),
        phase: KeyPhase::Pressed,
    };
    let release = TriggerEvent {
        phase: KeyPhase::Released,
        ..press.clone()
    };
    let script = vec![
        raw(NamedKey::AltLeft, Some(Side::Left), true),
        raw(NamedKey::Space, None, true),
        // The tap never sends the physical key-up. Entry must release the hold.
        Msg::Control(Control::SecureInput(true)),
        raw(NamedKey::AltLeft, Some(Side::Left), true),
        raw(NamedKey::Space, None, true),
        Msg::Fallback(press.clone(), 1),
        Msg::Fallback(press.clone(), 1), // Repeat must not toggle or start twice.
        Msg::Control(Control::ReconfigureFallback), // Same-id chord replacement.
        Msg::Fallback(press.clone(), 1), // Retired chord must not acquire its new epoch.
        Msg::Fallback(press.clone(), 2),
        Msg::Control(Control::SecureInput(false)), // Release fallback hold, too.
        Msg::Fallback(release.clone(), 1),         // Retired registration.
        Msg::Fallback(press.clone(), 1),
        raw(NamedKey::AltLeft, Some(Side::Left), true),
        raw(NamedKey::Space, None, true),
        raw(NamedKey::Space, None, false),
        Msg::Control(Control::SecureInput(true)),
        Msg::Fallback(press.clone(), 1), // Stale even after the next secure entry.
        Msg::Control(Control::SecureInput(false)),
    ]
    .into_iter()
    .map(|msg| (msg, Duration::ZERO))
    .collect();
    let engine = TriggerEngine::new();
    engine.set_bindings(vec![(
        "primary".into(),
        Trigger::ComboExact {
            mods: keytrigger::ModSet::empty().with(Modifier::Alt),
            key: KeySpec::Named(NamedKey::Space),
        },
    )]);
    let events = Arc::new(Mutex::new(vec![]));
    let sink = Arc::clone(&events);
    let (tx, rx) = mpsc::channel();
    engine.start_with_source_and_status(
        Arc::new(MockSource::new(script)),
        move |event| sink.lock().unwrap().push(event),
        move |state| {
            let _ = tx.send(state);
        },
    )?;
    let result = (|| {
        for (epoch, enabled) in [(1, true), (2, true), (3, false), (4, true), (5, false)] {
            let state = rx.recv_timeout(Duration::from_secs(3))?;
            if state.epoch != epoch || state.enabled != enabled {
                return Err("Secure Input transition ordering failed".into());
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })();
    engine.stop();
    result?;
    let events = events.lock().unwrap();
    let expected = vec![
        press.clone(),
        release.clone(),
        press.clone(),
        release.clone(),
        press.clone(),
        release.clone(),
        press,
        release,
    ];
    if *events != expected {
        return Err("Secure Input hold reset or exclusive routing failed".into());
    }
    Ok(events.len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn secure_input_fixture_resets_both_hold_sources_and_recovers_once() {
        assert_eq!(super::verify_fixture().unwrap(), 8);
    }
}
