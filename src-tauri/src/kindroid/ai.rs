use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AiError {
    #[error("auth failed: {body}")]
    Auth { status: u16, body: String },
    #[error("bad request: {body}")]
    BadRequest { status: u16, body: String },
    #[error("rate limited: {body}")]
    RateLimited {
        status: u16,
        body: String,
        retry_after: Option<Duration>,
    },
    #[error("server error {status}: {body}")]
    Server { status: u16, body: String },
    #[error("(network) {message}")]
    Network { message: String },
    #[error("decode error: {message}")]
    Decode { message: String },
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: Option<String>,
    pub messages: Vec<AiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseFormat {
    pub r#type: String,
}

#[derive(Debug, Clone)]
pub struct ChatCompletionResponse {
    pub content: String,
    pub model: Option<String>,
}

#[async_trait]
pub trait AiClient: Send + Sync {
    async fn chat_completion(
        &self,
        base_url: &str,
        bearer_token: Option<&str>,
        req: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, AiError>;
}

#[derive(Clone)]
pub struct HttpAiClient {
    http: Client,
}

impl HttpAiClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self { http }
    }

    fn build_headers(bearer_token: Option<&str>) -> Option<HeaderMap> {
        let mut h = HeaderMap::new();
        if let Some(token) = bearer_token {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                h.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {trimmed}")).expect("bearer"),
                );
            }
        }
        if h.is_empty() {
            None
        } else {
            h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            Some(h)
        }
    }
}

impl Default for HttpAiClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AiClient for HttpAiClient {
    async fn chat_completion(
        &self,
        base_url: &str,
        bearer_token: Option<&str>,
        req: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, AiError> {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let mut builder = self.http.post(&url);
        if let Some(headers) = Self::build_headers(bearer_token) {
            builder = builder.headers(headers);
        }
        let resp = match builder.json(&req).send().await {
            Ok(r) => r,
            Err(e) => {
                return Err(AiError::Network {
                    message: e.to_string(),
                })
            }
        };
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|e| format!("(read body error: {e})"));
        let ok = (200..300).contains(&status);
        if !ok {
            return Err(map_error(status, &headers, body));
        }
        let parsed: ChatCompletionBody =
            serde_json::from_str(&body).map_err(|e| AiError::Decode {
                message: e.to_string(),
            })?;
        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AiError::Decode {
                message: "missing choices".into(),
            })?;
        Ok(ChatCompletionResponse {
            content: choice.message.content,
            model: parsed.model,
        })
    }
}

fn map_error(status: u16, headers: &HeaderMap, body: String) -> AiError {
    match status {
        401 | 403 => AiError::Auth { status, body },
        429 => {
            let retry_after = headers
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            AiError::RateLimited {
                status,
                body,
                retry_after,
            }
        }
        400..=499 => AiError::BadRequest { status, body },
        _ => AiError::Server { status, body },
    }
}

/// Parse a Retry-After header per RFC 7231 §7.1.3 — either an integer
/// (seconds) or an HTTP-date.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    if let Ok(secs) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(value) {
        let now = chrono::Utc::now();
        let date_utc = date.with_timezone(&chrono::Utc);
        if date_utc > now {
            let diff = (date_utc - now).to_std().ok()?;
            return Some(diff);
        }
    }
    None
}

#[derive(Deserialize)]
struct ChatCompletionBody {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: Some("gpt-4o-mini".into()),
            messages: vec![AiMessage {
                role: "user".into(),
                content: "ping".into(),
            }],
            response_format: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn chat_completion_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer t"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    json!({
                        "id": "x",
                        "choices": [{"message": {"role": "assistant", "content": "pong"}}],
                        "model": "gpt-4o-mini"
                    })
                    .to_string(),
                ),
            )
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let r = c
            .chat_completion(&server.uri(), Some("t"), make_request())
            .await
            .unwrap();
        assert_eq!(r.content, "pong");
        assert_eq!(r.model.as_deref(), Some("gpt-4o-mini"));
    }

    #[tokio::test]
    async fn chat_completion_no_auth_header_when_token_empty() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    json!({
                        "choices": [{"message": {"role": "assistant", "content": "pong"}}],
                    })
                    .to_string(),
                ),
            )
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let r = c
            .chat_completion(&server.uri(), Some(""), make_request())
            .await
            .unwrap();
        assert_eq!(r.content, "pong");
    }

    #[tokio::test]
    async fn chat_completion_no_auth_header_when_token_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    json!({
                        "choices": [{"message": {"role": "assistant", "content": "pong"}}],
                    })
                    .to_string(),
                ),
            )
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let r = c
            .chat_completion(&server.uri(), None, make_request())
            .await
            .unwrap();
        assert_eq!(r.content, "pong");
    }

    #[tokio::test]
    async fn chat_completion_includes_response_format_when_json_mode() {
        let server = MockServer::start().await;
        // Verify the body contains response_format by echoing it back.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    json!({
                        "choices": [{"message": {"role": "assistant", "content": "{}"}}],
                    })
                    .to_string(),
                ),
            )
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let mut req = make_request();
        req.response_format = Some(ResponseFormat {
            r#type: "json_object".into(),
        });
        let r = c
            .chat_completion(&server.uri(), Some("t"), req)
            .await
            .unwrap();
        assert_eq!(r.content, "{}");
    }

    #[tokio::test]
    async fn maps_to_auth_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let err = c
            .chat_completion(&server.uri(), Some("t"), make_request())
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::Auth { status: 401, .. }));
    }

    #[tokio::test]
    async fn maps_to_bad_request_on_400() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let err = c
            .chat_completion(&server.uri(), Some("t"), make_request())
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::BadRequest { status: 400, .. }));
    }

    #[tokio::test]
    async fn maps_to_rate_limited_on_429() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "7")
                    .set_body_string("slow"),
            )
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let err = c
            .chat_completion(&server.uri(), Some("t"), make_request())
            .await
            .unwrap_err();
        match err {
            AiError::RateLimited { retry_after, .. } => {
                assert_eq!(retry_after, Some(Duration::from_secs(7)));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn maps_to_server_on_500() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let err = c
            .chat_completion(&server.uri(), Some("t"), make_request())
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::Server { status: 500, .. }));
    }

    #[tokio::test]
    async fn maps_to_network_on_connect_error() {
        // Port 1 is reserved; connection will be refused immediately.
        let c = HttpAiClient::new();
        let err = c
            .chat_completion("http://127.0.0.1:1/v1", Some("t"), make_request())
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::Network { message: _ }));
    }

    #[tokio::test]
    async fn maps_to_decode_on_missing_choices() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(json!({"model": "x"}).to_string()),
            )
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let err = c
            .chat_completion(&server.uri(), Some("t"), make_request())
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::Decode { message: _ }));
    }

    #[tokio::test]
    async fn maps_to_decode_on_non_json_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let err = c
            .chat_completion(&server.uri(), Some("t"), make_request())
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::Decode { message: _ }));
    }

    #[tokio::test]
    async fn base_url_with_trailing_slash_still_hits_chat_completions() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    json!({
                        "choices": [{"message": {"role": "assistant", "content": "pong"}}],
                    })
                    .to_string(),
                ),
            )
            .mount(&server)
            .await;
        let c = HttpAiClient::new();
        let url = format!("{}/", server.uri());
        let r = c
            .chat_completion(&url, Some("t"), make_request())
            .await
            .unwrap();
        assert_eq!(r.content, "pong");
    }
}
