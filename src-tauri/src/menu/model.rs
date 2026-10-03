//! Pure native-menu presentation. Transcript previews exist only in menu labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Action {
        id: String,
        label: String,
        enabled: bool,
        checked: Option<bool>,
        accelerator: Option<String>,
    },
    Submenu {
        id: String,
        label: String,
        items: Vec<Item>,
    },
    Separator,
}
#[derive(Default)]
pub struct Snapshot {
    pub windows: bool,
    pub engine: String,
    pub recording: Option<u64>,
    pub blocked: Option<String>,
    pub kept: Option<(String, Option<String>)>,
    pub kept_busy: bool,
    pub shortcut: String,
    pub polish: String,
    pub engines: Vec<(String, String, String, bool)>,
    pub mic: Option<String>,
    pub devices: Vec<String>,
    pub language: String,
    pub languages: Vec<(String, String)>,
    pub hold: bool,
    pub preview: bool,
    pub recent: Vec<(String, String)>,
    pub updates: bool,
}
fn action(id: &str, label: impl Into<String>) -> Item {
    Item::Action {
        id: id.into(),
        label: label.into(),
        enabled: true,
        checked: None,
        accelerator: None,
    }
}
fn check(id: &str, label: impl Into<String>, checked: bool) -> Item {
    let mut item = action(id, label);
    if let Item::Action { checked: value, .. } = &mut item {
        *value = Some(checked);
    }
    item
}
fn disabled(id: &str, label: impl Into<String>) -> Item {
    let mut item = action(id, label);
    if let Item::Action { enabled, .. } = &mut item {
        *enabled = false;
    }
    item
}
fn submenu(id: &str, label: impl Into<String>, items: Vec<Item>) -> Item {
    Item::Submenu {
        id: id.into(),
        label: label.into(),
        items,
    }
}
/// The tray shows eight common choices; the island scrolls the same complete model.
pub(super) fn language_options(s: &Snapshot) -> Vec<Item> {
    s.languages
        .iter()
        .map(|(code, name)| check(&format!("language_{code}"), name, code == &s.language))
        .collect()
}
pub fn recording_label(seconds: u64) -> String {
    format!("● Recording · {}:{:02}", seconds / 60, seconds % 60)
}
pub fn build(s: &Snapshot) -> Vec<Item> {
    let wording = |mac: &str, win: &str| {
        if s.windows {
            win.to_owned()
        } else {
            mac.to_owned()
        }
    };
    let location = if s.windows {
        "on this PC"
    } else {
        "on this Mac"
    };
    let status = if let Some(seconds) = s.recording {
        recording_label(seconds)
    } else if let Some(problem) = &s.blocked {
        format!("● {problem}")
    } else {
        format!("● Ready · {} · {location}", s.engine)
    };
    let mut items = vec![disabled("status", status)];
    if s.recording.is_none() && s.blocked.is_some() {
        items.push(action("fix", "Fix…"));
    }
    if let Some((id, alternative)) = &s.kept {
        let label = wording("Retry Last Dictation", "Retry last dictation");
        let mut retry = action(
            &format!("retry_{id}"),
            alternative
                .as_ref()
                .map(|e| format!("{label} with {e}"))
                .unwrap_or(label),
        );
        if let Item::Action { enabled, .. } = &mut retry {
            *enabled = alternative.is_some() && !s.kept_busy;
        }
        items.push(retry);
        items.push(action(
            &format!("discard_{id}"),
            wording("Discard Last Recording", "Discard last recording"),
        ));
    }
    let toggle = if s.recording.is_some() {
        wording("Stop Dictation", "Stop dictation")
    } else {
        wording("Start Dictation", "Start dictation")
    };
    // Tauri accelerators register actions; label text avoids a second recording shortcut.
    items.push(action("dictate", format!("{toggle}   {}", s.shortcut)));
    items.push(Item::Separator);
    let mut styles = ["Off", "Clean", "Writing", "Notes", "Message", "Code"]
        .into_iter()
        .map(|style| check(&format!("style_{style}"), style, s.polish == style))
        .collect::<Vec<_>>();
    styles.extend([
        Item::Separator,
        action("nav_polish", wording("Per-app Styles…", "Per-app styles…")),
    ]);
    items.push(submenu("polish", format!("Polish: {}", s.polish), styles));
    let mut engines = Vec::new();
    for (group, header) in [
        ("local", wording("On this Mac", "On this PC")),
        ("cloud", "Cloud".into()),
        ("network", "Network".into()),
    ] {
        engines.push(disabled(&format!("header_{group}"), header));
        engines.extend(
            s.engines
                .iter()
                .filter(|(_, _, kind, _)| kind == group)
                .map(|(id, name, _, selected)| check(&format!("model_{id}"), name, *selected)),
        );
    }
    engines.extend([
        Item::Separator,
        action(
            "nav_models",
            wording("Download More Models…", "Download more models…"),
        ),
    ]);
    items.push(submenu("models", format!("Engine: {}", s.engine), engines));
    let mut mics = vec![check(
        "microphone_default",
        wording("System Default", "System default"),
        s.mic.is_none(),
    )];
    mics.extend(s.devices.iter().map(|name| {
        check(
            &format!("microphone_{name}"),
            name,
            s.mic.as_ref() == Some(name),
        )
    }));
    let mic = s
        .mic
        .as_deref()
        .map(crate::pill::context::mic_display_name)
        .unwrap_or_else(|| wording("System Default", "System default"));
    items.push(submenu("microphones", format!("Microphone: {mic}"), mics));
    let current_language = s
        .languages
        .iter()
        .find(|(code, _)| code == &s.language)
        .map(|(_, name)| name.as_str())
        .unwrap_or(&s.language);
    let mut languages = language_options(s).into_iter().take(8).collect::<Vec<_>>();
    if s.languages.len() > 8 {
        languages.push(action("nav_transcription", "More…"));
    }
    items.push(submenu(
        "languages",
        format!("Language: {current_language}"),
        languages,
    ));
    let hold = wording("Hold to Talk", "Hold to talk");
    let toggle = wording("Press to Start and Stop", "Press to start and stop");
    items.push(submenu(
        "recording_mode",
        format!("Mode: {}", if s.hold { &hold } else { &toggle }),
        vec![
            check("recording_mode_push_to_talk", hold, s.hold),
            check("recording_mode_toggle", toggle, !s.hold),
        ],
    ));
    items.push(check(
        "live_preview",
        wording("Live Preview", "Live preview"),
        s.preview,
    ));
    items.push(Item::Separator);
    for (id, label) in [
        (
            "copy_last_transcription",
            wording("Copy Last Transcript", "Copy last transcript"),
        ),
        (
            "paste_last",
            wording("Paste Last Transcript", "Paste last transcript"),
        ),
    ] {
        items.push(if s.recent.is_empty() {
            disabled(id, label)
        } else {
            action(id, label)
        });
    }
    items.push(submenu(
        "recent",
        "Recent",
        if s.recent.is_empty() {
            vec![disabled(
                "empty_history",
                wording("No Transcripts Yet", "No transcripts yet"),
            )]
        } else {
            s.recent
                .iter()
                .take(5)
                .map(|(id, label)| action(&format!("recent_copy_{id}"), label))
                .collect()
        },
    ));
    items.push(action(
        "nav_file",
        wording("Transcribe a File…", "Transcribe a file…"),
    ));
    items.push(Item::Separator);
    items.push(action(
        "nav_home",
        wording("Open Voicetypr", "Open Voicetypr"),
    ));
    items.push(action("nav_insights", "Insights"));
    let mut settings = action("nav_settings", "Settings…");
    if let Item::Action { accelerator, .. } = &mut settings {
        *accelerator = Some("CmdOrCtrl+,".into());
    }
    items.push(settings);
    let updates_label = wording("Check for Updates…", "Check for updates…");
    items.push(if s.updates {
        action("check_updates", updates_label)
    } else {
        disabled("check_updates", updates_label)
    });
    items.push(action(
        "nav_help",
        wording("Help & Feedback", "Help & feedback"),
    ));
    items.push(Item::Separator);
    let mut quit = action("quit", "Quit Voicetypr");
    if !s.windows {
        if let Item::Action { accelerator, .. } = &mut quit {
            *accelerator = Some("Cmd+Q".into());
        }
    }
    items.push(quit);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item<'a>(items: &'a [Item], needle: &str) -> &'a Item {
        items.iter().find(|item| matches!(item, Item::Action { id, .. } | Item::Submenu { id, .. } if id == needle)).unwrap()
    }
    fn label(item: &Item) -> &str {
        match item {
            Item::Action { label, .. } | Item::Submenu { label, .. } => label,
            Item::Separator => "",
        }
    }
    fn children(item: &Item) -> &[Item] {
        match item {
            Item::Submenu { items, .. } => items,
            _ => panic!("Expected submenu"),
        }
    }
    #[test]
    fn ready_platform_order_and_shortcut() {
        for windows in [false, true] {
            let s = Snapshot {
                windows,
                engine: "Parakeet TDT".into(),
                shortcut: if windows {
                    "Ctrl+Alt+Space"
                } else {
                    "⌥Space"
                }
                .into(),
                polish: "Clean".into(),
                updates: true,
                ..Default::default()
            };
            let items = build(&s);
            assert_eq!(
                label(&items[0]),
                format!(
                    "● Ready · Parakeet TDT · on this {}",
                    if windows { "PC" } else { "Mac" }
                )
            );
            assert!(label(item(&items, "dictate")).ends_with(&s.shortcut));
            let ids = items
                .iter()
                .filter_map(|i| match i {
                    Item::Action { id, .. } | Item::Submenu { id, .. } => Some(id.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                ids,
                [
                    "status",
                    "dictate",
                    "polish",
                    "models",
                    "microphones",
                    "languages",
                    "recording_mode",
                    "live_preview",
                    "copy_last_transcription",
                    "paste_last",
                    "recent",
                    "nav_file",
                    "nav_home",
                    "nav_insights",
                    "nav_settings",
                    "check_updates",
                    "nav_help",
                    "quit"
                ]
            );
        }
    }
    #[test]
    fn recording_has_stop_and_clock_without_fix() {
        let s = Snapshot {
            recording: Some(7),
            blocked: Some("Mic busy".into()),
            ..Default::default()
        };
        let items = build(&s);
        assert_eq!(label(&items[0]), "● Recording · 0:07");
        assert!(label(item(&items, "dictate")).starts_with("Stop Dictation"));
        assert!(!items
            .iter()
            .any(|i| matches!(i, Item::Action { id, .. } if id == "fix")));
        assert_eq!(recording_label(67), "● Recording · 1:07");
    }
    #[test]
    fn blocked_and_kept_are_keyboard_actions() {
        let s = Snapshot {
            blocked: Some("Microphone missing".into()),
            kept: Some(("opaque".into(), Some("Whisper Tiny".into()))),
            ..Default::default()
        };
        let items = build(&s);
        assert_eq!(label(&items[0]), "● Microphone missing");
        assert_eq!(label(&items[1]), "Fix…");
        assert_eq!(
            label(item(&items, "retry_opaque")),
            "Retry Last Dictation with Whisper Tiny"
        );
        assert_eq!(
            label(item(&items, "discard_opaque")),
            "Discard Last Recording"
        );
    }
    #[test]
    fn no_models_still_has_download_and_system_mic() {
        let items = build(&Snapshot::default());
        assert!(matches!(
            item(children(item(&items, "models")), "nav_models"),
            Item::Action { enabled: true, .. }
        ));
        assert!(matches!(
            item(children(item(&items, "microphones")), "microphone_default"),
            Item::Action {
                checked: Some(true),
                ..
            }
        ));
    }
    #[test]
    fn groups_remote_and_cloud_in_engine_menu() {
        let s = Snapshot {
            engines: vec![
                ("tiny".into(), "Whisper Tiny".into(), "local".into(), false),
                ("soniox".into(), "Soniox".into(), "cloud".into(), false),
                (
                    "remote_lan".into(),
                    "Studio · Parakeet TDT".into(),
                    "network".into(),
                    true,
                ),
            ],
            ..Default::default()
        };
        let items = build(&s);
        let engines = children(item(&items, "models"));
        assert!(matches!(
            item(engines, "header_network"),
            Item::Action { enabled: false, .. }
        ));
        assert!(matches!(
            item(engines, "model_remote_lan"),
            Item::Action {
                checked: Some(true),
                ..
            }
        ));
    }
    #[test]
    fn many_languages_caps_at_eight_and_links_to_transcription() {
        let s = Snapshot {
            language: "3".into(),
            languages: (0..12)
                .map(|n| (n.to_string(), format!("Language {n}")))
                .collect(),
            ..Default::default()
        };
        let items = build(&s);
        let languages = children(item(&items, "languages"));
        assert_eq!(languages.len(), 9);
        assert!(matches!(
            item(languages, "language_3"),
            Item::Action {
                checked: Some(true),
                ..
            }
        ));
        assert_eq!(label(languages.last().unwrap()), "More…");
    }
}

#[cfg(test)]
mod busy_tests {
    use super::*;
    #[test]
    fn busy_kept_clip_keeps_discard_accessible() {
        let items = build(&Snapshot {
            kept: Some(("clip".into(), Some("Whisper Tiny".into()))),
            kept_busy: true,
            ..Default::default()
        });
        assert!(items
            .iter()
            .any(|i| matches!(i, Item::Action { id, enabled: false, .. } if id == "retry_clip")));
        assert!(items
            .iter()
            .any(|i| matches!(i, Item::Action { id, enabled: true, .. } if id == "discard_clip")));
    }
}
