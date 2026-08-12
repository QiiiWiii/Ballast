use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::DatabasePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub id: String,
    pub exchange: String,
    pub label: String,
    pub environment: String,
    pub secret_name: String,
    pub enabled: bool,
    pub withdrawals_disabled: bool,
    pub ip_restricted: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTaskApproval {
    pub id: Uuid,
    pub task_id: Uuid,
    pub action: String,
    pub actor_id: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRiskDecision {
    pub sequence: i64,
    pub task_id: Option<Uuid>,
    pub account_id: Option<String>,
    pub stage: String,
    pub decision: String,
    pub code: String,
    pub inputs: Value,
    pub created_at: DateTime<Utc>,
}

pub async fn list_accounts(pool: &DatabasePool) -> Result<Vec<StoredAccount>, sqlx::Error> {
    let rows = sqlx::query("SELECT * FROM accounts ORDER BY exchange, lower(label)")
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_account).collect()
}

pub async fn list_task_approvals(
    pool: &DatabasePool,
    task_id: Option<Uuid>,
) -> Result<Vec<StoredTaskApproval>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM task_approvals
        WHERE $1::uuid IS NULL OR task_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_approval).collect()
}

pub async fn approve_task(
    pool: &DatabasePool,
    task_id: Uuid,
    actor_id: &str,
) -> Result<bool, sqlx::Error> {
    decide_task(pool, task_id, actor_id, "approved", None).await
}

pub async fn reject_task(
    pool: &DatabasePool,
    task_id: Uuid,
    actor_id: &str,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    decide_task(pool, task_id, actor_id, "rejected", Some(reason)).await
}

async fn decide_task(
    pool: &DatabasePool,
    task_id: Uuid,
    actor_id: &str,
    action: &'static str,
    reason: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let requester: Option<String> = sqlx::query_scalar(
        "SELECT requested_by FROM execution_tasks WHERE id = $1 AND status = 'pending_approval' FOR UPDATE",
    )
    .bind(task_id)
    .fetch_optional(&mut *transaction)
    .await?
    .flatten();
    let Some(requester) = requester else {
        transaction.rollback().await?;
        return Ok(false);
    };
    if requester == actor_id {
        return Err(sqlx::Error::Protocol("self_approval_forbidden".into()));
    }
    let next_status = if action == "approved" {
        "scheduled"
    } else {
        "rejected"
    };
    sqlx::query(
        r#"
        UPDATE execution_tasks
        SET status = $2,
            approved_by = CASE WHEN $2 = 'scheduled' THEN $3 ELSE approved_by END,
            approved_at = CASE WHEN $2 = 'scheduled' THEN now() ELSE approved_at END,
            rejected_by = CASE WHEN $2 = 'rejected' THEN $3 ELSE rejected_by END,
            rejected_at = CASE WHEN $2 = 'rejected' THEN now() ELSE rejected_at END,
            rejection_reason = CASE WHEN $2 = 'rejected' THEN $4 ELSE rejection_reason END,
            version = version + 1,
            updated_at = now()
        WHERE id = $1 AND status = 'pending_approval'
        "#,
    )
    .bind(task_id)
    .bind(next_status)
    .bind(actor_id)
    .bind(reason)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO task_approvals (id, task_id, action, actor_id, reason) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(task_id)
    .bind(action)
    .bind(actor_id)
    .bind(reason)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO execution_events (event_id, task_id, event_type, payload) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(task_id)
    .bind(if action == "approved" { "task_approved" } else { "task_rejected" })
    .bind(serde_json::json!({ "actor_id": actor_id, "reason": reason }))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(true)
}

pub async fn list_risk_decisions(
    pool: &DatabasePool,
    limit: i64,
) -> Result<Vec<StoredRiskDecision>, sqlx::Error> {
    let rows = sqlx::query("SELECT * FROM risk_decisions ORDER BY sequence DESC LIMIT $1")
        .bind(limit)
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_risk_decision).collect()
}

fn row_to_account(row: &sqlx::postgres::PgRow) -> Result<StoredAccount, sqlx::Error> {
    Ok(StoredAccount {
        id: row.try_get("id")?,
        exchange: row.try_get("exchange")?,
        label: row.try_get("label")?,
        environment: row.try_get("environment")?,
        secret_name: row.try_get("secret_name")?,
        enabled: row.try_get("enabled")?,
        withdrawals_disabled: row.try_get("withdrawals_disabled")?,
        ip_restricted: row.try_get("ip_restricted")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_approval(row: &sqlx::postgres::PgRow) -> Result<StoredTaskApproval, sqlx::Error> {
    Ok(StoredTaskApproval {
        id: row.try_get("id")?,
        task_id: row.try_get("task_id")?,
        action: row.try_get("action")?,
        actor_id: row.try_get("actor_id")?,
        reason: row.try_get("reason")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_risk_decision(row: &sqlx::postgres::PgRow) -> Result<StoredRiskDecision, sqlx::Error> {
    Ok(StoredRiskDecision {
        sequence: row.try_get("sequence")?,
        task_id: row.try_get("task_id")?,
        account_id: row.try_get("account_id")?,
        stage: row.try_get("stage")?,
        decision: row.try_get("decision")?,
        code: row.try_get("code")?,
        inputs: row.try_get("inputs")?,
        created_at: row.try_get("created_at")?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredWsTicket {
    pub subject: String,
    pub roles: Vec<String>,
    pub expires_at: DateTime<Utc>,
}

pub async fn create_ws_ticket(
    pool: &DatabasePool,
    ticket_hash: &str,
    subject: &str,
    roles: &[String],
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let roles_json =
        serde_json::to_value(roles).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    sqlx::query(
        r#"
        INSERT INTO ws_tickets (ticket_hash, subject, roles, expires_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(ticket_hash)
    .bind(subject)
    .bind(roles_json)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically consume a non-expired ticket. Returns None if missing, expired, or already used.
pub async fn consume_ws_ticket(
    pool: &DatabasePool,
    ticket_hash: &str,
) -> Result<Option<StoredWsTicket>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        UPDATE ws_tickets
        SET consumed_at = now()
        WHERE ticket_hash = $1
          AND consumed_at IS NULL
          AND expires_at > now()
        RETURNING subject, roles, expires_at
        "#,
    )
    .bind(ticket_hash)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let roles_value: Value = row.try_get("roles")?;
    let roles = match roles_value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        Value::String(text) => vec![text],
        _ => Vec::new(),
    };
    Ok(Some(StoredWsTicket {
        subject: row.try_get("subject")?,
        roles,
        expires_at: row.try_get("expires_at")?,
    }))
}
