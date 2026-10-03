//! Copy original text without exposing clipboard content in results or logs.
use serde::Serialize;
use tauri_plugin_store::StoreExt;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CopyOriginalResult {
    Copied,
    Nothing,
}

fn last_original(entries: &[(String, serde_json::Value)]) -> Option<String> {
    let entry = &entries
        .iter()
        .max_by(|(left, _), (right, _)| left.cmp(right))?
        .1;
    entry
        .get("writing")
        .and_then(|writing| writing.get("original_text"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| entry.get("text").and_then(serde_json::Value::as_str))
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

async fn copy_original<F, Fut>(
    entries: &[(String, serde_json::Value)],
    copy: F,
) -> Result<CopyOriginalResult, String>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let Some(text) = last_original(entries) else {
        return Ok(CopyOriginalResult::Nothing);
    };
    copy(text)
        .await
        .map_err(|_| "Could not copy original".to_string())?;
    Ok(CopyOriginalResult::Copied)
}

#[tauri::command]
pub async fn copy_last_original(app: tauri::AppHandle) -> Result<CopyOriginalResult, String> {
    let entries = {
        let store = app
            .store("transcriptions")
            .map_err(|_| "Could not read history")?;
        store
            .keys()
            .into_iter()
            .filter_map(|key| store.get(&key).map(|value| (key, value)))
            .collect::<Vec<_>>()
    };
    copy_original(&entries, crate::commands::text::copy_text_to_clipboard).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    async fn copies_latest_original_or_unchanged_text_using_clipboard_seam() {
        let mut entries = vec![(
            "01".into(),
            json!({"text":"polished", "writing":{"original_text":"raw"}}),
        )];
        assert_eq!(
            copy_original(&entries, |text| async move {
                assert_eq!(text, "raw");
                Ok(())
            })
            .await
            .unwrap(),
            CopyOriginalResult::Copied
        );
        entries.push(("02".into(), json!({"text":"unchanged"})));
        assert_eq!(
            copy_original(&entries, |text| async move {
                assert_eq!(text, "unchanged");
                Ok(())
            })
            .await
            .unwrap(),
            CopyOriginalResult::Copied
        );
    }
    #[tokio::test]
    async fn nothing_does_not_touch_clipboard_and_errors_are_content_free() {
        assert_eq!(
            copy_original(&[], |_| async { panic!("clipboard called") })
                .await
                .unwrap(),
            CopyOriginalResult::Nothing
        );
        let empty_latest = vec![
            ("01".into(), json!({"text":"older"})),
            ("02".into(), json!({"text":""})),
        ];
        assert_eq!(
            copy_original(&empty_latest, |_| async { panic!("clipboard called") })
                .await
                .unwrap(),
            CopyOriginalResult::Nothing
        );
        let entries = vec![("01".into(), json!({"text":"value"}))];
        assert_eq!(
            copy_original(&entries, |_| async { Err("private clipboard data".into()) })
                .await
                .unwrap_err(),
            "Could not copy original"
        );
        assert_eq!(
            serde_json::to_string(&CopyOriginalResult::Copied).unwrap(),
            "\"copied\""
        );
        assert_eq!(
            serde_json::to_string(&CopyOriginalResult::Nothing).unwrap(),
            "\"nothing\""
        );
    }
}
