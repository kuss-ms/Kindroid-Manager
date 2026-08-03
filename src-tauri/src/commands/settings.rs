use serde::{Deserialize, Serialize};

use crate::commands::push::{DEFAULT_BASE_URL, SETTING_BASE_URL_PUBLIC as SETTING_BASE_URL};
use crate::error::AppError;
use crate::kindroid::{HttpResponse, KindroidClient, UpdateInfoRequest};
use crate::security::secrets::{SecretStoreError, Secrets, API_TOKEN_KEY};
use crate::storage::Repository;

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingsDto {
    pub base_url: String,
    pub token_configured: bool,
    /// SettingsPage debug toggle. When true, the chat-automation cycle
    /// captures the raw AI provider response into process memory and
    /// surfaces it via `ChatAutomationDto::journal_last_response_debug`
    /// / `summary_last_response_debug`. Lives in the `settings` table;
    /// defaults to false.
    pub debug_show_automation_response: bool,
}

pub async fn get_settings(repo: std::sync::Arc<dyn Repository>) -> Result<SettingsDto, AppError> {
    let base_url = repo
        .get_setting(SETTING_BASE_URL)
        .await?
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let debug_show_automation_response = matches!(
        repo.get_setting(crate::commands::chat_automation::SETTING_DEBUG_SHOW_AUTOMATION_RESPONSE)
            .await?
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    );
    Ok(SettingsDto {
        base_url,
        token_configured: Secrets::exists(API_TOKEN_KEY),
        debug_show_automation_response,
    })
}

#[derive(Debug, Deserialize)]
pub struct SetSettingsInput {
    pub base_url: String,
}

pub async fn set_settings(
    repo: std::sync::Arc<dyn Repository>,
    input: SetSettingsInput,
) -> Result<(), AppError> {
    let trimmed = input.base_url.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(AppError::invalid("base_url is required"));
    }
    repo.set_setting(SETTING_BASE_URL, &trimmed).await?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct TokenStatus {
    pub configured: bool,
}

pub fn token_status() -> TokenStatus {
    TokenStatus {
        configured: Secrets::exists(API_TOKEN_KEY),
    }
}

pub fn set_token(token: String) -> Result<(), AppError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("token is required"));
    }
    Secrets::set(API_TOKEN_KEY, trimmed).map_err(map_secret_err)?;
    Ok(())
}

pub fn clear_token() -> Result<(), AppError> {
    Secrets::clear(API_TOKEN_KEY).map_err(map_secret_err)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct SetDebugFlagsInput {
    /// `true` to capture the raw AI provider response from each
    /// automation cycle in process memory (no DB writes). The value is
    /// surfaced via `ChatAutomationDto::journal_last_response_debug` /
    /// `summary_last_response_debug` so the AutomationPanel can render
    /// the most recent response for debugging.
    pub debug_show_automation_response: bool,
}

/// Persist the SettingsPage debug toggles. Today there is only one
/// (`debug_show_automation_response`); more can be added to the input
/// struct without changing the command signature.
pub async fn set_debug_flags(
    repo: std::sync::Arc<dyn Repository>,
    input: SetDebugFlagsInput,
) -> Result<(), AppError> {
    repo.set_setting(
        crate::commands::chat_automation::SETTING_DEBUG_SHOW_AUTOMATION_RESPONSE,
        if input.debug_show_automation_response {
            "true"
        } else {
            "false"
        },
    )
    .await?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct TestTokenResult {
    pub ok: bool,
    pub rate_limited: bool,
    pub message: String,
    pub status: u16,
}

/// Best-effort probe: POST `/update-info` with `{}`. The server rejects on
/// schema (400) if the token is valid, on auth (401/403) if not.
pub async fn test_token(
    repo: std::sync::Arc<dyn Repository>,
    client: std::sync::Arc<dyn KindroidClient>,
) -> Result<TestTokenResult, AppError> {
    let token = Secrets::get(API_TOKEN_KEY).map_err(map_secret_err)?;
    let base_url = repo
        .get_setting(SETTING_BASE_URL)
        .await?
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let resp: Result<HttpResponse, _> = client
        .update_info(
            &token,
            &base_url,
            UpdateInfoRequest {
                body: serde_json::json!({}),
            },
        )
        .await;
    let result = match resp {
        Ok(r) => TestTokenResult {
            ok: true,
            rate_limited: false,
            message: format!("OK (status {})", r.status),
            status: r.status,
        },
        Err(crate::kindroid::KindroidError::Auth { status, .. }) => TestTokenResult {
            ok: false,
            rate_limited: false,
            message: format!("Invalid token (status {status})"),
            status,
        },
        Err(crate::kindroid::KindroidError::RateLimited {
            status,
            retry_after,
            ..
        }) => TestTokenResult {
            ok: true,
            rate_limited: true,
            message: format!(
                "Rate limited; auth reached (status {status}{})",
                retry_after
                    .map(|d| format!(", retry in {}s", d.as_secs()))
                    .unwrap_or_default()
            ),
            status,
        },
        Err(crate::kindroid::KindroidError::BadRequest { status, .. }) => TestTokenResult {
            // 400 here means the schema probe was rejected because ai_id
            // is missing — that proves auth reached the server.
            ok: true,
            rate_limited: false,
            message: format!("OK (status {status}, auth reached)"),
            status,
        },
        Err(crate::kindroid::KindroidError::NotFound { status, .. }) => TestTokenResult {
            ok: false,
            rate_limited: false,
            message: format!("Unexpected 404 from server (status {status})"),
            status,
        },
        Err(crate::kindroid::KindroidError::Server { status, body }) => TestTokenResult {
            ok: false,
            rate_limited: false,
            message: format!("Server error {status}: {body}"),
            status,
        },
        Err(crate::kindroid::KindroidError::Network { message: msg }) => TestTokenResult {
            ok: false,
            rate_limited: false,
            message: format!("(network) {msg}"),
            status: 0,
        },
    };
    Ok(result)
}

fn map_secret_err(e: SecretStoreError) -> AppError {
    AppError::from(e)
}
