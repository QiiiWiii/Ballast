//! HTTP/WebSocket authentication and role checks.
//!
//! When `BALLAST_OIDC_ISSUER` is unset, paper APIs stay open (open mode).
//! When set, Bearer JWT is required for non-public API routes; the event
//! WebSocket accepts a single-use ticket instead of a long-lived header.

mod oidc;
mod policy;
mod tickets;

#[cfg(test)]
mod test_keys;

pub use oidc::{OidcConfig, OidcValidator};
pub use policy::{AuthMode, Role, Session, authorize_request, role_rank};
pub use tickets::{TicketService, issue_ws_ticket};

use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, State},
    http::{HeaderMap, header, request::Parts},
};
use serde::Serialize;

use crate::AppState;
use crate::api::{ApiError, ApiResult};

use self::oidc::OidcValidatorInner;

pub(crate) const WS_TICKET_SUBPROTOCOL: &str = "ballast-ticket";
const WS_TICKET_VALUE_PREFIX: &str = "ballast-ticket-value.";

#[derive(Clone)]
pub struct AuthState {
    pub mode: AuthMode,
    pub oidc: Option<Arc<OidcValidator>>,
    pub tickets: TicketService,
}

impl AuthState {
    pub fn from_env(database: ballast_storage::DatabasePool) -> Result<Self, String> {
        let issuer = std::env::var("BALLAST_OIDC_ISSUER")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let audience = std::env::var("BALLAST_OIDC_AUDIENCE")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let role_claim = std::env::var("BALLAST_OIDC_ROLE_CLAIM")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "ballast_roles".to_owned());
        let clock_skew_secs = match std::env::var("BALLAST_OIDC_CLOCK_SKEW_SECS") {
            Ok(value) => parse_clock_skew_secs(&value)?,
            Err(std::env::VarError::NotPresent) => 60,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err("BALLAST_OIDC_CLOCK_SKEW_SECS must be an unsigned integer".into());
            }
        };
        let tickets = TicketService::new(database);

        match (issuer, audience) {
            (None, None) => Ok(Self {
                mode: AuthMode::Open,
                oidc: None,
                tickets,
            }),
            (Some(issuer), Some(audience)) => {
                let config = OidcConfig {
                    issuer,
                    audience,
                    role_claim,
                    clock_skew_secs,
                };
                let validator = OidcValidatorInner::new(config)?;
                Ok(Self {
                    mode: AuthMode::Oidc,
                    oidc: Some(Arc::new(OidcValidator::from_inner(validator))),
                    tickets,
                })
            }
            (Some(_), None) => {
                Err("BALLAST_OIDC_AUDIENCE is required when BALLAST_OIDC_ISSUER is set".into())
            }
            (None, Some(_)) => {
                Err("BALLAST_OIDC_ISSUER is required when BALLAST_OIDC_AUDIENCE is set".into())
            }
        }
    }

    pub fn readiness_status(&self) -> &'static str {
        match self.mode {
            AuthMode::Open => "not_configured",
            AuthMode::Oidc => "configured",
        }
    }
}

fn parse_clock_skew_secs(value: &str) -> Result<u64, String> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| "BALLAST_OIDC_CLOCK_SKEW_SECS must be an unsigned integer".to_owned())
}

#[derive(Debug, Clone, Serialize)]
pub struct Principal {
    pub subject: String,
    pub roles: Vec<Role>,
}

impl Principal {
    pub fn has_at_least(&self, required: Role) -> bool {
        self.roles
            .iter()
            .copied()
            .any(|role| role_rank(role) >= role_rank(required))
    }
}

/// Authenticated or open-mode session for handlers.
#[derive(Debug, Clone)]
pub struct RequestAuth {
    pub session: Session,
}

impl RequestAuth {
    /// Open deployment: `Ok(None)`. OIDC user: principal when role check passes.
    pub fn require_role(&self, role: Role) -> ApiResult<Option<&Principal>> {
        match &self.session {
            Session::Open => Ok(None),
            Session::Anonymous => Err(ApiError::unauthorized("missing_bearer_token")),
            Session::User(principal) => {
                if principal.has_at_least(role) {
                    Ok(Some(principal))
                } else {
                    Err(ApiError::forbidden("insufficient_role"))
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn actor_id(&self) -> Option<&str> {
        match &self.session {
            Session::Open | Session::Anonymous => None,
            Session::User(principal) => Some(principal.subject.as_str()),
        }
    }
}

impl FromRequestParts<AppState> for RequestAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(session) = parts.extensions.get::<Session>().cloned() {
            return Ok(Self { session });
        }
        // Fallback when middleware is not applied (tests / nested routers).
        if state.auth.mode == AuthMode::Open {
            return Ok(Self {
                session: Session::Open,
            });
        }
        Err(ApiError::unauthorized("missing_bearer_token"))
    }
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Result<&str, ApiError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing_bearer_token"))?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing_bearer_token"))
}

/// Enforce OIDC on non-public routes and attach `Session`.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, ApiError> {
    let required = authorize_request(state.auth.mode, req.method(), req.uri())?;
    let path = req.uri().path().to_owned();
    let is_ws = path == "/api/v1/ws";

    let session = if state.auth.mode == AuthMode::Open {
        Session::Open
    } else if is_ws {
        let raw = ws_ticket_from_headers(req.headers())?;
        let principal = state.auth.tickets.consume(raw).await?;
        if !principal.has_at_least(Role::Viewer) {
            return Err(ApiError::forbidden("insufficient_role"));
        }
        Session::User(principal)
    } else if let Some(role) = required {
        let token = bearer_token(req.headers())?;
        let validator = state
            .auth
            .oidc
            .as_ref()
            .ok_or_else(|| ApiError::internal("oidc_not_initialized"))?;
        let principal = validator.authenticate(token).await?;
        if !principal.has_at_least(role) {
            return Err(ApiError::forbidden("insufficient_role"));
        }
        Session::User(principal)
    } else {
        // Public route under OIDC: identity optional.
        match bearer_token(req.headers()) {
            Ok(token) => {
                let validator = state
                    .auth
                    .oidc
                    .as_ref()
                    .ok_or_else(|| ApiError::internal("oidc_not_initialized"))?;
                match validator.authenticate(token).await {
                    Ok(principal) => Session::User(principal),
                    Err(error) => return Err(error),
                }
            }
            Err(_) => Session::Anonymous,
        }
    };

    req.extensions_mut().insert(session);
    Ok(next.run(req).await)
}

fn ws_ticket_from_headers(headers: &HeaderMap) -> Result<&str, ApiError> {
    let protocols = headers
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("ws_ticket_required"))?;
    let mut has_ticket_protocol = false;
    let mut ticket = None;
    for protocol in protocols.split(',').map(str::trim) {
        if protocol == WS_TICKET_SUBPROTOCOL {
            has_ticket_protocol = true;
        } else if let Some(value) = protocol.strip_prefix(WS_TICKET_VALUE_PREFIX) {
            if !value.is_empty() {
                ticket = Some(value);
            }
        }
    }
    if !has_ticket_protocol {
        return Err(ApiError::unauthorized("ws_ticket_required"));
    }
    ticket.ok_or_else(|| ApiError::unauthorized("ws_ticket_required"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_ws_ticket_from_subprotocols() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            "ballast-ticket, ballast-ticket-value.bws_test"
                .parse()
                .unwrap(),
        );
        assert_eq!(ws_ticket_from_headers(&headers).unwrap(), "bws_test");
    }

    #[test]
    fn rejects_ws_ticket_in_query_only() {
        let headers = HeaderMap::new();
        let error = ws_ticket_from_headers(&headers).unwrap_err();
        assert_eq!(error.code(), "ws_ticket_required");
    }

    #[test]
    fn requires_the_ticket_subprotocol_marker() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            "ballast-ticket-value.bws_test".parse().unwrap(),
        );
        let error = ws_ticket_from_headers(&headers).unwrap_err();
        assert_eq!(error.code(), "ws_ticket_required");
    }

    #[test]
    fn rejects_invalid_clock_skew() {
        assert_eq!(parse_clock_skew_secs("30").unwrap(), 30);
        assert!(parse_clock_skew_secs("").is_err());
        assert!(parse_clock_skew_secs("-1").is_err());
        assert!(parse_clock_skew_secs("seconds").is_err());
    }
}
