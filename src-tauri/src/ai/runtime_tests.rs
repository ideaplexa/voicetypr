#[cfg(test)]
mod tests {
    use super::super::contract::AiPolishRequest;
    use super::super::error::AiProviderError;
    use super::super::executor::{AiExecutor, OpenAiCompatibleConfig};
    use super::super::genai_runtime::AiKeyResolver;
    use super::super::providers::{PROVIDER_CLAUDE_CODE, PROVIDER_CUSTOM, PROVIDER_OPENROUTER};
    use reqwest::header::AUTHORIZATION;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    const PROVIDER_OPENAI: &str = "openai";
    const PROVIDER_ANTHROPIC: &str = "anthropic";
    const PROVIDER_GEMINI: &str = "gemini";

    #[derive(Clone, Copy, Debug)]
    struct ProviderCase {
        id: &'static str,
        model: &'static str,
    }

    const PROVIDERS: &[ProviderCase] = &[
        ProviderCase {
            id: PROVIDER_OPENAI,
            model: "gpt-6-luna",
        },
        ProviderCase {
            id: PROVIDER_ANTHROPIC,
            model: "claude-sonnet-5-5",
        },
        ProviderCase {
            id: PROVIDER_GEMINI,
            model: "gemini-3.5-flash-lite",
        },
        ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        },
    ];

    #[tokio::test]
    async fn ai_runtime_success_round_trip_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![ok_response(case.id, "polished")]).await;
            let executor = executor_for(*case, &server, true, false);

            let result = executor
                .polish(request(*case, 1_000), CancellationToken::new())
                .await
                .unwrap();

            assert_eq!(result.output_text, "polished");
            assert_eq!(result.provider_id, case.id);
            assert_eq!(result.model_id, case.model);
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn ai_runtime_maps_401_to_invalid_api_key_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![error_response(401, "bad key")]).await;
            let executor = executor_for(*case, &server, true, false);

            let error = executor
                .polish(request(*case, 1_000), CancellationToken::new())
                .await
                .unwrap_err();

            assert_eq!(error.error, AiProviderError::InvalidApiKey);
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn ai_runtime_retries_429_once_then_success_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(
                &server,
                case.id,
                vec![
                    error_response(429, "rate limited"),
                    ok_response(case.id, "polished"),
                ],
            )
            .await;
            let executor = executor_for(*case, &server, true, false);

            let result = executor
                .polish(request(*case, 1_000), CancellationToken::new())
                .await
                .unwrap();

            assert_eq!(result.output_text, "polished");
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn ai_runtime_retries_500_once_then_success_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(
                &server,
                case.id,
                vec![
                    error_response(500, "server error"),
                    ok_response(case.id, "polished"),
                ],
            )
            .await;
            let executor = executor_for(*case, &server, true, false);

            let result = executor
                .polish(request(*case, 1_000), CancellationToken::new())
                .await
                .unwrap();

            assert_eq!(result.output_text, "polished");
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn ai_runtime_retries_500_once_then_returns_service_unavailable_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(
                &server,
                case.id,
                vec![
                    error_response(500, "server error"),
                    error_response(500, "server error"),
                ],
            )
            .await;
            let executor = executor_for(*case, &server, true, false);

            let error = executor
                .polish(request(*case, 1_000), CancellationToken::new())
                .await
                .unwrap_err();

            assert_eq!(error.error, AiProviderError::ServiceUnavailable);
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn ai_runtime_timeout_uses_total_budget_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*"))
                .respond_with(ok_response(case.id, "late").set_delay(Duration::from_secs(2)))
                .mount(&server)
                .await;
            let executor = executor_for(*case, &server, true, false);
            let started = Instant::now();

            let error = executor
                .polish(request(*case, 150), CancellationToken::new())
                .await
                .unwrap_err();

            let elapsed = started.elapsed();
            assert_eq!(error.error, AiProviderError::Timeout);
            assert!(elapsed >= Duration::from_millis(100), "elapsed={elapsed:?}");
            assert!(elapsed < Duration::from_millis(900), "elapsed={elapsed:?}");
        }
    }

    #[tokio::test]
    async fn ai_runtime_cancellation_mid_flight_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(".*"))
                .respond_with(ok_response(case.id, "late").set_delay(Duration::from_secs(2)))
                .mount(&server)
                .await;
            let executor = executor_for(*case, &server, true, false);
            let token = CancellationToken::new();
            let child = token.clone();
            let request = request(*case, 2_000);
            let handle = tokio::spawn(async move { executor.polish(request, child).await });
            tokio::time::sleep(Duration::from_millis(50)).await;
            token.cancel();

            let error = handle.await.unwrap().unwrap_err();

            assert_eq!(error.error, AiProviderError::Canceled);
        }
    }

    #[tokio::test]
    async fn ai_runtime_empty_content_is_bad_response_for_all_providers() {
        for case in PROVIDERS {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![ok_response(case.id, "   ")]).await;
            let executor = executor_for(*case, &server, true, false);

            let error = executor
                .polish(request(*case, 1_000), CancellationToken::new())
                .await
                .unwrap_err();

            assert_eq!(error.error, AiProviderError::BadResponse);
        }
    }

    #[tokio::test]
    async fn ai_runtime_auth_header_from_key_source_and_custom_no_auth_supported() {
        for case in PROVIDERS.iter().filter(|case| case.id != PROVIDER_CUSTOM) {
            let server = MockServer::start().await;
            mount_sequence(&server, case.id, vec![ok_response(case.id, "polished")]).await;
            let executor = executor_for(*case, &server, true, false);

            executor
                .polish(request(*case, 1_000), CancellationToken::new())
                .await
                .unwrap();

            let request = server.received_requests().await.unwrap().remove(0);
            assert!(native_auth_header_present(case.id, &request));
        }

        let custom = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(&server, custom.id, vec![ok_response(custom.id, "polished")]).await;
        let executor = executor_for(custom, &server, false, true);

        executor
            .polish(request(custom, 1_000), CancellationToken::new())
            .await
            .unwrap();

        let request = server.received_requests().await.unwrap().remove(0);
        assert!(request.headers.get(AUTHORIZATION).is_none());
    }

    async fn sent_body(case: ProviderCase, input_text: &str) -> serde_json::Value {
        let server = MockServer::start().await;
        mount_sequence(&server, case.id, vec![ok_response(case.id, "polished")]).await;
        let executor = executor_for(case, &server, true, false);
        let mut polish_request = request(case, 1_000);
        polish_request.input_text = input_text.to_string();
        executor
            .polish(polish_request, CancellationToken::new())
            .await
            .unwrap();
        let received = server.received_requests().await.unwrap().remove(0);
        serde_json::from_slice(&received.body).unwrap()
    }

    async fn sent_request(
        case: ProviderCase,
        input_text: &str,
        reasoning_level: Option<&str>,
        fast_mode: bool,
    ) -> (Request, serde_json::Value) {
        let server = MockServer::start().await;
        mount_sequence(&server, case.id, vec![ok_response(case.id, "polished")]).await;
        let executor = executor_for(case, &server, true, false);
        let mut polish_request = request(case, 1_000);
        polish_request.input_text = input_text.to_string();
        polish_request.reasoning_level = reasoning_level.map(str::to_string);
        polish_request.fast_mode = fast_mode;
        executor
            .polish(polish_request, CancellationToken::new())
            .await
            .unwrap();
        let received = server.received_requests().await.unwrap().remove(0);
        let body = serde_json::from_slice(&received.body).unwrap();
        (received, body)
    }

    async fn sent_body_with_level(
        case: ProviderCase,
        input_text: &str,
        reasoning_level: Option<&str>,
    ) -> serde_json::Value {
        sent_request(case, input_text, reasoning_level, false)
            .await
            .1
    }

    async fn sent_body_with_level_and_fast(
        case: ProviderCase,
        input_text: &str,
        reasoning_level: Option<&str>,
        fast_mode: bool,
    ) -> serde_json::Value {
        sent_request(case, input_text, reasoning_level, fast_mode)
            .await
            .1
    }

    #[tokio::test]
    async fn validation_probes_use_latest_models_at_lowest_effort_without_fast() {
        for (id, model, expected_cap) in [
            (PROVIDER_OPENAI, "gpt-6-luna", 256),
            (PROVIDER_ANTHROPIC, "claude-haiku-5-5", 256),
            (PROVIDER_GEMINI, "gemini-3.5-flash-lite", 1280),
        ] {
            assert_eq!(super::super::catalog::default_model_id(id), Some(model));
            let effort = super::super::catalog::lowest_reasoning_level(id, model);
            let body =
                sent_body_with_level(ProviderCase { id, model }, "ok", effort.as_deref()).await;
            assert!(body.get("speed").is_none());
            assert!(body.get("service_tier").is_none());
            match id {
                PROVIDER_OPENAI => {
                    assert_eq!(body["reasoning"]["effort"], "none");
                    assert_eq!(body["max_output_tokens"], expected_cap);
                }
                PROVIDER_ANTHROPIC => {
                    assert_eq!(body["thinking"]["type"], "disabled");
                    assert_eq!(body["max_tokens"], expected_cap);
                }
                _ => {
                    assert_eq!(
                        body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                        "MINIMAL"
                    );
                    assert_eq!(body["generationConfig"]["maxOutputTokens"], expected_cap);
                }
            }
        }
    }

    #[tokio::test]
    async fn gemini_2_5_budget_medium_uses_legacy_budget_and_room_for_visible_output() {
        let body = sent_body_with_level(
            ProviderCase {
                id: PROVIDER_GEMINI,
                model: "gemini-2.5-flash",
            },
            "ok",
            Some("medium"),
        )
        .await;
        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            8000
        );
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 9472);
    }

    #[tokio::test]
    async fn ai_runtime_gpt_reasoning_models_get_no_temperature() {
        // Reasoning models reject any non-default temperature, so Polish
        // silently fell back to raw text when one was sent. gpt-5.6-luna is
        // the second recommended OpenAI model, used here as a placeholder.
        let body = sent_body(
            ProviderCase {
                id: PROVIDER_OPENAI,
                model: "gpt-5.6-luna",
            },
            "ok",
        )
        .await;
        assert!(body.get("temperature").is_none(), "{body}");
        // No stored level resolves to medium for gpt-5.6-luna.
        assert_eq!(body["reasoning"]["effort"], "medium");

        // Non-reasoning models keep a low temperature and no reasoning field.
        let body = sent_body(
            ProviderCase {
                id: PROVIDER_OPENAI,
                model: "gpt-4o",
            },
            "ok",
        )
        .await;
        assert_eq!(body["temperature"], 0.2);
        assert!(body.get("reasoning").is_none(), "{body}");
    }

    #[tokio::test]
    async fn ai_runtime_openai_responses_request_shape() {
        // gpt-6-luna rides the Responses API: instructions + input,
        // max_output_tokens, reasoning.effort, store:false.
        let case = ProviderCase {
            id: PROVIDER_OPENAI,
            model: "gpt-6-luna",
        };
        let body = sent_body_with_level(case, "ok", Some("medium")).await;
        assert_eq!(body["model"], "gpt-6-luna");
        assert_eq!(body["instructions"], "polish the transcript");
        assert_eq!(body["store"], false);
        assert_eq!(body["reasoning"]["effort"], "medium");
        assert!(body.get("temperature").is_none(), "{body}");
        let max_tokens = body["max_output_tokens"].as_u64().unwrap();
        // "ok" input -> visible 256 + medium allowance 9216.
        assert_eq!(max_tokens, 9472, "{body}");
        let input = body["input"].as_array().unwrap();
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[0]["content"], "ok");

        // Stored "none" resolves to none on luna.
        let body = sent_body_with_level(case, "ok", Some("none")).await;
        assert_eq!(body["reasoning"]["effort"], "none");

        // Fast mode asks for the fast service tier via extra_body.
        let body = sent_body_with_level_and_fast(case, "ok", Some("medium"), true).await;
        assert_eq!(body["service_tier"], "fast", "{body}");
        let body = sent_body_with_level_and_fast(case, "ok", Some("medium"), false).await;
        assert!(body.get("service_tier").is_none(), "{body}");
    }

    #[tokio::test]
    async fn ai_runtime_anthropic_new_models_use_effort_and_adaptive_thinking() {
        // Claude 5.x/Opus 4.7+ reject genai's legacy thinking shape; they ride
        // the raw Messages client with output_config.effort + adaptive.
        let body = sent_body_with_level(
            ProviderCase {
                id: PROVIDER_ANTHROPIC,
                model: "claude-haiku-5-5",
            },
            "ok",
            Some("medium"),
        )
        .await;
        assert_eq!(body["model"], "claude-haiku-5-5");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body.get("temperature").is_none(), "{body}");
        assert!(body.get("speed").is_none(), "{body}");

        // "none" on a toggle model = thinking disabled, no effort field.
        let body = sent_body_with_level(
            ProviderCase {
                id: PROVIDER_ANTHROPIC,
                model: "claude-haiku-5-5",
            },
            "ok",
            Some("none"),
        )
        .await;
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("output_config").is_none(), "{body}");

        // Opus 4.8 gets effort too (fixes the pre-existing rejection), and
        // fast mode adds speed + the beta header.
        let body = sent_body_with_level_and_fast(
            ProviderCase {
                id: PROVIDER_ANTHROPIC,
                model: "claude-opus-4-8",
            },
            "ok",
            Some("medium"),
            true,
        )
        .await;
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["speed"], "fast");
    }

    #[tokio::test]
    async fn ai_runtime_opus_4_8_fast_sends_beta_header() {
        let case = ProviderCase {
            id: PROVIDER_ANTHROPIC,
            model: "claude-opus-4-8",
        };
        let (request, _) = sent_request(case, "ok", Some("medium"), true).await;
        assert_eq!(
            request
                .headers
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok()),
            Some("fast-mode-2026-02-01")
        );
    }

    #[tokio::test]
    async fn ai_runtime_gemini_2_5_none_sends_zero_thinking_budget() {
        // gemini-2.5-flash "none" must actually disable thinking; genai only
        // emits thinkingBudget via the explicit Budget(0) variant.
        let body = sent_body_with_level(
            ProviderCase {
                id: PROVIDER_GEMINI,
                model: "gemini-2.5-flash",
            },
            "ok",
            Some("none"),
        )
        .await;
        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            0
        );
        assert!(
            body["generationConfig"].get("temperature").is_none(),
            "{body}"
        );
    }

    #[tokio::test]
    async fn ai_runtime_gemini_2_5_output_cap_clamps_to_catalog_limit() {
        // The 2.0.8 review found maxOutputTokens could exceed gemini's 65,536
        // output limit; the cap now clamps to the catalog limit.
        let body = sent_body_with_level(
            ProviderCase {
                id: PROVIDER_GEMINI,
                model: "gemini-2.5-flash",
            },
            &"x".repeat(200_000),
            Some("medium"),
        )
        .await;
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 65536);
    }

    #[tokio::test]
    async fn ai_runtime_legacy_anthropic_thinking_budget_stays_below_max_tokens() {
        // Legacy (budget-only) Claude models keep the genai path; this pins
        // that path specifically, hence non-recommended claude-haiku-4-5.
        // max_tokens must always exceed the thinking budget Anthropic sends.
        let body = sent_body_with_level(
            ProviderCase {
                id: PROVIDER_ANTHROPIC,
                model: "claude-haiku-4-5",
            },
            "ok",
            Some("medium"),
        )
        .await;
        assert!(body.get("temperature").is_none(), "{body}");
        let budget = body["thinking"]["budget_tokens"].as_u64().unwrap();
        assert!(body["max_tokens"].as_u64().unwrap() > budget, "{body}");
    }

    #[tokio::test]
    async fn ai_runtime_fast_refusal_retries_once_at_standard_speed() {
        // A fast-tier refusal (OpenAI 400 on service_tier) retries once at
        // the explicit default tier — omitting service_tier would fall back
        // to the project's Default Service Tier, which may itself be fast.
        let case = ProviderCase {
            id: PROVIDER_OPENAI,
            model: "gpt-6-luna",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                error_response(400, r#"{"error":{"message":"Invalid service_tier value"}}"#),
                ok_response(case.id, "polished"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let mut polish_request = request(case, 1_000);
        polish_request.fast_mode = true;

        let result = executor
            .polish(polish_request, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.output_text, "polished");
        assert!(result.fast_mode_fallback);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        let first: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        let second: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
        assert_eq!(first["service_tier"], "fast");
        assert_eq!(second["service_tier"], "default");
    }

    #[tokio::test]
    async fn ai_runtime_fast_fallback_flag_survives_standard_retry_failure() {
        // When the standard-speed retry also fails, the refusal is still
        // reported: callers must persist fast=false and emit the unavailable
        // event even though no polished text was produced.
        let case = ProviderCase {
            id: PROVIDER_OPENAI,
            model: "gpt-6-luna",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                error_response(400, r#"{"error":{"message":"Invalid service_tier value"}}"#),
                error_response(500, "server error"),
                error_response(500, "server error"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let mut polish_request = request(case, 1_000);
        polish_request.fast_mode = true;

        let failure = executor
            .polish(polish_request, CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(failure.error, AiProviderError::ServiceUnavailable);
        assert!(failure.fast_mode_fallback);
        let requests = server.received_requests().await.unwrap();
        // fast refusal + standard attempt + one validation retry.
        assert_eq!(requests.len(), 3);
        let second: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
        assert_eq!(second["service_tier"], "default");
    }
    #[tokio::test]
    async fn ai_runtime_anthropic_fast_refusal_retries_at_standard_speed() {
        // Anthropic 403/429 refusals mentioning speed/fast also retry once
        // at standard speed.
        let case = ProviderCase {
            id: PROVIDER_ANTHROPIC,
            model: "claude-opus-4-8",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                error_response(429, r#"{"error":{"message":"fast mode rate limited"}}"#),
                ok_response(case.id, "polished"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let mut polish_request = request(case, 1_000);
        polish_request.fast_mode = true;

        let result = executor
            .polish(polish_request, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.output_text, "polished");
        assert!(result.fast_mode_fallback);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn ai_runtime_anthropic_non_natural_stop_reasons_are_bad_response() {
        // model_context_window_exceeded and refusal are partial/failed
        // responses — pasting them would inject truncated text or a refusal
        // notice, so they must surface as BadResponse like max_tokens.
        let case = ProviderCase {
            id: PROVIDER_ANTHROPIC,
            model: "claude-haiku-5-5",
        };
        for stop_reason in ["max_tokens", "model_context_window_exceeded", "refusal"] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(path_for_provider(case.id)))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "model": case.model,
                    "content": [{ "type": "text", "text": "partial output" }],
                    "stop_reason": stop_reason
                })))
                .mount(&server)
                .await;
            let executor = executor_for(case, &server, true, false);

            let failure = executor
                .polish(request(case, 1_000), CancellationToken::new())
                .await
                .unwrap_err();

            assert_eq!(
                failure.error,
                AiProviderError::BadResponse,
                "stop_reason {stop_reason} should error, not paste"
            );
        }
    }

    #[tokio::test]
    async fn ai_runtime_auth_error_never_triggers_fast_fallback() {
        // A 401 must not downgrade to standard speed — auth errors are real.
        let case = ProviderCase {
            id: PROVIDER_OPENAI,
            model: "gpt-6-luna",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                error_response(401, "unauthorized"),
                ok_response(case.id, "should not be used"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let mut polish_request = request(case, 1_000);
        polish_request.fast_mode = true;

        let error = executor
            .polish(polish_request, CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.error, AiProviderError::InvalidApiKey);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ai_runtime_max_tokens_stop_is_bad_response_not_pasted() {
        // A response cut off at the token limit must error (raw-text
        // fallback) instead of injecting partial text.
        let case = ProviderCase {
            id: PROVIDER_OPENAI,
            model: "gpt-6-luna",
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(path_for_provider(case.id)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "gpt-6-luna",
                "status": "incomplete",
                "id": "resp_test",
                "output": [{
                    "type": "message",
                    "content": [{"type": "output_text", "text": "partial"}]
                }]
            })))
            .mount(&server)
            .await;
        let executor = executor_for(case, &server, true, false);

        let error = executor
            .polish(request(case, 1_000), CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.error, AiProviderError::BadResponse);
    }

    #[tokio::test]
    async fn ai_runtime_openrouter_routes_via_openai_compatible_with_attribution_headers() {
        // OpenRouter must route through the OpenAI-compatible runtime (never the
        // genai adapter), carrying its own bearer key plus the app-attribution
        // headers. The embedded catalog marks `openrouter` as runtime
        // "openai_compatible", so dispatch resolves it here.
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            PROVIDER_OPENROUTER,
            vec![ok_response(PROVIDER_OPENROUTER, "polished")],
        )
        .await;

        let key_resolver: AiKeyResolver = Arc::new(|provider_id| {
            if provider_id == PROVIDER_OPENROUTER {
                Some("test-key".to_string())
            } else {
                None
            }
        });
        let executor = AiExecutor::with_native_endpoint_overrides(
            reqwest::Client::new(),
            key_resolver,
            OpenAiCompatibleConfig {
                base_url: server.uri(),
                no_auth: false,
                key_provider_id: PROVIDER_OPENROUTER.to_string(),
                extra_headers: vec![
                    (
                        "HTTP-Referer".to_string(),
                        "https://voicetypr.com".to_string(),
                    ),
                    ("X-Title".to_string(), "VoiceTypr".to_string()),
                ],
            },
            HashMap::new(),
        );

        let result = executor
            .polish(
                AiPolishRequest {
                    provider_id: PROVIDER_OPENROUTER.to_string(),
                    model_id: "google/gemini-2.5-flash-lite".to_string(),
                    reasoning_level: None,
                    fast_mode: false,
                    input_text: "raw transcript".to_string(),
                    prompt: "polish the transcript".to_string(),
                    timeout_ms: 1_000,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.output_text, "polished");
        assert_eq!(result.provider_id, PROVIDER_OPENROUTER);

        let request = server.received_requests().await.unwrap().remove(0);
        assert_eq!(
            request
                .headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-key")
        );
        assert_eq!(
            request
                .headers
                .get("HTTP-Referer")
                .and_then(|value| value.to_str().ok()),
            Some("https://voicetypr.com")
        );
        assert_eq!(
            request
                .headers
                .get("X-Title")
                .and_then(|value| value.to_str().ok()),
            Some("VoiceTypr")
        );
    }

    #[tokio::test]
    async fn ai_runtime_maps_gemini_api_key_invalid_400_to_invalid_api_key() {
        let case = ProviderCase {
            id: PROVIDER_GEMINI,
            model: "gemini-3.8-flash",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![error_response(
                400,
                r#"{"error":{"message":"API key not valid. API_KEY_INVALID for model lookup"}}"#,
            )],
        )
        .await;
        let executor = executor_for(case, &server, true, false);

        let error = executor
            .polish(request(case, 1_000), CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.error, AiProviderError::InvalidApiKey);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ai_runtime_does_not_retry_retry_after_beyond_remaining_budget() {
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                retry_after_response(429, "rate limited", "2"),
                ok_response(case.id, "should not be used"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let started = Instant::now();

        let error = executor
            .polish(request(case, 150), CancellationToken::new())
            .await
            .unwrap_err();

        let elapsed = started.elapsed();
        assert!(matches!(
            error.error,
            AiProviderError::Timeout | AiProviderError::RateLimited
        ));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        assert!(elapsed < Duration::from_millis(300), "elapsed={elapsed:?}");
    }

    #[tokio::test]
    async fn ai_runtime_retries_429_without_retry_after_immediately() {
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                error_response(429, "rate limited"),
                ok_response(case.id, "polished"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let started = Instant::now();

        let result = executor
            .polish(request(case, 1_000), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.output_text, "polished");
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
        assert!(started.elapsed() < Duration::from_millis(300));
    }

    #[tokio::test]
    async fn ai_runtime_retries_once_on_refusal_content_then_succeeds() {
        // Distinct from the HTTP-error retry path (should_retry): a 200-OK
        // response whose content is a refusal fails validate_ai_output
        // (BadResponse), triggering EXACTLY one retry. First call refuses,
        // second call returns clean text — the executor must recover and have
        // hit the server exactly twice.
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                ok_response(case.id, "I'm sorry, I can't help with that."),
                ok_response(case.id, "Polished transcript here."),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);

        let result = executor
            .polish(request(case, 1_000), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.output_text, "Polished transcript here.");
        // Exactly one retry → two total requests, no more, no fewer.
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn ai_runtime_gives_up_after_one_retry_when_content_stays_malformed() {
        // The validate-on-content retry fires ONCE: a persistently malformed
        // response surfaces BadResponse after the single retry, never an
        // infinite loop. Complements the success-recovery case to pin the retry
        // ceiling at exactly one.
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                ok_response(case.id, "I'm sorry, I can't do that."),
                ok_response(case.id, "I cannot help with this request."),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);

        let error = executor
            .polish(request(case, 1_000), CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.error, AiProviderError::BadResponse);
        // One attempt + one retry, then it stops: exactly two requests.
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn ai_runtime_retries_once_on_truncated_content_then_succeeds() {
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        let truncated = "x".repeat(4097);
        mount_sequence(
            &server,
            case.id,
            vec![
                ok_response(case.id, &truncated),
                ok_response(case.id, "Polished transcript here."),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);

        let result = executor
            .polish(request(case, 1_000), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.output_text, "Polished transcript here.");
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn ai_runtime_gives_up_after_one_retry_when_content_stays_truncated() {
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        let truncated = "x".repeat(4097);
        mount_sequence(
            &server,
            case.id,
            vec![
                ok_response(case.id, &truncated),
                ok_response(case.id, &truncated),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);

        let error = executor
            .polish(request(case, 1_000), CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.error, AiProviderError::BadResponse);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn ai_runtime_retries_after_http_date_retry_after_within_budget() {
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                retry_after_response(
                    429,
                    "rate limited",
                    &http_date_after(Duration::from_secs(2)),
                ),
                ok_response(case.id, "polished"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let started = Instant::now();

        let result = executor
            .polish(request(case, 3_500), CancellationToken::new())
            .await
            .unwrap();

        let elapsed = started.elapsed();
        assert_eq!(result.output_text, "polished");
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
        assert!(elapsed >= Duration::from_millis(500), "elapsed={elapsed:?}");
    }

    #[tokio::test]
    async fn ai_runtime_cancels_during_retry_after_backoff() {
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                retry_after_response(429, "rate limited", "2"),
                ok_response(case.id, "should not be used"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let token = CancellationToken::new();
        let child = token.clone();
        let request = request(case, 5_000);
        let started = Instant::now();
        let handle = tokio::spawn(async move { executor.polish(request, child).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        token.cancel();

        let error = handle.await.unwrap().unwrap_err();

        assert_eq!(error.error, AiProviderError::Canceled);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[tokio::test]
    async fn ai_runtime_shared_executor_handles_concurrent_polish_calls_independently() {
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            case.id,
            vec![
                ok_response(case.id, "polished one"),
                ok_response(case.id, "polished two"),
            ],
        )
        .await;
        let executor = executor_for(case, &server, true, false);
        let first = executor.clone();
        let second = executor.clone();
        let first_request = request(case, 1_000);
        let second_request = request(case, 1_000);

        let (first_result, second_result) = tokio::join!(
            first.polish(first_request, CancellationToken::new()),
            second.polish(second_request, CancellationToken::new())
        );

        let mut outputs = vec![
            first_result.unwrap().output_text,
            second_result.unwrap().output_text,
        ];
        outputs.sort();
        assert_eq!(outputs, vec!["polished one", "polished two"]);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn ai_runtime_response_just_after_budget_expiry_times_out() {
        let case = ProviderCase {
            id: PROVIDER_CUSTOM,
            model: "custom-model",
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(path_for_provider(case.id)))
            .respond_with(ok_response(case.id, "late").set_delay(Duration::from_millis(120)))
            .mount(&server)
            .await;
        let executor = executor_for(case, &server, true, false);

        let error = executor
            .polish(request(case, 100), CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.error, AiProviderError::Timeout);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    async fn mount_sequence(
        server: &MockServer,
        provider_id: &str,
        responses: Vec<ResponseTemplate>,
    ) {
        let counter = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(responses);
        Mock::given(method("POST"))
            .and(path_regex(path_for_provider(provider_id)))
            .respond_with(move |_request: &Request| {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                responses
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| responses.last().unwrap().clone())
            })
            .mount(server)
            .await;
    }

    fn executor_for(
        case: ProviderCase,
        server: &MockServer,
        include_key: bool,
        custom_no_auth: bool,
    ) -> AiExecutor {
        let key_resolver: AiKeyResolver = Arc::new(move |_provider_id| {
            if include_key {
                Some("test-key".to_string())
            } else {
                None
            }
        });
        let mut overrides = HashMap::new();
        if case.id != PROVIDER_CUSTOM {
            overrides.insert(case.id.to_string(), server.uri());
        }
        AiExecutor::with_native_endpoint_overrides(
            reqwest::Client::new(),
            key_resolver,
            OpenAiCompatibleConfig::custom(server.uri(), custom_no_auth),
            overrides,
        )
    }

    fn request(case: ProviderCase, timeout_ms: u64) -> AiPolishRequest {
        AiPolishRequest {
            provider_id: case.id.to_string(),
            model_id: case.model.to_string(),
            reasoning_level: None,
            fast_mode: false,
            input_text: "raw transcript".to_string(),
            prompt: "polish the transcript".to_string(),
            timeout_ms,
        }
    }

    fn ok_response(provider_id: &str, content: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(match provider_id {
            PROVIDER_ANTHROPIC => json!({
                "model": "claude-sonnet-5-5",
                "content": [{ "type": "text", "text": content }],
                "stop_reason": "end_turn"
            }),
            PROVIDER_GEMINI => json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{ "text": content }]
                    },
                    "finishReason": "STOP"
                }]
            }),
            PROVIDER_OPENAI => json!({
                "model": "gpt-6-luna",
                "status": "completed",
                "id": "resp_test",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": content }]
                }]
            }),
            _ => json!({
                "model": "custom-model",
                "choices": [{
                    "message": { "role": "assistant", "content": content },
                    "finish_reason": "stop"
                }]
            }),
        })
    }

    fn error_response(status: u16, body: &str) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_string(body.to_string())
    }

    fn retry_after_response(status: u16, body: &str, retry_after: &str) -> ResponseTemplate {
        error_response(status, body).insert_header("Retry-After", retry_after)
    }

    fn http_date_after(duration: Duration) -> String {
        let retry_at = chrono::Utc::now()
            + chrono::Duration::from_std(duration).expect("test duration fits chrono");
        retry_at.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
    }

    fn path_for_provider(provider_id: &str) -> &'static str {
        match provider_id {
            PROVIDER_ANTHROPIC => r"^/messages$",
            PROVIDER_GEMINI => r"^/models/.+:generateContent$",
            PROVIDER_OPENAI => r"^/responses$",
            _ => r"^/chat/completions$",
        }
    }

    fn native_auth_header_present(provider_id: &str, request: &Request) -> bool {
        match provider_id {
            PROVIDER_OPENAI => {
                request
                    .headers
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    == Some("Bearer test-key")
            }
            PROVIDER_ANTHROPIC => {
                request
                    .headers
                    .get("x-api-key")
                    .and_then(|value| value.to_str().ok())
                    == Some("test-key")
            }
            PROVIDER_GEMINI => {
                request
                    .headers
                    .get("x-goog-api-key")
                    .and_then(|value| value.to_str().ok())
                    == Some("test-key")
            }
            _ => false,
        }
    }

    // Ignored: on a machine where `claude` is installed this actually spawns the
    // real CLI (and the first `resolve_binary` triggers the ~8s login-shell PATH
    // probe), so it is neither hermetic nor free. Routing (claude-code ->
    // AgentCliRuntime, not the HTTP paths) is proven hermetically by the catalog
    // `claude_code_provider_is_agent_cli_runtime_and_not_native` test. Run this
    // end-to-end variant manually with `--ignored` on a machine with the CLI.
    #[tokio::test]
    #[ignore = "spawns the real claude CLI + login-shell probe when installed; routing covered hermetically by the catalog runtime_kind test"]
    async fn execute_once_routes_agent_cli_provider_and_fails_gracefully_when_binary_missing() {
        // claude-code is an agent_cli provider. execute_once must route it to
        // AgentCliRuntime (NOT the HTTP/openai-compatible path, NOT the
        // UnsupportedProvider dispatch fallback). In CI the `claude` binary is
        // absent, so the cold spawn fails — but gracefully (Err, not panic),
        // proving the graceful spawn-failure path that backs the raw-transcript
        // fallback. The short budget bounds the test so a slow login-shell PATH
        // probe surfaces Timeout rather than hanging.
        let executor = AiExecutor::with_native_endpoint_overrides(
            reqwest::Client::new(),
            Arc::new(|_| None),
            OpenAiCompatibleConfig::custom(String::new(), true),
            HashMap::new(),
        );
        let request = AiPolishRequest {
            provider_id: PROVIDER_CLAUDE_CODE.to_string(),
            model_id: String::new(),
            reasoning_level: Some("low".to_string()),
            fast_mode: false,
            input_text: "raw transcript".to_string(),
            prompt: "polish the transcript".to_string(),
            timeout_ms: 2_000,
        };
        let result = executor.polish(request, CancellationToken::new()).await;
        // claude-code must route to AgentCliRuntime (NOT the HTTP/openai-
        // compatible path, NOT the UnsupportedProvider dispatch fallback).
        // The graceful outcome depends on whether `claude` is installed
        // locally: absent (CI) -> UnsupportedProvider/Timeout; present (a dev
        // machine with the CLI actually launches now that the child PATH is
        // restored) -> the spawn reaches the runtime and either succeeds (Ok)
        // or fails with a real variant. All prove graceful routing; only a
        // panic or an unexpected (wrong-runtime) variant fails the invariant.
        match super::super::agent_cli::resolve_binary("claude").await {
            None => {
                let err = result.expect_err(
                    "agent_cli cold spawn must fail gracefully when the binary is missing",
                );
                assert!(
                    matches!(
                        err.error,
                        AiProviderError::UnsupportedProvider | AiProviderError::Timeout
                    ),
                    "expected graceful spawn failure (UnsupportedProvider/Timeout), got {err:?}",
                );
            }
            Some(_) => {
                if let Err(err) = result {
                    assert!(
                        matches!(
                            err.error,
                            AiProviderError::UnsupportedProvider
                                | AiProviderError::Timeout
                                | AiProviderError::BadResponse
                                | AiProviderError::Internal
                        ),
                        "expected a graceful agent_cli outcome, got {err:?}",
                    );
                }
            }
        }
    }
}
