use rig_core::completion::CompletionError;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("{}", api_error_display(*status, code.as_deref(), message))]
    Api { status: u16, code: Option<String>, message: String },
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

fn api_error_display(status: u16, code: Option<&str>, message: &str) -> String {
    match code {
        Some(c) if !c.is_empty() => format!("API error {status} [{c}]: {message}"),
        _ => format!("API error {status}: {message}"),
    }
}

impl LlmError {
    /// Parse an HTTP error response body into a structured `Api` error.
    ///
    /// Supports the common OpenAI-compatible shape:
    /// `{"error": {"message": "...", "type": "...", "code": "..."}}`
    /// Falls back to the raw body when the shape is unrecognized.
    pub fn api_from_response(status: u16, body: String) -> Self {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(err_obj) = parsed.get("error").and_then(|v| v.as_object()) {
                let message = err_obj
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&body)
                    .to_string();
                let code = err_obj
                    .get("code")
                    .or_else(|| err_obj.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                return LlmError::Api {
                    status,
                    code: code.filter(|c| !c.eq_ignore_ascii_case("unknown_error")),
                    message,
                };
            }
        }
        LlmError::Api { status, code: None, message: body }
    }
}

/// Map a rig `CompletionError` onto the simplified `LlmError`.
///
/// Errors that preserve a provider response body (both the `ProviderResponse`
/// variant and `HttpError` wrapping a non-success HTTP response) become a
/// structured [`LlmError::Api`] via [`LlmError::api_from_response`]; everything
/// else becomes [`LlmError::Other`] (or [`LlmError::Serde`] for JSON errors).
pub fn map_completion_error(error: CompletionError) -> LlmError {
    if let Some(body) = error.provider_response_body() {
        let status = error
            .provider_response_status()
            .map(|s| s.as_u16())
            .unwrap_or(0);
        return LlmError::api_from_response(status, body.to_string());
    }
    match error {
        CompletionError::JsonError(e) => LlmError::Serde(e),
        other => LlmError::Other(other.to_string()),
    }
}

impl From<String> for LlmError {
    fn from(s: String) -> Self {
        LlmError::Other(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use rig_core::ProviderResponseError;

    #[test]
    fn parses_openai_style_error_with_code() {
        let body = r#"{"error":{"message":"InsufficientBalance","type":"unknown_error","param":null,"code":"invalid_request_error"}}"#.to_string();
        let err = LlmError::api_from_response(402, body);
        match err {
            LlmError::Api { status, ref code, ref message } => {
                assert_eq!(status, 402);
                assert_eq!(code.as_deref(), Some("invalid_request_error"));
                assert_eq!(message, "InsufficientBalance");
            },
            other => panic!("expected Api, got {other:?}"),
        }
        assert_eq!(
            err.to_string(),
            "API error 402 [invalid_request_error]: InsufficientBalance"
        );
    }

    #[test]
    fn falls_back_to_type_when_code_absent() {
        let body =
            r#"{"error":{"message":"rate limited","type":"rate_limit_exceeded"}}"#.to_string();
        let err = LlmError::api_from_response(429, body);
        match err {
            LlmError::Api { status, ref code, ref message } => {
                assert_eq!(status, 429);
                assert_eq!(code.as_deref(), Some("rate_limit_exceeded"));
                assert_eq!(message, "rate limited");
            },
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn omits_brackets_when_code_is_unknown_error() {
        let body = r#"{"error":{"message":"boom","type":"unknown_error"}}"#.to_string();
        let err = LlmError::api_from_response(500, body);
        assert_eq!(err.to_string(), "API error 500: boom");
    }

    #[test]
    fn uses_raw_body_when_not_json() {
        let err = LlmError::api_from_response(502, "Bad Gateway".to_string());
        match err {
            LlmError::Api { status, ref code, ref message } => {
                assert_eq!(status, 502);
                assert!(code.is_none());
                assert_eq!(message, "Bad Gateway");
            },
            other => panic!("expected Api, got {other:?}"),
        }
        assert_eq!(err.to_string(), "API error 502: Bad Gateway");
    }

    #[test]
    fn uses_raw_body_when_error_shape_unrecognized() {
        let body = r#"{"detail":"not found"}"#.to_string();
        let err = LlmError::api_from_response(404, body.clone());
        match err {
            LlmError::Api { status, ref code, ref message } => {
                assert_eq!(status, 404);
                assert!(code.is_none());
                assert_eq!(message, &body);
            },
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn provider_response_maps_to_structured_api_error() {
        let error = CompletionError::ProviderResponse(ProviderResponseError {
            status: Some(StatusCode::BAD_REQUEST),
            body: r#"{"error":{"message":"bad request","type":"invalid_request_error"}}"#
                .to_string(),
        });
        match map_completion_error(error) {
            LlmError::Api { status, code, message } => {
                assert_eq!(status, 400);
                assert_eq!(code.as_deref(), Some("invalid_request_error"));
                assert_eq!(message, "bad request");
            },
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn provider_error_maps_to_other() {
        let mapped = map_completion_error(CompletionError::ProviderError("boom".to_string()));
        assert!(matches!(mapped, LlmError::Other(s) if s.contains("boom")));
    }

    #[test]
    fn json_error_maps_to_serde() {
        let mapped = map_completion_error(CompletionError::JsonError(
            serde_json::from_str::<serde_json::Value>("not json").unwrap_err(),
        ));
        assert!(matches!(mapped, LlmError::Serde(_)));
    }
}
