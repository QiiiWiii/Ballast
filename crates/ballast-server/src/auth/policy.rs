use axum::http::{Method, Uri};

use crate::api::ApiError;

use super::Principal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// Paper/open deployment: no OIDC configured.
    Open,
    /// Issuer+audience configured; API requires Bearer or WS ticket.
    Oidc,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Operator,
    Admin,
}

impl Role {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "viewer" | "ballast_viewer" => Some(Self::Viewer),
            "operator" | "ballast_operator" => Some(Self::Operator),
            "admin" | "ballast_admin" => Some(Self::Admin),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Admin => "admin",
        }
    }
}

pub fn role_rank(role: Role) -> u8 {
    match role {
        Role::Viewer => 1,
        Role::Operator => 2,
        Role::Admin => 3,
    }
}

#[derive(Debug, Clone)]
pub enum Session {
    /// Paper/open deployment: authentication is disabled by configuration.
    Open,
    /// Public route in an OIDC deployment without an optional identity.
    Anonymous,
    User(Principal),
}

/// Classify whether a request is public, and the minimum role when auth is on.
pub fn authorize_request(
    mode: AuthMode,
    method: &Method,
    uri: &Uri,
) -> Result<Option<Role>, ApiError> {
    let path = uri.path();
    if is_public(method, path) {
        return Ok(None);
    }
    if mode == AuthMode::Open {
        return Ok(None);
    }
    Ok(Some(required_role(method, path)))
}

fn is_public(method: &Method, path: &str) -> bool {
    matches!(
        (method, path),
        (&Method::GET, "/health")
            | (&Method::GET, "/metrics")
            | (&Method::GET, "/api/v1/live/readiness")
    )
}

fn required_role(method: &Method, path: &str) -> Role {
    if method == Method::GET || method == Method::HEAD {
        // WebSocket upgrade is GET /api/v1/ws — ticket checked separately.
        return Role::Viewer;
    }
    // Mutations
    if path.starts_with("/api/v1/approvals") || path.starts_with("/api/v1/risk") {
        return Role::Admin;
    }
    if path == "/api/v1/ws-tickets" {
        return Role::Viewer;
    }
    Role::Operator
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Uri;

    #[test]
    fn open_mode_skips_roles() {
        let uri: Uri = "/api/v1/tasks".parse().unwrap();
        assert_eq!(
            authorize_request(AuthMode::Open, &Method::POST, &uri).unwrap(),
            None
        );
    }

    #[test]
    fn oidc_write_requires_operator() {
        let uri: Uri = "/api/v1/tasks".parse().unwrap();
        assert_eq!(
            authorize_request(AuthMode::Oidc, &Method::POST, &uri).unwrap(),
            Some(Role::Operator)
        );
    }

    #[test]
    fn oidc_read_requires_viewer() {
        let uri: Uri = "/api/v1/tasks".parse().unwrap();
        assert_eq!(
            authorize_request(AuthMode::Oidc, &Method::GET, &uri).unwrap(),
            Some(Role::Viewer)
        );
    }

    #[test]
    fn health_is_public() {
        let uri: Uri = "/health".parse().unwrap();
        assert_eq!(
            authorize_request(AuthMode::Oidc, &Method::GET, &uri).unwrap(),
            None
        );
    }

    #[test]
    fn role_hierarchy() {
        let principal = Principal {
            subject: "u1".into(),
            roles: vec![Role::Operator],
        };
        assert!(principal.has_at_least(Role::Viewer));
        assert!(principal.has_at_least(Role::Operator));
        assert!(!principal.has_at_least(Role::Admin));
    }
}
