use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::DatabasePool;

pub(crate) const RECONCILIATION_DIFFERENCE_WEBHOOK_COMMAND: &str =
    "reconciliation_difference_webhook";
pub(crate) const EXECUTION_ALERT_WEBHOOK_COMMAND: &str = "execution_alert_webhook";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationWebhookPayload {
    pub version: u8,
    pub event: String,
    pub account_id: String,
    pub exchange: String,
    pub reconciliation_run_id: Uuid,
    pub difference_count: i32,
    pub difference_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionAlertPayload {
    pub version: u8,
    pub event: String,
    pub task_id: Option<Uuid>,
    pub account_id: String,
    pub exchange: String,
    pub client_order_id: Option<String>,
    pub code: String,
}

#[derive(Debug, Clone)]
pub struct ClaimedReconciliationWebhook {
    pub id: Uuid,
    pub claim_token: Uuid,
    pub payload: Value,
    pub attempts: i32,
}

pub(crate) async fn enqueue_reconciliation_alert(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: &str,
    exchange: &str,
    run_id: Uuid,
    differences: &Value,
    difference_count: i32,
    difference_types: &[String],
) -> Result<bool, sqlx::Error> {
    if difference_count <= 0 || !differences.is_array() {
        return Ok(false);
    }

    let fingerprint = reconciliation_fingerprint(differences)?;
    let previous: Option<String> = sqlx::query_scalar(
        "SELECT fingerprint FROM reconciliation_alert_states WHERE account_id = $1 FOR UPDATE",
    )
    .bind(account_id)
    .fetch_optional(&mut **transaction)
    .await?;
    if previous.as_deref() == Some(fingerprint.as_str()) {
        return Ok(false);
    }

    let payload = serde_json::to_value(ReconciliationWebhookPayload {
        version: 1,
        event: "reconciliation_difference".to_owned(),
        account_id: account_id.to_owned(),
        exchange: exchange.to_owned(),
        reconciliation_run_id: run_id,
        difference_count,
        difference_types: difference_types.to_vec(),
    })
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO reconciliation_alert_states (account_id, fingerprint, last_run_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (account_id) DO UPDATE
        SET fingerprint = EXCLUDED.fingerprint,
            last_run_id = EXCLUDED.last_run_id,
            updated_at = now()
        "#,
    )
    .bind(account_id)
    .bind(&fingerprint)
    .bind(run_id)
    .execute(&mut **transaction)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO outbox_commands (
            id, command_type, payload, status, attempts, available_at
        ) VALUES ($1, $2, $3, 'pending', 0, now())
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(RECONCILIATION_DIFFERENCE_WEBHOOK_COMMAND)
    .bind(payload)
    .execute(&mut **transaction)
    .await?;

    Ok(true)
}

pub(crate) async fn enqueue_execution_alert(
    transaction: &mut Transaction<'_, Postgres>,
    task_id: Option<Uuid>,
    account_id: &str,
    exchange: &str,
    client_order_id: Option<&str>,
    event: &str,
    code: &str,
) -> Result<(), sqlx::Error> {
    if account_id.trim().is_empty()
        || exchange.trim().is_empty()
        || event.trim().is_empty()
        || code.trim().is_empty()
    {
        return Err(protocol_error("execution_alert_payload_invalid"));
    }
    let payload = serde_json::to_value(ExecutionAlertPayload {
        version: 1,
        event: event.to_owned(),
        task_id,
        account_id: account_id.to_owned(),
        exchange: exchange.to_owned(),
        client_order_id: client_order_id.map(str::to_owned),
        code: code.to_owned(),
    })
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    sqlx::query(
        r#"
        INSERT INTO outbox_commands (id, task_id, command_type, payload, status, attempts, available_at)
        VALUES ($1, $2, $3, $4, 'pending', 0, now())
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(task_id)
    .bind(EXECUTION_ALERT_WEBHOOK_COMMAND)
    .bind(payload)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn enqueue_system_alert(
    pool: &DatabasePool,
    event: &str,
    scope_id: &str,
    code: &str,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    enqueue_execution_alert(
        &mut transaction,
        None,
        scope_id,
        "system",
        None,
        event,
        code,
    )
    .await?;
    transaction.commit().await
}

pub(crate) async fn clear_reconciliation_alert_state(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM reconciliation_alert_states WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

pub async fn claim_reconciliation_webhooks(
    pool: &DatabasePool,
    limit: i64,
    claimed_until: DateTime<Utc>,
) -> Result<Vec<ClaimedReconciliationWebhook>, sqlx::Error> {
    if limit <= 0 {
        return Err(protocol_error("reconciliation_alert_claim_limit_invalid"));
    }
    let claim_token = Uuid::now_v7();
    let rows = sqlx::query(
        r#"
        WITH claimable AS (
            SELECT id
            FROM outbox_commands
            WHERE command_type = $1
              AND (
                  (status = 'pending' AND available_at <= now())
                  OR (
                      status = 'processing'
                      AND (claimed_until IS NULL OR claimed_until < now())
                  )
              )
            ORDER BY available_at, created_at, id
            LIMIT $2
            FOR UPDATE SKIP LOCKED
        )
        UPDATE outbox_commands AS command
        SET status = 'processing',
            attempts = command.attempts + 1,
            claim_token = $3,
            claimed_until = $4,
            updated_at = now()
        FROM claimable
        WHERE command.id = claimable.id
        RETURNING command.id, command.claim_token, command.payload, command.attempts
        "#,
    )
    .bind(RECONCILIATION_DIFFERENCE_WEBHOOK_COMMAND)
    .bind(limit)
    .bind(claim_token)
    .bind(claimed_until)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(ClaimedReconciliationWebhook {
                id: row.try_get("id")?,
                claim_token: row.try_get("claim_token")?,
                payload: row.try_get("payload")?,
                attempts: row.try_get("attempts")?,
            })
        })
        .collect()
}

pub async fn claim_execution_alerts(
    pool: &DatabasePool,
    limit: i64,
    claimed_until: DateTime<Utc>,
) -> Result<Vec<ClaimedReconciliationWebhook>, sqlx::Error> {
    if limit <= 0 {
        return Err(protocol_error("execution_alert_claim_limit_invalid"));
    }
    let claim_token = Uuid::now_v7();
    let rows = sqlx::query(
        r#"
        WITH claimable AS (
            SELECT id
            FROM outbox_commands
            WHERE command_type = $1
              AND (
                  (status = 'pending' AND available_at <= now())
                  OR (
                      status = 'processing'
                      AND (claimed_until IS NULL OR claimed_until < now())
                  )
              )
            ORDER BY available_at, created_at, id
            LIMIT $2
            FOR UPDATE SKIP LOCKED
        )
        UPDATE outbox_commands AS command
        SET status = 'processing',
            attempts = command.attempts + 1,
            claim_token = $3,
            claimed_until = $4,
            updated_at = now()
        FROM claimable
        WHERE command.id = claimable.id
        RETURNING command.id, command.claim_token, command.payload, command.attempts
        "#,
    )
    .bind(EXECUTION_ALERT_WEBHOOK_COMMAND)
    .bind(limit)
    .bind(claim_token)
    .bind(claimed_until)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(ClaimedReconciliationWebhook {
                id: row.try_get("id")?,
                claim_token: row.try_get("claim_token")?,
                payload: row.try_get("payload")?,
                attempts: row.try_get("attempts")?,
            })
        })
        .collect()
}

pub async fn complete_reconciliation_webhook(
    pool: &DatabasePool,
    command_id: Uuid,
    claim_token: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE outbox_commands
        SET status = 'completed',
            claim_token = NULL,
            claimed_until = NULL,
            last_error = NULL,
            updated_at = now()
        WHERE id = $1 AND status = 'processing' AND claim_token = $2
        "#,
    )
    .bind(command_id)
    .bind(claim_token)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn retry_reconciliation_webhook(
    pool: &DatabasePool,
    command_id: Uuid,
    claim_token: Uuid,
    available_at: DateTime<Utc>,
    error_code: &str,
) -> Result<bool, sqlx::Error> {
    validate_error_code(error_code)?;
    let result = sqlx::query(
        r#"
        UPDATE outbox_commands
        SET status = 'pending',
            available_at = $3,
            claim_token = NULL,
            claimed_until = NULL,
            last_error = $4,
            updated_at = now()
        WHERE id = $1 AND status = 'processing' AND claim_token = $2
        "#,
    )
    .bind(command_id)
    .bind(claim_token)
    .bind(available_at)
    .bind(error_code)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn fail_reconciliation_webhook(
    pool: &DatabasePool,
    command_id: Uuid,
    claim_token: Uuid,
    error_code: &str,
) -> Result<bool, sqlx::Error> {
    validate_error_code(error_code)?;
    let result = sqlx::query(
        r#"
        UPDATE outbox_commands
        SET status = 'failed',
            claim_token = NULL,
            claimed_until = NULL,
            last_error = $3,
            updated_at = now()
        WHERE id = $1 AND status = 'processing' AND claim_token = $2
        "#,
    )
    .bind(command_id)
    .bind(claim_token)
    .bind(error_code)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

fn reconciliation_fingerprint(differences: &Value) -> Result<String, sqlx::Error> {
    let bytes = serde_json::to_vec(differences)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_error_code(error_code: &str) -> Result<(), sqlx::Error> {
    if error_code.is_empty()
        || error_code.len() > 128
        || !error_code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(protocol_error("reconciliation_alert_error_code_invalid"));
    }
    Ok(())
}

fn protocol_error(message: &str) -> sqlx::Error {
    sqlx::Error::Protocol(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::validate_error_code;

    #[test]
    fn error_codes_are_safe_to_persist() {
        assert!(validate_error_code("webhook_http_503").is_ok());
        assert!(validate_error_code("url?secret=1").is_err());
        assert!(validate_error_code("WebhookFailed").is_err());
    }
}
