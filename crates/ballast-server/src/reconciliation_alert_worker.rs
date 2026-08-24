use std::time::Duration;

use ballast_storage::{
    ClaimedReconciliationWebhook, DatabasePool, claim_execution_alerts,
    claim_reconciliation_webhooks, complete_reconciliation_webhook, fail_reconciliation_webhook,
    retry_reconciliation_webhook,
};
use chrono::Utc;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, Url};
use serde_json::Value;
use thiserror::Error;
use tracing::{error, warn};

use crate::reconciliation_alert_config::ReconciliationAlertConfig;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CLAIM_DURATION_SECONDS: i64 = 30;
const CLAIM_BATCH_SIZE: i64 = 8;
const MAX_ATTEMPTS: i32 = 5;
const RETRY_BASE_SECONDS: i64 = 10;

pub fn spawn_worker(database: DatabasePool, config: ReconciliationAlertConfig) {
    tokio::spawn(async move {
        let mut client_builder = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none());
        if let Some(pem) = config.root_certificate_pem.as_deref() {
            match reqwest::Certificate::from_pem(pem) {
                Ok(certificate) => {
                    client_builder = client_builder.add_root_certificate(certificate);
                }
                Err(error) => {
                    error!(%error, "failed to initialize reconciliation alert webhook certificate");
                    return;
                }
            }
        }
        let client = match client_builder.build() {
            Ok(client) => client,
            Err(error) => {
                error!(%error, "failed to initialize reconciliation alert webhook client");
                return;
            }
        };
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        loop {
            interval.tick().await;
            let claimed_until = Utc::now() + chrono::Duration::seconds(CLAIM_DURATION_SECONDS);
            match claim_alerts(&database, claimed_until).await {
                Ok(commands) => {
                    futures_util::stream::iter(commands)
                        .for_each_concurrent(CLAIM_BATCH_SIZE as usize, |command| {
                            let database = database.clone();
                            let client = client.clone();
                            let webhook_url = config.webhook_url.clone();
                            async move {
                                deliver_one(&database, &client, &webhook_url, command).await;
                            }
                        })
                        .await;
                }
                Err(error) => error!(%error, "failed to claim reconciliation alert webhooks"),
            }
        }
    });
}

async fn claim_alerts(
    database: &DatabasePool,
    claimed_until: chrono::DateTime<Utc>,
) -> Result<Vec<ClaimedReconciliationWebhook>, sqlx::Error> {
    let mut commands =
        claim_reconciliation_webhooks(database, CLAIM_BATCH_SIZE, claimed_until).await?;
    commands.extend(claim_execution_alerts(database, CLAIM_BATCH_SIZE, claimed_until).await?);
    Ok(commands)
}

async fn deliver_one(
    database: &DatabasePool,
    client: &Client,
    webhook_url: &Url,
    command: ClaimedReconciliationWebhook,
) {
    match send_webhook(client, webhook_url, &command.payload).await {
        Ok(()) => {
            if let Err(error) =
                complete_reconciliation_webhook(database, command.id, command.claim_token).await
            {
                error!(%error, command_id = %command.id, "failed to complete reconciliation alert webhook");
            }
        }
        Err(DeliveryError::Permanent(code)) => {
            mark_failed(database, &command, code).await;
        }
        Err(DeliveryError::Retry(code)) if command.attempts >= MAX_ATTEMPTS => {
            mark_failed(database, &command, "webhook_retry_exhausted").await;
            warn!(
                command_id = %command.id,
                attempts = command.attempts,
                last_error = code,
                "reconciliation alert webhook retry limit reached"
            );
        }
        Err(DeliveryError::Retry(code)) => {
            let available_at = Utc::now() + retry_delay(command.attempts);
            match retry_reconciliation_webhook(
                database,
                command.id,
                command.claim_token,
                available_at,
                code,
            )
            .await
            {
                Ok(false) => {
                    warn!(command_id = %command.id, "reconciliation alert webhook claim was lost before retry")
                }
                Ok(true) => {}
                Err(error) => {
                    error!(%error, command_id = %command.id, "failed to schedule reconciliation alert webhook retry")
                }
            }
        }
    }
}

async fn mark_failed(
    database: &DatabasePool,
    command: &ClaimedReconciliationWebhook,
    error_code: &'static str,
) {
    match fail_reconciliation_webhook(database, command.id, command.claim_token, error_code).await {
        Ok(false) => {
            warn!(command_id = %command.id, "reconciliation alert webhook claim was lost before failure update")
        }
        Ok(true) => {}
        Err(error) => {
            error!(%error, command_id = %command.id, "failed to mark reconciliation alert webhook failed")
        }
    }
}

async fn send_webhook(
    client: &Client,
    webhook_url: &Url,
    payload: &Value,
) -> Result<(), DeliveryError> {
    validate_payload(payload)?;
    let response = client
        .post(webhook_url.clone())
        .json(payload)
        .send()
        .await
        .map_err(|_| DeliveryError::Retry("webhook_transport_failed"))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    if is_retryable_status(status) {
        return Err(DeliveryError::Retry(status_error_code(status)));
    }
    Err(DeliveryError::Permanent(status_error_code(status)))
}

fn validate_payload(payload: &Value) -> Result<(), DeliveryError> {
    let object = payload
        .as_object()
        .ok_or(DeliveryError::Permanent("webhook_payload_invalid"))?;
    if object.get("version").and_then(Value::as_u64) != Some(1)
        || object
            .get("event")
            .and_then(Value::as_str)
            .is_none_or(|value| value.is_empty())
    {
        return Err(DeliveryError::Permanent("webhook_payload_invalid"));
    }
    Ok(())
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_EARLY
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn status_error_code(status: StatusCode) -> &'static str {
    match status.as_u16() {
        400 => "webhook_http_400",
        401 => "webhook_http_401",
        403 => "webhook_http_403",
        404 => "webhook_http_404",
        408 => "webhook_http_408",
        409 => "webhook_http_409",
        410 => "webhook_http_410",
        425 => "webhook_http_425",
        429 => "webhook_http_429",
        500 => "webhook_http_500",
        502 => "webhook_http_502",
        503 => "webhook_http_503",
        504 => "webhook_http_504",
        _ if status.is_client_error() => "webhook_http_4xx",
        _ if status.is_server_error() => "webhook_http_5xx",
        _ => "webhook_http_unexpected_status",
    }
}

fn retry_delay(attempts: i32) -> chrono::Duration {
    let exponent = attempts.saturating_sub(1).clamp(0, 5) as u32;
    chrono::Duration::seconds(RETRY_BASE_SECONDS * 2_i64.pow(exponent))
}

#[derive(Debug, Error)]
enum DeliveryError {
    #[error("permanent webhook failure: {0}")]
    Permanent(&'static str),
    #[error("retryable webhook failure: {0}")]
    Retry(&'static str),
}

#[cfg(test)]
mod tests {
    use super::{is_retryable_status, retry_delay, status_error_code};
    use reqwest::StatusCode;

    #[test]
    fn retry_policy_only_retries_transient_responses() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::MOVED_PERMANENTLY));
        assert_eq!(
            status_error_code(StatusCode::SERVICE_UNAVAILABLE),
            "webhook_http_503"
        );
    }

    #[test]
    fn retry_delay_is_capped() {
        assert_eq!(retry_delay(1), chrono::Duration::seconds(10));
        assert_eq!(retry_delay(4), chrono::Duration::seconds(80));
        assert_eq!(retry_delay(99), chrono::Duration::seconds(320));
    }
}
