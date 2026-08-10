use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::DatabasePool;

#[derive(Debug, Clone)]
pub struct NewReplayRun {
    pub idempotency_key: String,
    pub instrument_id: Uuid,
    pub template_version_id: Uuid,
    pub strategy_kind: String,
    pub side: String,
    pub requested_amount: Decimal,
    pub quantity_unit: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub execution_model: String,
    pub model_version: String,
    pub fee_rate: Decimal,
    pub extra_slippage_bps: Decimal,
    pub gap_threshold_seconds: i64,
    pub strategy_snapshot: Value,
    pub request_fingerprint: String,
    pub status: String,
    pub failure_code: Option<String>,
    pub data_first_at: Option<DateTime<Utc>>,
    pub data_last_at: Option<DateTime<Utc>>,
    pub trade_count: i64,
    pub gap_count: i32,
    pub data_gaps: Value,
    pub confidence: String,
    pub limitations: Value,
}

#[derive(Debug, Clone)]
pub struct NewReplaySlice {
    pub sequence: i32,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub status: String,
    pub trade_count: i64,
    pub market_volume: Decimal,
    pub requested_amount: Decimal,
    pub filled_amount: Decimal,
    pub filled_native_quantity: Decimal,
    pub market_vwap: Option<Decimal>,
    pub simulated_price: Option<Decimal>,
    pub fee_amount: Decimal,
    pub decision_input: Value,
}

#[derive(Debug, Clone)]
pub struct NewReplayMetrics {
    pub arrival_price: Option<Decimal>,
    pub market_vwap: Option<Decimal>,
    pub simulated_execution_vwap: Option<Decimal>,
    pub implementation_shortfall_bps: Option<Decimal>,
    pub implementation_shortfall_amount: Option<Decimal>,
    pub requested_amount: Decimal,
    pub filled_amount: Decimal,
    pub fill_rate: Decimal,
    pub residual_amount: Decimal,
    pub actual_participation_rate: Option<Decimal>,
    pub target_participation_rate: Option<Decimal>,
    pub participation_rate_deviation: Option<Decimal>,
    pub slice_count: i32,
    pub empty_window_count: i32,
    pub fee_amount: Decimal,
    pub explicit_slippage_amount: Decimal,
}

#[derive(Debug, Clone)]
pub struct StoredReplayRun {
    pub id: Uuid,
    pub idempotency_key: String,
    pub instrument_id: Uuid,
    pub template_version_id: Uuid,
    pub strategy_kind: String,
    pub side: String,
    pub requested_amount: Decimal,
    pub quantity_unit: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub execution_model: String,
    pub model_version: String,
    pub fee_rate: Decimal,
    pub extra_slippage_bps: Decimal,
    pub gap_threshold_seconds: i64,
    pub strategy_snapshot: Value,
    pub request_fingerprint: String,
    pub status: String,
    pub failure_code: Option<String>,
    pub data_first_at: Option<DateTime<Utc>>,
    pub data_last_at: Option<DateTime<Utc>>,
    pub trade_count: i64,
    pub gap_count: i32,
    pub data_gaps: Value,
    pub confidence: String,
    pub limitations: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StoredReplaySlice {
    pub run_id: Uuid,
    pub sequence: i32,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub status: String,
    pub trade_count: i64,
    pub market_volume: Decimal,
    pub requested_amount: Decimal,
    pub filled_amount: Decimal,
    pub filled_native_quantity: Decimal,
    pub market_vwap: Option<Decimal>,
    pub simulated_price: Option<Decimal>,
    pub fee_amount: Decimal,
    pub decision_input: Value,
}

#[derive(Debug, Clone)]
pub struct StoredReplayMetrics {
    pub run_id: Uuid,
    pub arrival_price: Option<Decimal>,
    pub market_vwap: Option<Decimal>,
    pub simulated_execution_vwap: Option<Decimal>,
    pub implementation_shortfall_bps: Option<Decimal>,
    pub implementation_shortfall_amount: Option<Decimal>,
    pub requested_amount: Decimal,
    pub filled_amount: Decimal,
    pub fill_rate: Decimal,
    pub residual_amount: Decimal,
    pub actual_participation_rate: Option<Decimal>,
    pub target_participation_rate: Option<Decimal>,
    pub participation_rate_deviation: Option<Decimal>,
    pub slice_count: i32,
    pub empty_window_count: i32,
    pub fee_amount: Decimal,
    pub explicit_slippage_amount: Decimal,
}

pub async fn create_replay_result(
    pool: &DatabasePool,
    run: &NewReplayRun,
    slices: &[NewReplaySlice],
    metrics: &NewReplayMetrics,
) -> Result<StoredReplayRun, sqlx::Error> {
    let id = Uuid::now_v7();
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        r#"
        INSERT INTO replay_runs (
            id, idempotency_key, instrument_id, template_version_id, strategy_kind,
            side, requested_amount, quantity_unit, start_at, end_at, execution_model,
            model_version, fee_rate, extra_slippage_bps, gap_threshold_seconds,
            strategy_snapshot, request_fingerprint, status, failure_code, data_first_at,
            data_last_at, trade_count, gap_count, data_gaps, confidence, limitations
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
            $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26
        ) ON CONFLICT (idempotency_key) DO NOTHING
        "#,
    )
    .bind(id)
    .bind(&run.idempotency_key)
    .bind(run.instrument_id)
    .bind(run.template_version_id)
    .bind(&run.strategy_kind)
    .bind(&run.side)
    .bind(run.requested_amount)
    .bind(&run.quantity_unit)
    .bind(run.start_at)
    .bind(run.end_at)
    .bind(&run.execution_model)
    .bind(&run.model_version)
    .bind(run.fee_rate)
    .bind(run.extra_slippage_bps)
    .bind(run.gap_threshold_seconds)
    .bind(&run.strategy_snapshot)
    .bind(&run.request_fingerprint)
    .bind(&run.status)
    .bind(&run.failure_code)
    .bind(run.data_first_at)
    .bind(run.data_last_at)
    .bind(run.trade_count)
    .bind(run.gap_count)
    .bind(&run.data_gaps)
    .bind(&run.confidence)
    .bind(&run.limitations)
    .execute(&mut *transaction)
    .await?
    .rows_affected()
        == 1;

    if inserted {
        for slice in slices {
            sqlx::query(
                r#"
                INSERT INTO replay_slices (
                    run_id, sequence, window_start, window_end, status, trade_count,
                    market_volume, requested_amount, filled_amount, filled_native_quantity,
                    market_vwap, simulated_price, fee_amount, decision_input
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                "#,
            )
            .bind(id)
            .bind(slice.sequence)
            .bind(slice.window_start)
            .bind(slice.window_end)
            .bind(&slice.status)
            .bind(slice.trade_count)
            .bind(slice.market_volume)
            .bind(slice.requested_amount)
            .bind(slice.filled_amount)
            .bind(slice.filled_native_quantity)
            .bind(slice.market_vwap)
            .bind(slice.simulated_price)
            .bind(slice.fee_amount)
            .bind(&slice.decision_input)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            r#"
            INSERT INTO replay_metrics (
                run_id, arrival_price, market_vwap, simulated_execution_vwap,
                implementation_shortfall_bps, implementation_shortfall_amount,
                requested_amount, filled_amount, fill_rate, residual_amount,
                actual_participation_rate, target_participation_rate,
                participation_rate_deviation, slice_count, empty_window_count,
                fee_amount, explicit_slippage_amount
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            "#,
        )
        .bind(id)
        .bind(metrics.arrival_price)
        .bind(metrics.market_vwap)
        .bind(metrics.simulated_execution_vwap)
        .bind(metrics.implementation_shortfall_bps)
        .bind(metrics.implementation_shortfall_amount)
        .bind(metrics.requested_amount)
        .bind(metrics.filled_amount)
        .bind(metrics.fill_rate)
        .bind(metrics.residual_amount)
        .bind(metrics.actual_participation_rate)
        .bind(metrics.target_participation_rate)
        .bind(metrics.participation_rate_deviation)
        .bind(metrics.slice_count)
        .bind(metrics.empty_window_count)
        .bind(metrics.fee_amount)
        .bind(metrics.explicit_slippage_amount)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    get_replay_run_by_key(pool, &run.idempotency_key)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn get_replay_run(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredReplayRun>, sqlx::Error> {
    sqlx::query("SELECT * FROM replay_runs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_run)
        .transpose()
}

pub async fn get_replay_run_by_key(
    pool: &DatabasePool,
    key: &str,
) -> Result<Option<StoredReplayRun>, sqlx::Error> {
    sqlx::query("SELECT * FROM replay_runs WHERE idempotency_key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_run)
        .transpose()
}

pub async fn list_replay_slices(
    pool: &DatabasePool,
    run_id: Uuid,
) -> Result<Vec<StoredReplaySlice>, sqlx::Error> {
    sqlx::query("SELECT * FROM replay_slices WHERE run_id = $1 ORDER BY sequence")
        .bind(run_id)
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_slice)
        .collect()
}

pub async fn get_replay_metrics(
    pool: &DatabasePool,
    run_id: Uuid,
) -> Result<Option<StoredReplayMetrics>, sqlx::Error> {
    sqlx::query("SELECT * FROM replay_metrics WHERE run_id = $1")
        .bind(run_id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_metrics)
        .transpose()
}

fn row_to_run(row: &sqlx::postgres::PgRow) -> Result<StoredReplayRun, sqlx::Error> {
    Ok(StoredReplayRun {
        id: row.try_get("id")?,
        idempotency_key: row.try_get("idempotency_key")?,
        instrument_id: row.try_get("instrument_id")?,
        template_version_id: row.try_get("template_version_id")?,
        strategy_kind: row.try_get("strategy_kind")?,
        side: row.try_get("side")?,
        requested_amount: row.try_get("requested_amount")?,
        quantity_unit: row.try_get("quantity_unit")?,
        start_at: row.try_get("start_at")?,
        end_at: row.try_get("end_at")?,
        execution_model: row.try_get("execution_model")?,
        model_version: row.try_get("model_version")?,
        fee_rate: row.try_get("fee_rate")?,
        extra_slippage_bps: row.try_get("extra_slippage_bps")?,
        gap_threshold_seconds: row.try_get("gap_threshold_seconds")?,
        strategy_snapshot: row.try_get("strategy_snapshot")?,
        request_fingerprint: row.try_get("request_fingerprint")?,
        status: row.try_get("status")?,
        failure_code: row.try_get("failure_code")?,
        data_first_at: row.try_get("data_first_at")?,
        data_last_at: row.try_get("data_last_at")?,
        trade_count: row.try_get("trade_count")?,
        gap_count: row.try_get("gap_count")?,
        data_gaps: row.try_get("data_gaps")?,
        confidence: row.try_get("confidence")?,
        limitations: row.try_get("limitations")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_slice(row: &sqlx::postgres::PgRow) -> Result<StoredReplaySlice, sqlx::Error> {
    Ok(StoredReplaySlice {
        run_id: row.try_get("run_id")?,
        sequence: row.try_get("sequence")?,
        window_start: row.try_get("window_start")?,
        window_end: row.try_get("window_end")?,
        status: row.try_get("status")?,
        trade_count: row.try_get("trade_count")?,
        market_volume: row.try_get("market_volume")?,
        requested_amount: row.try_get("requested_amount")?,
        filled_amount: row.try_get("filled_amount")?,
        filled_native_quantity: row.try_get("filled_native_quantity")?,
        market_vwap: row.try_get("market_vwap")?,
        simulated_price: row.try_get("simulated_price")?,
        fee_amount: row.try_get("fee_amount")?,
        decision_input: row.try_get("decision_input")?,
    })
}

fn row_to_metrics(row: &sqlx::postgres::PgRow) -> Result<StoredReplayMetrics, sqlx::Error> {
    Ok(StoredReplayMetrics {
        run_id: row.try_get("run_id")?,
        arrival_price: row.try_get("arrival_price")?,
        market_vwap: row.try_get("market_vwap")?,
        simulated_execution_vwap: row.try_get("simulated_execution_vwap")?,
        implementation_shortfall_bps: row.try_get("implementation_shortfall_bps")?,
        implementation_shortfall_amount: row.try_get("implementation_shortfall_amount")?,
        requested_amount: row.try_get("requested_amount")?,
        filled_amount: row.try_get("filled_amount")?,
        fill_rate: row.try_get("fill_rate")?,
        residual_amount: row.try_get("residual_amount")?,
        actual_participation_rate: row.try_get("actual_participation_rate")?,
        target_participation_rate: row.try_get("target_participation_rate")?,
        participation_rate_deviation: row.try_get("participation_rate_deviation")?,
        slice_count: row.try_get("slice_count")?,
        empty_window_count: row.try_get("empty_window_count")?,
        fee_amount: row.try_get("fee_amount")?,
        explicit_slippage_amount: row.try_get("explicit_slippage_amount")?,
    })
}
