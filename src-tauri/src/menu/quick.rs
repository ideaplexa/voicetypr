//! Content-free projection of the native tray model. IDs go to its shared dispatcher.
use super::model::{Item, Snapshot};
use serde::Serialize;
use tauri::AppHandle;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OptionItem {
    id: String,
    label: String,
    checked: bool,
    disabled: bool,
}
#[derive(Serialize)]
pub struct Choices {
    current: String,
    options: Vec<OptionItem>,
}
#[derive(Serialize)]
pub struct Group {
    label: String,
    options: Vec<OptionItem>,
}
#[derive(Serialize)]
pub struct Engines {
    current: String,
    groups: Vec<Group>,
}
#[derive(Serialize)]
pub struct ShortcutCaps {
    mode: &'static str,
    keys: Vec<String>,
}
#[derive(Serialize)]
pub struct QuickOptions {
    polish: Choices,
    engine: Engines,
    mic: Choices,
    language: Choices,
    shortcut_caps: ShortcutCaps,
    mic_ok: bool,
    polish_context: Option<crate::pill::context::EffectivePolish>,
}
fn children<'a>(items: &'a [Item], name: &str) -> &'a [Item] {
    items
        .iter()
        .find_map(|item| match item {
            Item::Submenu { id, items, .. } if id == name => Some(items.as_slice()),
            _ => None,
        })
        .unwrap_or_default()
}
fn option(item: &Item) -> Option<OptionItem> {
    match item {
        Item::Action {
            id,
            label,
            enabled,
            checked: Some(checked),
            ..
        } => Some(OptionItem {
            id: id.clone(),
            label: label.clone(),
            checked: *checked,
            disabled: !enabled,
        }),
        _ => None,
    }
}
fn shortcut_keys(raw: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut word = String::new();
    for character in raw.chars() {
        if "⌘⌥⌃⇧".contains(character) {
            let prefix = word.trim();
            if matches!(prefix, "Left" | "Right") {
                keys.push(format!("{prefix} {character}"));
            } else {
                if !prefix.is_empty() {
                    keys.push(prefix.into());
                }
                keys.push(character.to_string());
            }
            word.clear();
        } else if character == '+' {
            if !word.trim().is_empty() {
                keys.push(word.trim().into());
            }
            word.clear();
        } else {
            word.push(character);
        }
    }
    if !word.trim().is_empty() {
        keys.push(word.trim().into());
    }
    keys
}
fn project(s: &Snapshot) -> QuickOptions {
    let items = super::model::build(s);
    let choices = |name: &str, fallback: &str| {
        let options = children(&items, name)
            .iter()
            .filter_map(option)
            .collect::<Vec<_>>();
        let current = options
            .iter()
            .find(|o| o.checked)
            .map(|o| o.label.clone())
            .unwrap_or_else(|| fallback.into());
        Choices { current, options }
    };
    let mut groups: Vec<Group> = Vec::new();
    for item in children(&items, "models") {
        if let Some(option) = option(item) {
            if let Some(group) = groups.last_mut() {
                group.options.push(option);
            }
        } else if let Item::Action {
            id,
            label,
            enabled: false,
            ..
        } = item
        {
            if id.starts_with("header_") {
                groups.push(Group {
                    label: label.clone(),
                    options: vec![],
                });
            }
        }
    }
    let caps = shortcut_keys(&s.shortcut);
    let mut mic = choices("microphones", "System default");
    mic.current = crate::pill::context::mic_display_name(&mic.current);
    let mic_ok = !s.devices.is_empty();
    let language_options = super::model::language_options(s)
        .iter()
        .filter_map(option)
        .collect::<Vec<_>>();
    let language_current = language_options
        .iter()
        .find(|o| o.checked)
        .map(|o| o.label.clone())
        .unwrap_or_else(|| s.language.clone());
    QuickOptions {
        polish: choices("polish", &s.polish),
        engine: Engines {
            current: s.engine.clone(),
            groups,
        },
        mic,
        language: Choices {
            current: language_current,
            options: language_options,
        },
        shortcut_caps: ShortcutCaps {
            mode: if s.hold { "hold" } else { "toggle" },
            keys: caps,
        },
        mic_ok,
        polish_context: None,
    }
}
#[tauri::command]
pub async fn island_quick_options(app: AppHandle) -> Result<QuickOptions, String> {
    let snapshot = super::tray::snapshot(&app, false)
        .await
        .map_err(|_| "Quick settings unavailable")?;
    let mut options = project(&snapshot);
    options.polish_context = crate::pill::context::effective_polish(&app);
    if let Ok(primary) = crate::commands::shortcuts::get_effective_primary_shortcut(app) {
        options.shortcut_caps.mode = primary.mode;
    }
    Ok(options)
}
fn valid_kind(kind: &str, id: &str) -> bool {
    let prefix = match kind {
        "polish" => "style_",
        "engine" => "model_",
        "mic" => "microphone_",
        "language" => "language_",
        _ => return false,
    };
    id.strip_prefix(prefix).is_some_and(|v| !v.is_empty())
}
#[tauri::command]
pub async fn island_quick_set(app: AppHandle, kind: String, id: String) -> Result<(), String> {
    if !valid_kind(&kind, &id) {
        return Err("Invalid quick setting".into());
    }
    let options = island_quick_options(app.clone()).await?;
    set_with_handler(&options, &kind, id, |id| async move {
        super::actions::run_from(app, &id, "island").await
    })
    .await
}
/// Validation and dispatch are tested without hardware or an activating app window.
async fn set_with_handler<F, Fut>(
    options: &QuickOptions,
    kind: &str,
    id: String,
    handler: F,
) -> Result<(), String>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    if !valid_kind(kind, &id) {
        return Err("Invalid quick setting".into());
    }
    let available = match kind {
        "polish" => options
            .polish
            .options
            .iter()
            .any(|o| o.id == id && !o.disabled),
        "engine" => options
            .engine
            .groups
            .iter()
            .flat_map(|g| &g.options)
            .any(|o| o.id == id && !o.disabled),
        "mic" => options
            .mic
            .options
            .iter()
            .any(|o| o.id == id && !o.disabled),
        "language" => options
            .language
            .options
            .iter()
            .any(|o| o.id == id && !o.disabled),
        _ => false,
    };
    if !available {
        return Err("Quick setting unavailable".into());
    }
    handler(id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_are_exact_tray_check_items() {
        let s = Snapshot {
            polish: "Clean".into(),
            engines: vec![("tiny".into(), "Whisper Tiny".into(), "local".into(), true)],
            devices: vec!["Shure MV7".into()],
            languages: vec![("en".into(), "English".into())],
            language: "en".into(),
            ..Default::default()
        };
        let quick = project(&s);
        let tray = super::super::model::build(&s);
        for (name, actual) in [
            ("polish", quick.polish.options),
            ("microphones", quick.mic.options),
            ("languages", quick.language.options),
        ] {
            assert_eq!(
                actual,
                children(&tray, name)
                    .iter()
                    .filter_map(option)
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(
            quick
                .engine
                .groups
                .iter()
                .flat_map(|g| g.options.clone())
                .collect::<Vec<_>>(),
            children(&tray, "models")
                .iter()
                .filter_map(option)
                .collect::<Vec<_>>()
        );
        assert_eq!(quick.engine.groups[0].label, "On this Mac");
    }
    #[tokio::test]
    async fn each_set_dispatches_once_and_propagates_shared_handler_errors() {
        let options = project(&Snapshot {
            engines: vec![("tiny".into(), "Tiny".into(), "local".into(), true)],
            languages: vec![("en".into(), "English".into())],
            ..Default::default()
        });
        for (kind, id) in [
            ("polish", "style_Clean"),
            ("engine", "model_tiny"),
            ("mic", "microphone_default"),
            ("language", "language_en"),
        ] {
            let mut calls = Vec::new();
            let result = set_with_handler(&options, kind, id.into(), |value| {
                calls.push(value);
                async { Err("Shared handler failed".into()) }
            })
            .await;
            assert_eq!(calls, [id]);
            assert_eq!(result, Err("Shared handler failed".into()));
        }
        let result = set_with_handler(&options, "engine", "nav_settings".into(), |_| async {
            panic!("Invalid IDs must never dispatch")
        })
        .await;
        assert!(result.is_err());
    }
    #[test]
    fn full_language_model_retains_selected_language_beyond_tray_shortlist() {
        let s = Snapshot {
            language: "9".into(),
            languages: (0..12)
                .map(|n| (n.to_string(), format!("Language {n}")))
                .collect(),
            ..Default::default()
        };
        let quick = project(&s);
        assert_eq!(quick.language.options.len(), 12);
        assert_eq!(quick.language.current, "Language 9");
        assert!(quick.language.options[9].checked);
        let tray = super::super::model::build(&s);
        assert_eq!(
            quick.language.options[..8],
            children(&tray, "languages")
                .iter()
                .filter_map(option)
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn real_shortcut_caps_and_mode_are_platform_specific() {
        for (shortcut, expected) in [
            ("⌥Space", vec!["⌥", "Space"]),
            ("Right ⌥", vec!["Right ⌥"]),
            ("⌘⇧Space", vec!["⌘", "⇧", "Space"]),
            ("Ctrl+Alt+Space", vec!["Ctrl", "Alt", "Space"]),
        ] {
            let quick = project(&Snapshot {
                shortcut: shortcut.into(),
                hold: true,
                ..Default::default()
            });
            assert_eq!(quick.shortcut_caps.keys, expected);
            assert_eq!(quick.shortcut_caps.mode, "hold");
        }
    }
    #[test]
    fn only_shared_setting_ids_are_dispatched() {
        for (kind, id) in [
            ("polish", "style_Clean"),
            ("engine", "model_tiny"),
            ("mic", "microphone_default"),
            ("language", "language_en"),
        ] {
            assert!(valid_kind(kind, id));
            for other in ["polish", "engine", "mic", "language"]
                .into_iter()
                .filter(|k| *k != kind)
            {
                assert!(!valid_kind(other, id));
            }
        }
        assert!(!valid_kind("engine", "nav_settings"));
        assert!(!valid_kind("language", "language_"));
    }
}
