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
            model: "gpt-5-nano",
        },
        ProviderCase {
            id: PROVIDER_ANTHROPIC,
            model: "claude-sonnet-4-6",
        },
        ProviderCase {
            id: PROVIDER_GEMINI,
            model: "gemini-3-flash-preview",
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

            assert_eq!(error, AiProviderError::InvalidApiKey);
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

            assert_eq!(error, AiProviderError::ServiceUnavailable);
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
            assert_eq!(error, AiProviderError::Timeout);
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

            assert_eq!(error, AiProviderError::Canceled);
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

            assert_eq!(error, AiProviderError::BadResponse);
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
            model: "gemini-3-flash-preview",
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

        assert_eq!(error, AiProviderError::InvalidApiKey);
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
            error,
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

        assert_eq!(error, AiProviderError::BadResponse);
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

        assert_eq!(error, AiProviderError::BadResponse);
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

        assert_eq!(error, AiProviderError::Canceled);
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

        assert_eq!(error, AiProviderError::Timeout);
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
                "model": "claude-sonnet-4-6",
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
            _ => json!({
                "model": "gpt-5-nano",
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
                        err,
                        AiProviderError::UnsupportedProvider | AiProviderError::Timeout
                    ),
                    "expected graceful spawn failure (UnsupportedProvider/Timeout), got {err:?}",
                );
            }
            Some(_) => {
                if let Err(err) = result {
                    assert!(
                        matches!(
                            err,
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
