use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};
use reqwest::Client;

use super::{
    parse_retry_after, ChatBreakRequest, ChatMessagesPage, CreateNewAiRequest, HttpResponse,
    JournalCreateRequest, KindroidError, ListChatMessagesRequest, ToggleMessagePinRequest,
    ToggleMessagePinResponse, UpdateInfoRequest, REQUEST_TIMEOUT,
};

#[async_trait]
pub trait KindroidClient: Send + Sync {
    async fn create_new_ai(
        &self,
        token: &str,
        base_url: &str,
        req: CreateNewAiRequest,
    ) -> Result<HttpResponse, KindroidError>;
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
    async fn list_chat_messages(
        &self,
        token: &str,
        base_url: &str,
        req: ListChatMessagesRequest,
    ) -> Result<ChatMessagesPage, KindroidError>;
    async fn toggle_message_pin(
        &self,
        token: &str,
        base_url: &str,
        req: ToggleMessagePinRequest,
    ) -> Result<ToggleMessagePinResponse, KindroidError>;
    async fn journal_create(
        &self,
        token: &str,
        base_url: &str,
        req: JournalCreateRequest<'_>,
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
            Err(e) => {
                return Err(KindroidError::Network {
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
        if ok {
            return Ok(HttpResponse {
                status,
                ok: true,
                body,
            });
        }
        Err(map_error(status, &headers, body))
    }

    async fn get_json(&self, url: &str, token: &str) -> Result<HttpResponse, KindroidError> {
        let resp = match self
            .http
            .get(url)
            .headers(Self::build_headers(token))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(KindroidError::Network {
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
    async fn create_new_ai(
        &self,
        token: &str,
        base_url: &str,
        req: CreateNewAiRequest,
    ) -> Result<HttpResponse, KindroidError> {
        let url = format!("{}/create-new-ai", base_url.trim_end_matches('/'));
        self.post_json(&url, token, req.body).await
    }

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

    async fn list_chat_messages(
        &self,
        token: &str,
        base_url: &str,
        req: ListChatMessagesRequest,
    ) -> Result<ChatMessagesPage, KindroidError> {
        let limit = req.limit.clamp(1, 100);
        let mut url = format!(
            "{}/get-chat-messages?ai_id={}&limit={}",
            base_url.trim_end_matches('/'),
            urlencoding(&req.ai_id),
            limit
        );
        if let Some(ts) = req.start_after_timestamp {
            // Always pass the cursor (even when `0`) so the API's
            // pagination behaves consistently across calls. The server
            // treats `0` as "from the beginning" so this is safe.
            url.push_str(&format!("&start_after_timestamp={ts}"));
        }
        let resp = self.get_json(&url, token).await?;
        let parsed: ChatMessagesResponse =
            serde_json::from_str(&resp.body).map_err(|e| KindroidError::Server {
                status: resp.status,
                body: format!("invalid chat-history JSON: {e}"),
            })?;
        let messages = parsed.messages;
        let pagination = parsed.pagination;
        let pagination_last_ts = pagination.as_ref().and_then(|p| p.last_timestamp);
        let pagination_has_more = pagination.as_ref().and_then(|p| p.has_more);
        // Prefer the server's `hasMore` flag; if absent, infer from the
        // page size (a full page usually implies there's more).
        let has_more = pagination_has_more.unwrap_or((messages.len() as u32) >= limit);
        Ok(ChatMessagesPage {
            messages,
            has_more,
            limit,
            pagination_last_timestamp: pagination_last_ts,
        })
    }

    async fn toggle_message_pin(
        &self,
        token: &str,
        base_url: &str,
        req: ToggleMessagePinRequest,
    ) -> Result<ToggleMessagePinResponse, KindroidError> {
        let url = format!("{}/toggle-message-pin", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "ai_id": req.ai_id,
            "message_id": req.message_id,
        });
        let resp = self.post_json(&url, token, body).await?;
        serde_json::from_str(&resp.body).map_err(|e| KindroidError::Server {
            status: resp.status,
            body: format!("invalid toggle-message-pin JSON: {e}"),
        })
    }

    async fn journal_create(
        &self,
        token: &str,
        base_url: &str,
        req: JournalCreateRequest<'_>,
    ) -> Result<HttpResponse, KindroidError> {
        let url = format!("{}/journal-create", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "ai_id": req.ai_id,
            "entry": req.entry,
            "keyphrases": req.keyphrases,
        });
        self.post_json(&url, token, body).await
    }
}

#[derive(serde::Deserialize, Default)]
struct ChatMessagesResponse {
    #[serde(default)]
    messages: Vec<super::RawChatMessage>,
    #[serde(default)]
    pagination: Option<ChatMessagesPagination>,
}

#[derive(serde::Deserialize, Default)]
struct ChatMessagesPagination {
    #[serde(default, rename = "lastTimestamp")]
    last_timestamp: Option<i64>,
    #[serde(default, rename = "hasMore")]
    has_more: Option<bool>,
}

fn urlencoding(s: &str) -> String {
    // Minimal percent-encoding for query-string values: only alphanumerics,
    // `-`, `_`, `.`, `~` pass through.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let is_unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn create_new_ai_200_returns_ai_id_in_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/create-new-ai"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ai_NEW_OK"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let r = c
            .create_new_ai(
                "t",
                &server.uri(),
                CreateNewAiRequest {
                    body: json!({"ai_name":"Aria"}),
                },
            )
            .await
            .unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, "ai_NEW_OK");
    }

    #[tokio::test]
    async fn create_new_ai_sends_authorization_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/create-new-ai"))
            .and(header("Authorization", "Bearer t"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ai_NEW_OK"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let r = c
            .create_new_ai(
                "t",
                &server.uri(),
                CreateNewAiRequest {
                    body: json!({"ai_name":"Aria"}),
                },
            )
            .await
            .unwrap();
        assert!(r.ok);
    }

    #[tokio::test]
    async fn create_new_ai_400_maps_to_bad_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/create-new-ai"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .create_new_ai(
                "t",
                &server.uri(),
                CreateNewAiRequest {
                    body: json!({"ai_name":""}),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::BadRequest { status: 400, .. }));
    }

    #[tokio::test]
    async fn create_new_ai_401_maps_to_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/create-new-ai"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .create_new_ai(
                "t",
                &server.uri(),
                CreateNewAiRequest {
                    body: json!({"ai_name":"Aria"}),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::Auth { status: 401, .. }));
    }

    #[tokio::test]
    async fn create_new_ai_500_maps_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/create-new-ai"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .create_new_ai(
                "t",
                &server.uri(),
                CreateNewAiRequest {
                    body: json!({"ai_name":"Aria"}),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::Server { status: 500, .. }));
    }

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

    #[tokio::test]
    async fn list_chat_messages_200_with_results() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    json!({
                        "messages": [
                            {
                                "id": "m1",
                                "sender": "user",
                                "display_name": "Alice",
                                "timestamp": 1_700_000_000_000i64,
                                "message": "hello there",
                                "image_urls": ["https://x/1.png"],
                                "link_url": "https://example.com"
                            },
                            {
                                "id": "m2",
                                "sender": "ai",
                                "display_name": null,
                                "timestamp": 1_700_000_001_000i64,
                                "message": null
                            }
                        ]
                    })
                    .to_string(),
                ),
            )
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let page = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].id, "m1");
        assert_eq!(page.messages[0].message.as_deref(), Some("hello there"));
        assert_eq!(page.messages[1].message, None);
        // Two messages returned, but limit=100, so there isn't necessarily more.
        assert!(!page.has_more);
        // No pagination object → no cursor.
        assert_eq!(page.pagination_last_timestamp, None);
    }

    #[tokio::test]
    async fn list_chat_messages_200_full_page_signals_has_more() {
        let server = MockServer::start().await;
        // Build a 5-message response and use limit=5.
        let mut msgs = Vec::new();
        for i in 0..5 {
            msgs.push(json!({
                "id": format!("m{i}"),
                "sender": "user",
                "timestamp": i as i64,
                "message": "x"
            }));
        }
        let body = json!({ "messages": msgs }).to_string();
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let page = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 5,
                    start_after_timestamp: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(page.messages.len(), 5);
        assert!(page.has_more, "a full page implies there may be more");
    }

    #[tokio::test]
    async fn list_chat_messages_200_parses_pagination_object() {
        let server = MockServer::start().await;
        let body = json!({
            "messages": [
                {
                    "id": "m1",
                    "sender": "user",
                    "timestamp": 1_700_000_000_000i64,
                    "message": "x"
                },
                {
                    "id": "m2",
                    "sender": "user",
                    "timestamp": 1_700_000_001_000i64,
                    "message": "y"
                }
            ],
            "pagination": {
                "lastTimestamp": 1_700_000_001_000i64,
                "hasMore": true
            }
        })
        .to_string();
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .and(query_param("start_after_timestamp", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let page = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: Some(0),
                },
            )
            .await
            .unwrap();
        assert_eq!(page.messages.len(), 2);
        // Server's pagination object wins.
        assert_eq!(page.pagination_last_timestamp, Some(1_700_000_001_000));
        assert!(page.has_more);
    }

    #[tokio::test]
    async fn list_chat_messages_200_last_page_has_more_false() {
        let server = MockServer::start().await;
        let body = json!({
            "messages": [
                {
                    "id": "m1",
                    "sender": "user",
                    "timestamp": 1_700_000_000_000i64,
                    "message": "x"
                }
            ],
            "pagination": {
                "lastTimestamp": 1_700_000_000_000i64,
                "hasMore": false
            }
        })
        .to_string();
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let page = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(page.messages.len(), 1);
        // Server says we're done — has_more wins over the "full page" heuristic.
        assert!(!page.has_more);
        assert_eq!(page.pagination_last_timestamp, Some(1_700_000_000_000));
    }

    #[tokio::test]
    async fn list_chat_messages_200_empty_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(json!({"messages": []}).to_string()),
            )
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let page = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: None,
                },
            )
            .await
            .unwrap();
        assert!(page.messages.is_empty());
        assert!(!page.has_more);
    }

    #[tokio::test]
    async fn list_chat_messages_400_maps_to_bad_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::BadRequest { status: 400, .. }));
    }

    #[tokio::test]
    async fn list_chat_messages_401_maps_to_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::Auth { status: 401, .. }));
    }

    #[tokio::test]
    async fn list_chat_messages_429_with_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "42")
                    .set_body_string("slow"),
            )
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: None,
                },
            )
            .await
            .unwrap_err();
        match err {
            KindroidError::RateLimited { retry_after, .. } => {
                assert_eq!(retry_after, Some(Duration::from_secs(42)));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_chat_messages_429_without_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: None,
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
    async fn list_chat_messages_500_maps_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/get-chat-messages"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .list_chat_messages(
                "t",
                &server.uri(),
                ListChatMessagesRequest {
                    ai_id: "ai_x".into(),
                    limit: 100,
                    start_after_timestamp: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::Server { status: 500, .. }));
    }

    #[tokio::test]
    async fn toggle_message_pin_200_true() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/toggle-message-pin"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(json!({ "isPinned": true }).to_string()),
            )
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let r = c
            .toggle_message_pin(
                "t",
                &server.uri(),
                ToggleMessagePinRequest {
                    ai_id: "ai_x".into(),
                    message_id: "m1".into(),
                },
            )
            .await
            .unwrap();
        assert!(r.is_pinned);
    }

    #[tokio::test]
    async fn toggle_message_pin_200_false() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/toggle-message-pin"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(json!({ "isPinned": false }).to_string()),
            )
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let r = c
            .toggle_message_pin(
                "t",
                &server.uri(),
                ToggleMessagePinRequest {
                    ai_id: "ai_x".into(),
                    message_id: "m1".into(),
                },
            )
            .await
            .unwrap();
        assert!(!r.is_pinned);
    }

    #[tokio::test]
    async fn toggle_message_pin_invalid_json_maps_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/toggle-message-pin"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .toggle_message_pin(
                "t",
                &server.uri(),
                ToggleMessagePinRequest {
                    ai_id: "ai_x".into(),
                    message_id: "m1".into(),
                },
            )
            .await
            .unwrap_err();
        match err {
            KindroidError::Server { status, body } => {
                assert_eq!(status, 200);
                assert!(body.contains("invalid toggle-message-pin JSON"));
            }
            other => panic!("expected Server, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn toggle_message_pin_401_maps_to_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/toggle-message-pin"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .toggle_message_pin(
                "t",
                &server.uri(),
                ToggleMessagePinRequest {
                    ai_id: "ai_x".into(),
                    message_id: "m1".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::Auth { status: 401, .. }));
    }

    #[tokio::test]
    async fn toggle_message_pin_404_maps_to_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/toggle-message-pin"))
            .respond_with(ResponseTemplate::new(404).set_body_string(""))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .toggle_message_pin(
                "t",
                &server.uri(),
                ToggleMessagePinRequest {
                    ai_id: "ai_x".into(),
                    message_id: "missing".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::NotFound { status: 404, .. }));
    }

    #[tokio::test]
    async fn toggle_message_pin_500_maps_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/toggle-message-pin"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .toggle_message_pin(
                "t",
                &server.uri(),
                ToggleMessagePinRequest {
                    ai_id: "ai_x".into(),
                    message_id: "m1".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::Server { status: 500, .. }));
    }

    #[tokio::test]
    async fn journal_create_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/journal-create"))
            .and(header("Authorization", "Bearer t"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let phrases = vec!["memory".to_string(), "anchor".to_string()];
        let r = c
            .journal_create(
                "t",
                &server.uri(),
                JournalCreateRequest {
                    ai_id: "ai_x",
                    entry: "Once upon a time",
                    keyphrases: &phrases,
                },
            )
            .await
            .unwrap();
        assert_eq!(r.status, 200);
        assert!(r.ok);
    }

    #[tokio::test]
    async fn journal_create_400_maps_to_bad_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/journal-create"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .journal_create(
                "t",
                &server.uri(),
                JournalCreateRequest {
                    ai_id: "ai_x",
                    entry: "x",
                    keyphrases: &[],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::BadRequest { status: 400, .. }));
    }

    #[tokio::test]
    async fn journal_create_401_maps_to_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/journal-create"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .journal_create(
                "t",
                &server.uri(),
                JournalCreateRequest {
                    ai_id: "ai_x",
                    entry: "x",
                    keyphrases: &[],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::Auth { status: 401, .. }));
    }

    #[tokio::test]
    async fn journal_create_429_with_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/journal-create"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "5")
                    .set_body_string("slow"),
            )
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .journal_create(
                "t",
                &server.uri(),
                JournalCreateRequest {
                    ai_id: "ai_x",
                    entry: "x",
                    keyphrases: &[],
                },
            )
            .await
            .unwrap_err();
        match err {
            KindroidError::RateLimited { retry_after, .. } => {
                assert_eq!(retry_after, Some(Duration::from_secs(5)))
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn journal_create_500_maps_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/journal-create"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let c = HttpKindroidClient::new();
        let err = c
            .journal_create(
                "t",
                &server.uri(),
                JournalCreateRequest {
                    ai_id: "ai_x",
                    entry: "x",
                    keyphrases: &[],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, KindroidError::Server { status: 500, .. }));
    }
}
