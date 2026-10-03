//! ISO language sets for the curated engines; no provider calls while building menus.
// Provider tables verified 2026-10-03:
// https://soniox.com/docs/stt/concepts/supported-languages
// https://docs.cohere.com/docs/transcribe
// https://developers.deepgram.com/docs/models-languages-overview/
const SONIOX: &str = "af sq ar az eu be bn bs bg ca zh hr cs da nl en et fi fr gl de el gu he hi hu id it ja kn kk ko lv lt mk ms ml mr no fa pl pt pa ro ru sr sk sl es sw sv tl ta te th tr uk ur vi cy";
const COHERE: &str = "en de fr it es pt el nl pl vi zh ar ja ko";
const NOVA_2: &str = "bg ca zh cs da nl en et fi fr de el hi hu id it ja ko lv lt ms no pl pt ro ru sk es sv th tr uk vi";
const NOVA_3: &str = "af ar hy as be bn bs bg ca zh hr cs da nl en et fi fr ka de el gu he hi hu id it ja kn kk ko lv lt mk ms mr mn ne no ps fa pl pt pa ro ru sr sk sl es sv tl ta te th tr uk ur vi";
pub fn supported(model: &str, engine: &str, cloud_model: Option<&str>) -> Vec<(String, String)> {
    let codes = if let Some(definition) = crate::parakeet::models::AVAILABLE_MODELS
        .iter()
        .find(|m| m.id == model)
    {
        definition.languages.to_vec()
    } else if engine == "whisper" && model.ends_with(".en") {
        vec!["en"]
    } else if let Some(set) = match engine {
        "cohere" => Some(COHERE),
        "soniox" => Some(SONIOX),
        "deepgram" if cloud_model == Some("nova-2") => Some(NOVA_2),
        "deepgram" => Some(NOVA_3),
        _ => None,
    } {
        set.split_whitespace().collect()
    } else {
        crate::whisper::languages::SUPPORTED_LANGUAGES
            .keys()
            .copied()
            .collect()
    };
    let common = ["en", "es", "fr", "de", "zh", "ja", "pt", "hi"];
    let mut languages = codes
        .into_iter()
        .map(|code| {
            (
                code.to_owned(),
                crate::whisper::languages::SUPPORTED_LANGUAGES
                    .get(code)
                    .map(|l| l.name)
                    .unwrap_or(code)
                    .to_owned(),
            )
        })
        .collect::<Vec<_>>();
    languages.sort_by_key(|(code, name)| {
        (
            common.iter().position(|c| c == code).unwrap_or(8),
            name.clone(),
        )
    });
    languages
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_languages_match_model_catalog() {
        assert_eq!(
            supported("tiny.en", "whisper", None),
            [("en".into(), "English".into())]
        );
        for model in crate::parakeet::models::AVAILABLE_MODELS.iter() {
            assert_eq!(
                supported(model.id, "parakeet", None).len(),
                model.languages.len()
            );
        }
    }
    #[test]
    fn cloud_languages_are_provider_and_model_specific() {
        assert_eq!(supported("cohere", "cohere", None).len(), 14);
        assert!(!supported("cohere", "cohere", None)
            .iter()
            .any(|(code, _)| code == "hi"));
        assert!(supported("soniox", "soniox", None)
            .iter()
            .any(|(code, _)| code == "bn"));
        assert!(
            supported("deepgram", "deepgram", Some("nova-3")).len()
                > supported("deepgram", "deepgram", Some("nova-2")).len()
        );
    }
}
