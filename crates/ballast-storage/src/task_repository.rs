use ballast_core::{QuantityUnit, Side, StrategyKind};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::DatabasePool;

#[derive(Debug, Clone)]
pub struct NewExecutionTask {
    pub instrument_id: Uuid,
    pub template_version_id: Uuid,
    pub idempotency_key: String,
    pub side: Side,
    pub strategy_kind: StrategyKind,
    pub strategy_params: Value,
    pub requested_amount: Decimal,
    pub quantity_unit: QuantityUnit,
    pub max_slippage_bps: i32,
    pub slice_interval_ms: i64,
    pub start_at: DateTime<Utc>,
    pub deadline_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredExecutionTask {
    pub id: Uuid,
    pub account_id: String,
    pub execution_mode: String,
    pub execution_backend: String,
    pub instrument_id: Uuid,
    pub template_version_id: Uuid,
    pub side: String,
    pub strategy_kind: String,
    pub strategy_params: Value,
    pub requested_amount: Decimal,
    pub quantity_unit: String,
    pub executed_amount: Decimal,
    pub residual_amount: Decimal,
    pub max_slippage_bps: i32,
    pub slice_interval_ms: i64,
    pub status: String,
    pub paused_reason: Option<String>,
    pub failure_code: Option<String>,
    pub requested_by: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejected_by: Option<String>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub deadline_at: DateTime<Utc>,
    pub next_tick_at: DateTime<Utc>,
    pub last_tick_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SliceRecord {
    pub task_id: Uuid,
    pub sequence: i32,
    pub requested_amount: Decimal,
    pub native_quantity: Decimal,
    pub filled_native_quantity: Decimal,
    pub filled_base_quantity: Decimal,
    pub filled_quote_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub worst_price: Option<Decimal>,
    pub slippage_bps: Option<Decimal>,
    pub fee_amount: Option<Decimal>,
    pub fee_asset: Option<String>,
    pub fee_status: &'static str,
    pub status: &'static str,
    pub market_snapshot: Value,
    pub decision_input: Value,
    pub executed_amount_delta: Decimal,
    pub residual_amount: Decimal,
    pub next_tick_at: DateTime<Utc>,
    pub task_status: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredExecutionSlice {
    pub id: Uuid,
    pub task_id: Uuid,
    pub sequence: i32,
    pub requested_amount: Decimal,
    pub native_quantity: Decimal,
    pub filled_native_quantity: Decimal,
    pub filled_base_quantity: Decimal,
    pub filled_quote_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub worst_price: Option<Decimal>,
    pub slippage_bps: Option<Decimal>,
    pub fee_amount: Option<Decimal>,
    pub fee_asset: Option<String>,
    pub fee_status: String,
    pub status: String,
    pub market_snapshot: Value,
    pub decision_input: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredExecutionEvent {
    pub sequence: i64,
    pub event_id: Uuid,
    pub task_id: Option<Uuid>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

pub async fn create_task(
    pool: &DatabasePool,
    new_task: NewExecutionTask,
) -> Result<StoredExecutionTask, sqlx::Error> {
    let id = Uuid::now_v7();
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        r#"
        INSERT INTO execution_tasks (
            id, account_id, instrument_id, template_version_id, idempotency_key, side, strategy_kind,
            strategy_params, status, version, deadline_at, next_tick_at,
            execution_mode, requested_amount, quantity_unit, residual_amount,
            max_slippage_bps, slice_interval_ms
        ) VALUES (
            $1, 'paper', $2, $3, $4, $5, $6, $7, 'scheduled', 0, $8, $9,
            'paper', $10, $11, $10, $12, $13
        )
        ON CONFLICT (idempotency_key) DO NOTHING
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(new_task.instrument_id)
    .bind(new_task.template_version_id)
    .bind(&new_task.idempotency_key)
    .bind(side_text(new_task.side))
    .bind(strategy_text(new_task.strategy_kind))
    .bind(new_task.strategy_params)
    .bind(new_task.deadline_at)
    .bind(new_task.start_at)
    .bind(new_task.requested_amount)
    .bind(quantity_unit_text(new_task.quantity_unit))
    .bind(new_task.max_slippage_bps)
    .bind(new_task.slice_interval_ms)
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(row) = inserted {
        let task = row_to_task(&row)?;
        sqlx::query(
            r#"
            INSERT INTO execution_events (event_id, task_id, event_type, payload)
            VALUES ($1, $2, 'task_created', $3)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(task.id)
        .bind(serde_json::json!({ "status": task.status }))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        return Ok(task);
    }
    let row = sqlx::query("SELECT * FROM execution_tasks WHERE idempotency_key = $1")
        .bind(new_task.idempotency_key)
        .fetch_one(&mut *transaction)
        .await?;
    let task = row_to_task(&row)?;
    transaction.commit().await?;
    Ok(task)
}

pub async fn get_task(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredExecutionTask>, sqlx::Error> {
    sqlx::query("SELECT * FROM execution_tasks WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_task)
        .transpose()
}

pub async fn list_tasks(
    pool: &DatabasePool,
    limit: i64,
) -> Result<Vec<StoredExecutionTask>, sqlx::Error> {
    sqlx::query("SELECT * FROM execution_tasks ORDER BY created_at DESC LIMIT $1")
        .bind(limit)
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_task)
        .collect()
}

pub async fn list_runnable_tasks(
    pool: &DatabasePool,
    limit: i64,
) -> Result<Vec<StoredExecutionTask>, sqlx::Error> {
    sqlx::query(
        r#"
        SELECT * FROM execution_tasks
        WHERE status IN ('scheduled', 'running', 'paused')
          AND next_tick_at <= now()
        ORDER BY next_tick_at, created_at
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?
    .iter()
    .map(row_to_task)
    .collect()
}

pub async fn claim_runnable_tasks(
    pool: &DatabasePool,
    limit: i64,
    lease_until: DateTime<Utc>,
) -> Result<Vec<StoredExecutionTask>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        WITH due AS (
            SELECT id
            FROM execution_tasks
            WHERE status IN ('scheduled', 'running', 'paused')
              AND next_tick_at <= now()
            ORDER BY next_tick_at, created_at
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE execution_tasks AS task
        SET next_tick_at = $2, updated_at = now()
        FROM due
        WHERE task.id = due.id
        RETURNING task.*
        "#,
    )
    .bind(limit)
    .bind(lease_until)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_task).collect()
}

pub async fn mark_task_state(
    pool: &DatabasePool,
    id: Uuid,
    status: &str,
    reason: Option<&str>,
    next_tick_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let updated = sqlx::query(
        r#"
        UPDATE execution_tasks
        SET status = $2, paused_reason = $3, next_tick_at = $4,
            started_at = CASE WHEN $2 = 'running' THEN COALESCE(started_at, now()) ELSE started_at END,
            version = version + 1, updated_at = now()
        WHERE id = $1
          AND status IN ('scheduled', 'running', 'paused', 'cancelling')
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(reason)
    .bind(next_tick_at)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() > 0 {
        sqlx::query(
            r#"
            INSERT INTO execution_events (event_id, task_id, event_type, payload)
            VALUES ($1, $2, 'task_state_changed', $3)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(serde_json::json!({ "status": status, "reason": reason }))
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn cancel_task(pool: &DatabasePool, id: Uuid) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE execution_tasks
        SET status = 'cancelled', version = version + 1, updated_at = now()
        WHERE id = $1 AND status IN ('scheduled', 'running', 'paused', 'cancelling')
        "#,
    )
    .bind(id)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() > 0 {
        sqlx::query(
            r#"
            INSERT INTO execution_events (event_id, task_id, event_type, payload)
            VALUES ($1, $2, 'task_state_changed', '{"status":"cancelled"}'::jsonb)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_slices(
    pool: &DatabasePool,
    task_id: Uuid,
) -> Result<Vec<StoredExecutionSlice>, sqlx::Error> {
    let rows = sqlx::query("SELECT * FROM execution_slices WHERE task_id = $1 ORDER BY sequence")
        .bind(task_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_slice).collect()
}

pub async fn next_slice_sequence(pool: &DatabasePool, task_id: Uuid) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence), 0)::integer + 1 FROM execution_slices WHERE task_id = $1",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
}

pub async fn list_events_after(
    pool: &DatabasePool,
    after_sequence: i64,
    limit: i64,
) -> Result<Vec<StoredExecutionEvent>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM execution_events
        WHERE sequence > $1
        ORDER BY sequence
        LIMIT $2
        "#,
    )
    .bind(after_sequence)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_event).collect()
}

pub async fn record_slice(pool: &DatabasePool, slice: SliceRecord) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let slice_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO execution_slices (
            id, task_id, sequence, requested_amount, native_quantity,
            filled_native_quantity, filled_base_quantity, filled_quote_quantity,
            average_price, worst_price, slippage_bps, fee_amount, fee_asset,
            fee_status, status, market_snapshot, decision_input
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17
        )
        "#,
    )
    .bind(slice_id)
    .bind(slice.task_id)
    .bind(slice.sequence)
    .bind(slice.requested_amount)
    .bind(slice.native_quantity)
    .bind(slice.filled_native_quantity)
    .bind(slice.filled_base_quantity)
    .bind(slice.filled_quote_quantity)
    .bind(slice.average_price)
    .bind(slice.worst_price)
    .bind(slice.slippage_bps)
    .bind(slice.fee_amount)
    .bind(&slice.fee_asset)
    .bind(slice.fee_status)
    .bind(slice.status)
    .bind(&slice.market_snapshot)
    .bind(&slice.decision_input)
    .execute(&mut *transaction)
    .await?;
    let updated = sqlx::query(
        r#"
        UPDATE execution_tasks
        SET executed_amount = executed_amount + $2, residual_amount = $3,
            status = $4, next_tick_at = $5, last_tick_at = now(),
            started_at = COALESCE(started_at, now()),
            version = version + 1, updated_at = now()
        WHERE id = $1
          AND status IN ('scheduled', 'running', 'paused')
        "#,
    )
    .bind(slice.task_id)
    .bind(slice.executed_amount_delta)
    .bind(slice.residual_amount)
    .bind(slice.task_status)
    .bind(slice.next_tick_at)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    sqlx::query(
        r#"
        INSERT INTO execution_events (event_id, task_id, event_type, payload)
        VALUES ($1, $2, 'slice_recorded', $3)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(slice.task_id)
    .bind(serde_json::json!({
        "slice_id": slice_id,
        "sequence": slice.sequence,
        "status": slice.status,
    }))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

fn row_to_task(row: &sqlx::postgres::PgRow) -> Result<StoredExecutionTask, sqlx::Error> {
    Ok(StoredExecutionTask {
        id: row.try_get("id")?,
        account_id: row.try_get("account_id")?,
        execution_mode: row.try_get("execution_mode")?,
        execution_backend: row.try_get("execution_backend")?,
        instrument_id: row.try_get("instrument_id")?,
        template_version_id: row.try_get("template_version_id")?,
        side: row.try_get("side")?,
        strategy_kind: row.try_get("strategy_kind")?,
        strategy_params: row.try_get("strategy_params")?,
        requested_amount: row.try_get("requested_amount")?,
        quantity_unit: row.try_get("quantity_unit")?,
        executed_amount: row.try_get("executed_amount")?,
        residual_amount: row.try_get("residual_amount")?,
        max_slippage_bps: row.try_get("max_slippage_bps")?,
        slice_interval_ms: row.try_get("slice_interval_ms")?,
        status: row.try_get("status")?,
        paused_reason: row.try_get("paused_reason")?,
        failure_code: row.try_get("failure_code")?,
        requested_by: row.try_get("requested_by")?,
        approved_by: row.try_get("approved_by")?,
        approved_at: row.try_get("approved_at")?,
        rejected_by: row.try_get("rejected_by")?,
        rejected_at: row.try_get("rejected_at")?,
        rejection_reason: row.try_get("rejection_reason")?,
        started_at: row.try_get("started_at")?,
        deadline_at: row.try_get("deadline_at")?,
        next_tick_at: row.try_get("next_tick_at")?,
        last_tick_at: row.try_get("last_tick_at")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_slice(row: &sqlx::postgres::PgRow) -> Result<StoredExecutionSlice, sqlx::Error> {
    Ok(StoredExecutionSlice {
        id: row.try_get("id")?,
        task_id: row.try_get("task_id")?,
        sequence: row.try_get("sequence")?,
        requested_amount: row.try_get("requested_amount")?,
        native_quantity: row.try_get("native_quantity")?,
        filled_native_quantity: row.try_get("filled_native_quantity")?,
        filled_base_quantity: row.try_get("filled_base_quantity")?,
        filled_quote_quantity: row.try_get("filled_quote_quantity")?,
        average_price: row.try_get("average_price")?,
        worst_price: row.try_get("worst_price")?,
        slippage_bps: row.try_get("slippage_bps")?,
        fee_amount: row.try_get("fee_amount")?,
        fee_asset: row.try_get("fee_asset")?,
        fee_status: row.try_get("fee_status")?,
        status: row.try_get("status")?,
        market_snapshot: row.try_get("market_snapshot")?,
        decision_input: row.try_get("decision_input")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<StoredExecutionEvent, sqlx::Error> {
    Ok(StoredExecutionEvent {
        sequence: row.try_get("sequence")?,
        event_id: row.try_get("event_id")?,
        task_id: row.try_get("task_id")?,
        event_type: row.try_get("event_type")?,
        payload: row.try_get("payload")?,
        created_at: row.try_get("created_at")?,
    })
}

const fn side_text(value: Side) -> &'static str {
    match value {
        Side::Buy => "buy",
        Side::Sell => "sell",
    }
}

const fn strategy_text(value: StrategyKind) -> &'static str {
    match value {
        StrategyKind::Twap => "twap",
        StrategyKind::Pov => "pov",
    }
}

const fn quantity_unit_text(value: QuantityUnit) -> &'static str {
    match value {
        QuantityUnit::BaseQuantity => "base_quantity",
        QuantityUnit::QuoteNotional => "quote_notional",
        QuantityUnit::Contracts => "contracts",
    }
}
