use ballast_execution::{OrderLifecycle, OrderSnapshot, OrderState};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::str::FromStr;
use uuid::Uuid;

use crate::DatabasePool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChildOrder {
    pub task_id: Uuid,
    pub exchange: String,
    pub account_id: String,
    pub request_id: Uuid,
    pub client_order_id: String,
    pub side: String,
    pub order_type: String,
    pub order_backend: String,
    pub quantity: Decimal,
    pub limit_price: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredChildOrder {
    pub id: Uuid,
    pub task_id: Uuid,
    pub exchange: String,
    pub account_id: String,
    pub request_id: Uuid,
    pub client_order_id: String,
    pub exchange_order_id: Option<String>,
    pub side: String,
    pub order_type: String,
    pub order_backend: String,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub limit_price: Option<Decimal>,
    pub status: OrderState,
    pub raw_status: Option<String>,
    pub state_reason: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub last_reconciled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildOrderStateUpdate {
    pub status: OrderState,
    pub exchange_order_id: Option<String>,
    pub filled_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub limit_price: Option<Decimal>,
    pub raw_status: Option<String>,
    pub state_reason: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub last_reconciled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedChildOrderReconciliation {
    pub order: StoredChildOrder,
    pub account_environment: String,
    pub instrument_id: Uuid,
    pub claim_token: Uuid,
    pub failure_count: i32,
}

pub async fn create_or_get_child_order(
    pool: &DatabasePool,
    new_order: &NewChildOrder,
) -> Result<StoredChildOrder, sqlx::Error> {
    validate_new_order(new_order)?;
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        r#"
        INSERT INTO child_orders (
            id, task_id, exchange, account_id, request_id, client_order_id,
            side, order_type, order_backend, quantity, limit_price, status
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'submission_pending'
        )
        ON CONFLICT (exchange, account_id, client_order_id) DO NOTHING
        RETURNING *
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(new_order.task_id)
    .bind(&new_order.exchange)
    .bind(&new_order.account_id)
    .bind(new_order.request_id)
    .bind(&new_order.client_order_id)
    .bind(&new_order.side)
    .bind(&new_order.order_type)
    .bind(&new_order.order_backend)
    .bind(new_order.quantity)
    .bind(new_order.limit_price)
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(row) = inserted {
        let order = row_to_child_order(&row)?;
        sqlx::query(
            r#"
            INSERT INTO execution_events (event_id, task_id, event_type, payload)
            VALUES ($1, $2, 'child_order_created', $3)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(order.task_id)
        .bind(serde_json::json!({
            "child_order_id": order.id,
            "exchange": order.exchange,
            "account_id": order.account_id,
            "request_id": order.request_id,
            "client_order_id": order.client_order_id,
            "side": order.side,
            "order_type": order.order_type,
            "order_backend": order.order_backend,
            "quantity": decimal_value_text(order.quantity),
            "limit_price": decimal_text(order.limit_price),
            "status": order.status.as_str(),
        }))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        return Ok(order);
    }

    let row = sqlx::query(
        r#"
        SELECT *
        FROM child_orders
        WHERE exchange = $1 AND account_id = $2 AND client_order_id = $3
        FOR UPDATE
        "#,
    )
    .bind(&new_order.exchange)
    .bind(&new_order.account_id)
    .bind(&new_order.client_order_id)
    .fetch_one(&mut *transaction)
    .await?;
    let order = row_to_child_order(&row)?;
    if !same_intent(&order, new_order) {
        return Err(protocol_error("child_order_intent_conflict"));
    }
    transaction.commit().await?;
    Ok(order)
}

fn validate_new_order(order: &NewChildOrder) -> Result<(), sqlx::Error> {
    if order.exchange.trim().is_empty() || order.account_id.trim().is_empty() {
        return Err(protocol_error("child_order_scope_invalid"));
    }
    if order.client_order_id.len() != 32
        || !order
            .client_order_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(protocol_error("child_order_client_order_id_invalid"));
    }
    if order.quantity <= Decimal::ZERO {
        return Err(protocol_error("child_order_quantity_invalid"));
    }
    if order
        .limit_price
        .is_some_and(|price| price <= Decimal::ZERO)
    {
        return Err(protocol_error("child_order_limit_price_invalid"));
    }
    Ok(())
}

pub async fn get_child_order_by_client_order_id(
    pool: &DatabasePool,
    exchange: &str,
    account_id: &str,
    client_order_id: &str,
) -> Result<Option<StoredChildOrder>, sqlx::Error> {
    sqlx::query(
        r#"
        SELECT *
        FROM child_orders
        WHERE exchange = $1 AND account_id = $2 AND client_order_id = $3
        "#,
    )
    .bind(exchange)
    .bind(account_id)
    .bind(client_order_id)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(row_to_child_order)
    .transpose()
}

pub async fn claim_child_orders_for_reconciliation(
    pool: &DatabasePool,
    limit: i64,
    claimed_until: DateTime<Utc>,
) -> Result<Vec<ClaimedChildOrderReconciliation>, sqlx::Error> {
    if limit <= 0 {
        return Err(protocol_error("child_order_reconciliation_limit_invalid"));
    }
    let claim_token = Uuid::now_v7();
    let rows = sqlx::query(
        r#"
        WITH candidates AS (
            SELECT child_orders.id
            FROM child_orders
            JOIN accounts ON accounts.id = child_orders.account_id
            WHERE child_orders.status IN (
                    'submission_unknown', 'open', 'partially_filled', 'cancel_pending'
                )
              AND child_orders.reconciliation_due_at <= now()
              AND (
                    child_orders.reconciliation_claimed_until IS NULL
                    OR child_orders.reconciliation_claimed_until < now()
                )
              AND accounts.enabled
              AND accounts.exchange = 'okx'
              AND accounts.exchange = child_orders.exchange
            ORDER BY child_orders.reconciliation_due_at, child_orders.created_at
            LIMIT $1
            FOR UPDATE OF child_orders SKIP LOCKED
        ), claimed AS (
            UPDATE child_orders
            SET reconciliation_claim_token = $2,
                reconciliation_claimed_until = $3
            FROM candidates
            WHERE child_orders.id = candidates.id
            RETURNING child_orders.*
        )
        SELECT claimed.*, accounts.environment AS account_environment,
               execution_tasks.instrument_id
        FROM claimed
        JOIN accounts ON accounts.id = claimed.account_id
        JOIN execution_tasks ON execution_tasks.id = claimed.task_id
        ORDER BY claimed.reconciliation_due_at, claimed.created_at
        "#,
    )
    .bind(limit)
    .bind(claim_token)
    .bind(claimed_until)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(ClaimedChildOrderReconciliation {
                order: row_to_child_order(row)?,
                account_environment: row.try_get("account_environment")?,
                instrument_id: row.try_get("instrument_id")?,
                claim_token,
                failure_count: row.try_get("reconciliation_failure_count")?,
            })
        })
        .collect()
}

pub async fn complete_child_order_reconciliation(
    pool: &DatabasePool,
    order_id: Uuid,
    claim_token: Uuid,
    next_reconciliation_at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE child_orders
        SET reconciliation_due_at = CASE
                WHEN status IN ('filled', 'cancelled', 'rejected', 'expired', 'failed')
                    THEN 'infinity'::timestamptz
                ELSE $3
            END,
            reconciliation_claim_token = NULL,
            reconciliation_claimed_until = NULL,
            reconciliation_failure_count = 0,
            last_reconciliation_error = NULL
        WHERE id = $1 AND reconciliation_claim_token = $2
        "#,
    )
    .bind(order_id)
    .bind(claim_token)
    .bind(next_reconciliation_at)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn fail_child_order_reconciliation(
    pool: &DatabasePool,
    order_id: Uuid,
    claim_token: Uuid,
    retry_at: DateTime<Utc>,
    failure_code: &str,
) -> Result<bool, sqlx::Error> {
    if failure_code.trim().is_empty() {
        return Err(protocol_error(
            "child_order_reconciliation_failure_code_invalid",
        ));
    }
    let result = sqlx::query(
        r#"
        UPDATE child_orders
        SET reconciliation_due_at = $3,
            reconciliation_claim_token = NULL,
            reconciliation_claimed_until = NULL,
            reconciliation_failure_count = reconciliation_failure_count + 1,
            last_reconciliation_error = $4
        WHERE id = $1 AND reconciliation_claim_token = $2
        "#,
    )
    .bind(order_id)
    .bind(claim_token)
    .bind(retry_at)
    .bind(failure_code)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn suspend_child_order_reconciliation(
    pool: &DatabasePool,
    order_id: Uuid,
    claim_token: Uuid,
    failure_code: &str,
) -> Result<bool, sqlx::Error> {
    if failure_code.trim().is_empty() {
        return Err(protocol_error(
            "child_order_reconciliation_failure_code_invalid",
        ));
    }
    let result = sqlx::query(
        r#"
        UPDATE child_orders
        SET reconciliation_due_at = 'infinity'::timestamptz,
            reconciliation_claim_token = NULL,
            reconciliation_claimed_until = NULL,
            reconciliation_failure_count = reconciliation_failure_count + 1,
            last_reconciliation_error = $3
        WHERE id = $1 AND reconciliation_claim_token = $2
        "#,
    )
    .bind(order_id)
    .bind(claim_token)
    .bind(failure_code)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn compare_and_set_child_order_state(
    pool: &DatabasePool,
    exchange: &str,
    account_id: &str,
    client_order_id: &str,
    allowed_source_statuses: &[OrderState],
    update: &ChildOrderStateUpdate,
) -> Result<StoredChildOrder, sqlx::Error> {
    if allowed_source_statuses.is_empty() {
        return Err(protocol_error("child_order_allowed_source_statuses_empty"));
    }
    validate_state_update(update)?;
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT *
        FROM child_orders
        WHERE exchange = $1 AND account_id = $2 AND client_order_id = $3
        FOR UPDATE
        "#,
    )
    .bind(exchange)
    .bind(account_id)
    .bind(client_order_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(row) = row else {
        return Err(sqlx::Error::RowNotFound);
    };
    let current = row_to_child_order(&row)?;

    if !allowed_source_statuses.contains(&current.status) {
        return Err(protocol_error("child_order_compare_and_set_rejected"));
    }
    let mut lifecycle = OrderLifecycle {
        client_order_id: current.client_order_id.clone(),
        state: current.status,
        quantity: current.quantity,
        filled_quantity: current.filled_quantity,
        exchange_order_id: current.exchange_order_id.clone(),
    };
    lifecycle
        .observe(&OrderSnapshot {
            client_order_id: current.client_order_id.clone(),
            exchange_order_id: update.exchange_order_id.clone(),
            state: update.status,
            filled_quantity: update.filled_quantity,
            average_price: update.average_price,
            observed_at: update.last_reconciled_at.unwrap_or_else(Utc::now),
        })
        .map_err(|_| protocol_error("child_order_state_transition_invalid"))?;
    if let (Some(current_price), Some(observed_price)) = (current.limit_price, update.limit_price)
        && current_price != observed_price
    {
        return Err(protocol_error("child_order_limit_price_conflict"));
    }
    if let (Some(current_time), Some(observed_time)) = (current.submitted_at, update.submitted_at)
        && current_time.timestamp_micros() != observed_time.timestamp_micros()
    {
        return Err(protocol_error("child_order_submitted_at_conflict"));
    }
    if let (Some(current_time), Some(observed_time)) =
        (current.last_reconciled_at, update.last_reconciled_at)
        && observed_time.timestamp_micros() < current_time.timestamp_micros()
    {
        return Err(protocol_error("child_order_observation_stale"));
    }

    let exchange_order_id = lifecycle.exchange_order_id;
    let average_price = update.average_price.or(current.average_price);
    let limit_price = current.limit_price.or(update.limit_price);
    let submitted_at = current.submitted_at.or(update.submitted_at);
    let last_reconciled_at = update.last_reconciled_at.or(current.last_reconciled_at);
    let material_change = current.status != lifecycle.state
        || current.exchange_order_id != exchange_order_id
        || current.filled_quantity != update.filled_quantity
        || current.average_price != average_price
        || current.limit_price != limit_price
        || current.raw_status != update.raw_status
        || current.state_reason != update.state_reason
        || current.submitted_at != submitted_at;
    let reconciliation_time_changed =
        !same_optional_timestamp_micros(current.last_reconciled_at, last_reconciled_at);

    if material_change
        && current.last_reconciled_at.is_some()
        && same_optional_timestamp_micros(current.last_reconciled_at, last_reconciled_at)
    {
        return Err(protocol_error("child_order_observation_conflict"));
    }

    if !material_change && !reconciliation_time_changed {
        transaction.commit().await?;
        return Ok(current);
    }

    let row = sqlx::query(
        r#"
        UPDATE child_orders
        SET exchange_order_id = $2,
            filled_quantity = $3,
            average_price = $4,
            limit_price = $5,
            status = $6,
            raw_status = $7,
            state_reason = $8,
            submitted_at = $9,
            last_reconciled_at = $10,
            updated_at = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(current.id)
    .bind(&exchange_order_id)
    .bind(update.filled_quantity)
    .bind(average_price)
    .bind(limit_price)
    .bind(lifecycle.state.as_str())
    .bind(&update.raw_status)
    .bind(&update.state_reason)
    .bind(submitted_at)
    .bind(last_reconciled_at)
    .fetch_one(&mut *transaction)
    .await?;
    let stored = row_to_child_order(&row)?;

    if material_change {
        sqlx::query(
            r#"
            INSERT INTO execution_events (event_id, task_id, event_type, payload)
            VALUES ($1, $2, 'child_order_state_changed', $3)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(stored.task_id)
        .bind(serde_json::json!({
            "child_order_id": stored.id,
            "exchange": stored.exchange,
            "account_id": stored.account_id,
            "client_order_id": stored.client_order_id,
            "previous": audit_state(&current),
            "current": audit_state(&stored),
        }))
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;
    Ok(stored)
}

fn validate_state_update(update: &ChildOrderStateUpdate) -> Result<(), sqlx::Error> {
    if update
        .average_price
        .is_some_and(|average_price| average_price <= Decimal::ZERO)
    {
        return Err(protocol_error("child_order_average_price_invalid"));
    }
    if update.status == OrderState::SubmissionUnknown
        && (update
            .state_reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
            || update.last_reconciled_at.is_none())
    {
        return Err(protocol_error("child_order_submission_unknown_incomplete"));
    }
    if matches!(
        update.status,
        OrderState::Open
            | OrderState::PartiallyFilled
            | OrderState::Filled
            | OrderState::Cancelled
            | OrderState::Rejected
            | OrderState::Expired
            | OrderState::Failed
    ) && update.last_reconciled_at.is_none()
    {
        return Err(protocol_error("child_order_observation_time_required"));
    }
    Ok(())
}

fn same_intent(stored: &StoredChildOrder, new_order: &NewChildOrder) -> bool {
    stored.task_id == new_order.task_id
        && stored.exchange == new_order.exchange
        && stored.account_id == new_order.account_id
        && stored.client_order_id == new_order.client_order_id
        && stored.side == new_order.side
        && stored.order_type == new_order.order_type
        && stored.order_backend == new_order.order_backend
        && stored.quantity == new_order.quantity
        && stored.limit_price == new_order.limit_price
}

fn audit_state(order: &StoredChildOrder) -> serde_json::Value {
    serde_json::json!({
        "status": order.status.as_str(),
        "exchange_order_id": order.exchange_order_id,
        "filled_quantity": decimal_value_text(order.filled_quantity),
        "average_price": decimal_text(order.average_price),
        "limit_price": decimal_text(order.limit_price),
        "raw_status": order.raw_status,
        "state_reason": order.state_reason,
        "submitted_at": order.submitted_at,
        "last_reconciled_at": order.last_reconciled_at,
    })
}

fn decimal_text(value: Option<Decimal>) -> Option<String> {
    value.map(decimal_value_text)
}

fn decimal_value_text(value: Decimal) -> String {
    value.normalize().to_string()
}

fn same_optional_timestamp_micros(
    left: Option<DateTime<Utc>>,
    right: Option<DateTime<Utc>>,
) -> bool {
    left.map(|value| value.timestamp_micros()) == right.map(|value| value.timestamp_micros())
}

fn row_to_child_order(row: &sqlx::postgres::PgRow) -> Result<StoredChildOrder, sqlx::Error> {
    let status: String = row.try_get("status")?;
    Ok(StoredChildOrder {
        id: row.try_get("id")?,
        task_id: row.try_get("task_id")?,
        exchange: row.try_get("exchange")?,
        account_id: row.try_get("account_id")?,
        request_id: row.try_get("request_id")?,
        client_order_id: row.try_get("client_order_id")?,
        exchange_order_id: row.try_get("exchange_order_id")?,
        side: row.try_get("side")?,
        order_type: row.try_get("order_type")?,
        order_backend: row.try_get("order_backend")?,
        quantity: row.try_get("quantity")?,
        filled_quantity: row.try_get("filled_quantity")?,
        average_price: row.try_get("average_price")?,
        limit_price: row.try_get("limit_price")?,
        status: OrderState::from_str(&status)
            .map_err(|_| protocol_error("child_order_status_invalid"))?,
        raw_status: row.try_get("raw_status")?,
        state_reason: row.try_get("state_reason")?,
        submitted_at: row.try_get("submitted_at")?,
        last_reconciled_at: row.try_get("last_reconciled_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn protocol_error(message: &'static str) -> sqlx::Error {
    sqlx::Error::Protocol(message.into())
}
