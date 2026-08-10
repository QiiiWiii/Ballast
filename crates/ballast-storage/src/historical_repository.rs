use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::DatabasePool;

#[derive(Debug, Clone)]
pub struct NewHistoricalBackfill {
    pub idempotency_key: String,
    pub instrument_id: Uuid,
    pub data_type: String,
    pub timeframe: Option<String>,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StoredHistoricalBackfill {
    pub id: Uuid,
    pub idempotency_key: String,
    pub instrument_id: Uuid,
    pub data_type: String,
    pub timeframe: Option<String>,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub cursor_at: DateTime<Utc>,
    pub covered_from: Option<DateTime<Utc>>,
    pub covered_to: Option<DateTime<Utc>>,
    pub status: String,
    pub rows_written: i64,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HistoricalCandle {
    pub open_time: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub source: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct HistoricalTrade {
    pub exchange_trade_id: String,
    pub trade_time: DateTime<Utc>,
    pub price: Decimal,
    pub quantity: Decimal,
    pub taker_side: String,
    pub source: String,
    pub observed_at: DateTime<Utc>,
}

pub async fn create_or_get_historical_backfill(
    pool: &DatabasePool,
    value: &NewHistoricalBackfill,
) -> Result<StoredHistoricalBackfill, sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO historical_backfill_jobs (
            id, idempotency_key, instrument_id, data_type, timeframe,
            start_at, end_at, cursor_at, status
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $6, 'pending')
        ON CONFLICT (idempotency_key) DO NOTHING
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(&value.idempotency_key)
    .bind(value.instrument_id)
    .bind(&value.data_type)
    .bind(&value.timeframe)
    .bind(value.start_at)
    .bind(value.end_at)
    .execute(pool)
    .await?;
    get_historical_backfill_by_key(pool, &value.idempotency_key)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn get_historical_backfill(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredHistoricalBackfill>, sqlx::Error> {
    sqlx::query("SELECT * FROM historical_backfill_jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_backfill)
        .transpose()
}

async fn get_historical_backfill_by_key(
    pool: &DatabasePool,
    key: &str,
) -> Result<Option<StoredHistoricalBackfill>, sqlx::Error> {
    sqlx::query("SELECT * FROM historical_backfill_jobs WHERE idempotency_key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_backfill)
        .transpose()
}

pub async fn mark_historical_backfill_running(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE historical_backfill_jobs SET status = 'running', last_error_code = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_historical_backfill_failed(
    pool: &DatabasePool,
    id: Uuid,
    error_code: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE historical_backfill_jobs SET status = 'failed', last_error_code = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(error_code)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn persist_candle_batch(
    pool: &DatabasePool,
    job_id: Uuid,
    instrument_id: Uuid,
    timeframe: &str,
    candles: &[HistoricalCandle],
    next_cursor: DateTime<Utc>,
    completed: bool,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let mut inserted = 0_i64;
    for candle in candles {
        let result = sqlx::query(
            r#"
            INSERT INTO historical_candles (
                instrument_id, timeframe, open_time, open, high, low, close,
                volume, source, observed_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (instrument_id, timeframe, open_time) DO NOTHING
            "#,
        )
        .bind(instrument_id)
        .bind(timeframe)
        .bind(candle.open_time)
        .bind(candle.open)
        .bind(candle.high)
        .bind(candle.low)
        .bind(candle.close)
        .bind(candle.volume)
        .bind(&candle.source)
        .bind(candle.observed_at)
        .execute(&mut *transaction)
        .await?;
        inserted += result.rows_affected() as i64;
    }
    update_progress(
        &mut transaction,
        job_id,
        inserted,
        candles.first().map(|v| v.open_time),
        candles.last().map(|v| v.open_time),
        next_cursor,
        completed,
    )
    .await?;
    transaction.commit().await
}

pub async fn persist_trade_batch(
    pool: &DatabasePool,
    job_id: Uuid,
    instrument_id: Uuid,
    trades: &[HistoricalTrade],
    next_cursor: DateTime<Utc>,
    completed: bool,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let mut inserted = 0_i64;
    for trade in trades {
        let result = sqlx::query(
            r#"
            INSERT INTO historical_trades (
                instrument_id, exchange_trade_id, trade_time, price, quantity,
                taker_side, source, observed_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (instrument_id, exchange_trade_id) DO NOTHING
            "#,
        )
        .bind(instrument_id)
        .bind(&trade.exchange_trade_id)
        .bind(trade.trade_time)
        .bind(trade.price)
        .bind(trade.quantity)
        .bind(&trade.taker_side)
        .bind(&trade.source)
        .bind(trade.observed_at)
        .execute(&mut *transaction)
        .await?;
        inserted += result.rows_affected() as i64;
    }
    update_progress(
        &mut transaction,
        job_id,
        inserted,
        trades.first().map(|v| v.trade_time),
        trades.last().map(|v| v.trade_time),
        next_cursor,
        completed,
    )
    .await?;
    transaction.commit().await
}

async fn update_progress(
    transaction: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    rows: i64,
    batch_from: Option<DateTime<Utc>>,
    batch_to: Option<DateTime<Utc>>,
    next_cursor: DateTime<Utc>,
    completed: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE historical_backfill_jobs SET
            cursor_at = GREATEST(cursor_at, $2),
            covered_from = CASE WHEN $3::timestamptz IS NULL THEN covered_from ELSE LEAST(COALESCE(covered_from, $3), $3) END,
            covered_to = CASE WHEN $4::timestamptz IS NULL THEN covered_to ELSE GREATEST(COALESCE(covered_to, $4), $4) END,
            rows_written = rows_written + $5,
            status = CASE WHEN $6 THEN 'completed' ELSE 'pending' END,
            last_error_code = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .bind(next_cursor)
    .bind(batch_from)
    .bind(batch_to)
    .bind(rows)
    .bind(completed)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn list_historical_candles(
    pool: &DatabasePool,
    instrument_id: Uuid,
    timeframe: &str,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<HistoricalCandle>, i64), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT *, COUNT(*) OVER()::bigint AS total_count
        FROM historical_candles
        WHERE instrument_id = $1 AND timeframe = $2
          AND open_time >= $3 AND open_time < $4
        ORDER BY open_time, instrument_id
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(instrument_id)
    .bind(timeframe)
    .bind(start_at)
    .bind(end_at)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    let total = rows
        .first()
        .map_or(0, |row| row.try_get("total_count").unwrap_or(0));
    Ok((
        rows.iter().map(row_to_candle).collect::<Result<_, _>>()?,
        total,
    ))
}

pub async fn list_historical_trades(
    pool: &DatabasePool,
    instrument_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<HistoricalTrade>, i64), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT *, COUNT(*) OVER()::bigint AS total_count
        FROM historical_trades
        WHERE instrument_id = $1 AND trade_time >= $2 AND trade_time < $3
        ORDER BY trade_time, exchange_trade_id
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(instrument_id)
    .bind(start_at)
    .bind(end_at)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    let total = rows
        .first()
        .map_or(0, |row| row.try_get("total_count").unwrap_or(0));
    Ok((
        rows.iter().map(row_to_trade).collect::<Result<_, _>>()?,
        total,
    ))
}

fn row_to_backfill(row: &sqlx::postgres::PgRow) -> Result<StoredHistoricalBackfill, sqlx::Error> {
    Ok(StoredHistoricalBackfill {
        id: row.try_get("id")?,
        idempotency_key: row.try_get("idempotency_key")?,
        instrument_id: row.try_get("instrument_id")?,
        data_type: row.try_get("data_type")?,
        timeframe: row.try_get("timeframe")?,
        start_at: row.try_get("start_at")?,
        end_at: row.try_get("end_at")?,
        cursor_at: row.try_get("cursor_at")?,
        covered_from: row.try_get("covered_from")?,
        covered_to: row.try_get("covered_to")?,
        status: row.try_get("status")?,
        rows_written: row.try_get("rows_written")?,
        last_error_code: row.try_get("last_error_code")?,
    })
}

fn row_to_candle(row: &sqlx::postgres::PgRow) -> Result<HistoricalCandle, sqlx::Error> {
    Ok(HistoricalCandle {
        open_time: row.try_get("open_time")?,
        open: row.try_get("open")?,
        high: row.try_get("high")?,
        low: row.try_get("low")?,
        close: row.try_get("close")?,
        volume: row.try_get("volume")?,
        source: row.try_get("source")?,
        observed_at: row.try_get("observed_at")?,
    })
}

fn row_to_trade(row: &sqlx::postgres::PgRow) -> Result<HistoricalTrade, sqlx::Error> {
    Ok(HistoricalTrade {
        exchange_trade_id: row.try_get("exchange_trade_id")?,
        trade_time: row.try_get("trade_time")?,
        price: row.try_get("price")?,
        quantity: row.try_get("quantity")?,
        taker_side: row.try_get("taker_side")?,
        source: row.try_get("source")?,
        observed_at: row.try_get("observed_at")?,
    })
}
