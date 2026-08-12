use std::time::Duration;

use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::api::{ApiError, ApiResult};

use super::{RequestAuth, Role, Session};

const TICKET_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct TicketService {
    database: ballast_storage::DatabasePool,
}

impl TicketService {
    pub fn new(database: ballast_storage::DatabasePool) -> Self {
        Self { database }
    }

    pub async fn issue(&self, subject: &str, roles: &[Role]) -> Result<IssuedTicket, ApiError> {
        let raw = format!("bws_{}", uuid::Uuid::now_v7());
        let ticket_hash = hash_ticket(&raw);
        let expires_at = Utc::now() + chrono::Duration::from_std(TICKET_TTL).expect("ttl");
        let role_values: Vec<String> = roles.iter().map(|role| role.as_str().to_owned()).collect();
        ballast_storage::create_ws_ticket(
            &self.database,
            &ticket_hash,
            subject,
            &role_values,
            expires_at,
        )
        .await
        .map_err(ApiError::database)?;
        Ok(IssuedTicket {
            ticket: raw,
            expires_at,
        })
    }

    pub async fn consume(&self, raw_ticket: &str) -> Result<super::Principal, ApiError> {
        let ticket_hash = hash_ticket(raw_ticket);
        let stored = ballast_storage::consume_ws_ticket(&self.database, &ticket_hash)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::unauthorized("ws_ticket_invalid"))?;
        let mut roles = Vec::new();
        for value in stored.roles {
            if let Some(role) = Role::parse(&value) {
                roles.push(role);
            }
        }
        if roles.is_empty() {
            return Err(ApiError::forbidden("role_claim_missing"));
        }
        Ok(super::Principal {
            subject: stored.subject,
            roles,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct IssuedTicket {
    pub ticket: String,
    pub expires_at: DateTime<Utc>,
}

fn hash_ticket(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    hex::encode(digest)
}

pub async fn issue_ws_ticket(
    State(state): State<AppState>,
    auth: RequestAuth,
) -> ApiResult<Json<IssuedTicket>> {
    auth.require_role(Role::Viewer)?;
    let (subject, roles) = match &auth.session {
        Session::Open => ("open-mode", vec![Role::Operator]),
        Session::User(principal) => (principal.subject.as_str(), principal.roles.clone()),
        Session::Anonymous => return Err(ApiError::unauthorized("missing_bearer_token")),
    };
    let issued = state.auth.tickets.issue(subject, &roles).await?;
    Ok(Json(issued))
}

#[cfg(test)]
mod tests {
    use super::hash_ticket;

    #[test]
    fn ticket_hash_is_stable() {
        assert_eq!(hash_ticket("abc"), hash_ticket("abc"));
        assert_ne!(hash_ticket("abc"), hash_ticket("abd"));
    }
}
