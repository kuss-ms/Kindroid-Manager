use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};
use reqwest::Client;

use super::{
    parse_retry_after, ChatBreakRequest, HttpResponse, KindroidError, UpdateInfoRequest,
    REQUEST_TIMEOUT,
};

#[async_trait]
pub trait KindroidClient: Send + Sync {
    async fn update_info(
        &self,
        token: &str,
        base_url: &str,
        req: UpdateInfoRequest,
    ) -> Result<HttpResponse, KindroidError>;
    async fn chat_break(
        &self,
        token: &str,
        base_url: &str,
        req: ChatBreakRequest,
    ) -> Result<HttpResponse, KindroidError>;
}

#[derive(Clone)]
pub struct HttpKindroidClient {
    http: Client,
}

impl HttpKindroidClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self { http }
    }

    fn build_headers(token: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).expect("bearer"),
        );
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h
    }

    async fn post_json(
        &self,
        url: &str,
        token: &str,
        body: serde_json::Value,
    ) -> Result<HttpResponse, KindroidError> {
        let resp = match self
            .http
            .post(url)
            .headers(Self::build_headers(token))
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return Err(KindroidError::Network(e.to_string())),
        };
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|e| format!("(read body error: {e})"));
        let ok = (200..300).contains(&status);
        if ok {
            return Ok(HttpResponse {
                status,
                ok: true,
                body,
            });
        }
        Err(map_error(status, &headers, body))
    }
}

impl Default for HttpKindroidClient {
    fn default() -> Self {
        Self::new()
    }
}

fn map_error(status: u16, headers: &HeaderMap, body: String) -> KindroidError {
    match status {
        401 | 403 => KindroidError::Auth { status, body },
        404 => KindroidError::NotFound { status, body },
        429 => {
            let retry_after = headers
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            KindroidError::RateLimited {
                status,
                body,
                retry_after,
            }
        }
        400..=499 => KindroidError::BadRequest { status, body },
        _ => KindroidError::Server { status, body },
    }
}

#[async_trait]
impl KindroidClient for HttpKindroidClient {
    async fn update_info(
        &self,
        token: &str,
        base_url: &str,
        req: UpdateInfoRequest,
    ) -> Result<HttpResponse, KindroidError> {
        let url = format!("{}/update-info", base_url.trim_end_matches('/'));
        self.post_json(&url, token, req.body).await
    }

    async fn chat_break(
        &self,
        token: &str,
        base_url: &str,
        req: ChatBreakRequest,
    ) -> Result<HttpResponse, KindroidError> {
        let url = format!("{}/chat-break", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "ai_id": req.ai_id,
            "greeting": req.greeting,
            "wipe_cascaded": req.wipe_cascaded,
        });
        self.post_json(&url, token, body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn update_info_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/update-info"))
            .and(header("Authorization", "Bearer t"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let r = c
            .update_info(
                "t",
                &server.uri(),
                UpdateInfoRequest {
                    body: json!({"ai_id":"a","ai_name":"A"}),
                },
            )
            .await
            .unwrap();
        assert_eq!(r.status, 200);
        assert!(r.ok);
    }

    #[tokio::test]
    async fn update_info_401_maps_to_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/update-info"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .update_info(
                "t",
                &server.uri(),
                UpdateInfoRequest {
                    body: json!({"ai_id":"a"}),
                },
            )
            .await
            .unwrap_err();
        matches!(err, KindroidError::Auth { status: 401, .. });
    }

    #[tokio::test]
    async fn update_info_429_with_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/update-info"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "12")
                    .set_body_string("slow down"),
            )
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .update_info(
                "t",
                &server.uri(),
                UpdateInfoRequest {
                    body: json!({"ai_id":"a"}),
                },
            )
            .await
            .unwrap_err();
        match err {
            KindroidError::RateLimited { retry_after, .. } => {
                assert_eq!(retry_after, Some(Duration::from_secs(12)));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_info_429_without_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/update-info"))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .update_info(
                "t",
                &server.uri(),
                UpdateInfoRequest {
                    body: json!({"ai_id":"a"}),
                },
            )
            .await
            .unwrap_err();
        match err {
            KindroidError::RateLimited { retry_after, .. } => assert!(retry_after.is_none()),
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_info_400_maps_to_bad_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/update-info"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .update_info(
                "t",
                &server.uri(),
                UpdateInfoRequest {
                    body: json!({"ai_id":""}),
                },
            )
            .await
            .unwrap_err();
        match err {
            KindroidError::BadRequest { status, .. } => assert_eq!(status, 400),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_info_404_maps_to_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/update-info"))
            .respond_with(ResponseTemplate::new(404).set_body_string(""))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .update_info(
                "t",
                &server.uri(),
                UpdateInfoRequest {
                    body: json!({"ai_id":"a"}),
                },
            )
            .await
            .unwrap_err();
        match err {
            KindroidError::NotFound { status, .. } => assert_eq!(status, 404),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_info_500_maps_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/update-info"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .update_info(
                "t",
                &server.uri(),
                UpdateInfoRequest {
                    body: json!({"ai_id":"a"}),
                },
            )
            .await
            .unwrap_err();
        match err {
            KindroidError::Server { status, .. } => assert_eq!(status, 500),
            other => panic!("expected Server, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn chat_break_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat-break"))
            .and(header("Authorization", "Bearer t"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let r = c
            .chat_break(
                "t",
                &server.uri(),
                ChatBreakRequest {
                    ai_id: "a".into(),
                    greeting: "hi".into(),
                    wipe_cascaded: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(r.status, 200);
    }
}
