use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::DatabasePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredValidationCase {
    pub id: Uuid,
    pub name: String,
    pub version: i32,
    pub symbol: String,
    pub asset_class: String,
    pub instrument_kind: String,
    pub currency: String,
    pub market_data_provider: String,
    pub market_data_dataset: String,
    pub execution_venue: Option<String>,
    pub side: String,
    pub target_notional_usd: Decimal,
    pub timezone: String,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub max_participation_rate: Decimal,
    pub warmup_sessions: i32,
    pub evaluation_sessions: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ValidationCaseUpdate {
    pub name: String,
    pub target_notional_usd: Decimal,
    pub max_participation_rate: Decimal,
    pub evaluation_sessions: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDownloadJob {
    pub id: Uuid,
    pub provider: String,
    pub dataset: String,
    pub symbol: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub requested_sessions: i32,
    pub status: String,
    pub downloaded_records: i64,
    pub verified_sessions: i32,
    pub error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewDownloadJob {
    pub symbol: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub requested_sessions: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDailyManifest {
    pub id: Uuid,
    pub download_job_id: Uuid,
    pub provider: String,
    pub dataset: String,
    pub symbol: String,
    pub session_date: NaiveDate,
    pub schema_name: String,
    pub record_count: i32,
    pub sha256: String,
    pub storage_path: String,
    pub first_bar_at: DateTime<Utc>,
    pub last_bar_at: DateTime<Utc>,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewDailyManifest {
    pub download_job_id: Uuid,
    pub symbol: String,
    pub session_date: NaiveDate,
    pub schema_name: String,
    pub record_count: i32,
    pub sha256: String,
    pub storage_path: String,
    pub first_bar_at: DateTime<Utc>,
    pub last_bar_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredValidationRun {
    pub id: Uuid,
    pub case_id: Uuid,
    pub case_snapshot: serde_json::Value,
    pub strategy_version_ids: Vec<Uuid>,
    pub status: String,
    pub data_quality: String,
    pub report: Option<serde_json::Value>,
    pub report_sha256: Option<String>,
    pub error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredValidationDecision {
    pub sequence: i64,
    pub run_id: Uuid,
    pub decision: String,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

pub async fn list_validation_cases(
    pool: &DatabasePool,
) -> Result<Vec<StoredValidationCase>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT case_record.*, instrument.symbol, instrument.asset_class, instrument.instrument_kind,
               instrument.currency, instrument.market_data_provider, instrument.market_data_dataset,
               instrument.execution_venue
        FROM validation_cases case_record
        JOIN research_instruments instrument ON instrument.id = case_record.instrument_id
        ORDER BY case_record.status, case_record.created_at
        "#,
    )
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_case).collect()
}

pub async fn get_validation_case(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredValidationCase>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT case_record.*, instrument.symbol, instrument.asset_class, instrument.instrument_kind,
               instrument.currency, instrument.market_data_provider, instrument.market_data_dataset,
               instrument.execution_venue
        FROM validation_cases case_record
        JOIN research_instruments instrument ON instrument.id = case_record.instrument_id
        WHERE case_record.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(row_to_case).transpose()
}

pub async fn update_validation_case(
    pool: &DatabasePool,
    id: Uuid,
    input: ValidationCaseUpdate,
) -> Result<Option<StoredValidationCase>, sqlx::Error> {
    let updated = sqlx::query(
        r#"
        UPDATE validation_cases
        SET name = $2, target_notional_usd = $3, max_participation_rate = $4,
            evaluation_sessions = $5, version = version + 1, updated_at = now()
        WHERE id = $1 AND status = 'active'
        "#,
    )
    .bind(id)
    .bind(input.name)
    .bind(input.target_notional_usd)
    .bind(input.max_participation_rate)
    .bind(input.evaluation_sessions)
    .execute(pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Ok(None);
    }
    get_validation_case(pool, id).await
}

pub async fn create_download_job(
    pool: &DatabasePool,
    input: NewDownloadJob,
) -> Result<StoredDownloadJob, sqlx::Error> {
    if let Some(existing) = find_reusable_download_job(pool, &input).await? {
        return Ok(existing);
    }
    let id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO research_download_jobs (
            id, provider, dataset, symbol, start_at, end_at, requested_sessions, status
        ) VALUES ($1, 'alpaca', 'iex_1min_bars', $2, $3, $4, $5, 'queued')
        "#,
    )
    .bind(id)
    .bind(input.symbol)
    .bind(input.start_at)
    .bind(input.end_at)
    .bind(input.requested_sessions)
    .execute(pool)
    .await?;
    get_download_job(pool, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

async fn find_reusable_download_job(
    pool: &DatabasePool,
    input: &NewDownloadJob,
) -> Result<Option<StoredDownloadJob>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT * FROM research_download_jobs
        WHERE provider = 'alpaca' AND dataset = 'iex_1min_bars'
          AND symbol = $1 AND start_at = $2 AND end_at = $3
          AND status IN ('queued', 'downloading', 'verifying', 'completed')
        ORDER BY created_at DESC LIMIT 1
        "#,
    )
    .bind(&input.symbol)
    .bind(input.start_at)
    .bind(input.end_at)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(row_to_download_job).transpose()
}

pub async fn list_download_jobs(
    pool: &DatabasePool,
) -> Result<Vec<StoredDownloadJob>, sqlx::Error> {
    let rows =
        sqlx::query("SELECT * FROM research_download_jobs ORDER BY created_at DESC LIMIT 100")
            .fetch_all(pool)
            .await?;
    rows.iter().map(row_to_download_job).collect()
}

pub async fn get_download_job(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredDownloadJob>, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM research_download_jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(row_to_download_job).transpose()
}

pub async fn set_download_job_state(
    pool: &DatabasePool,
    id: Uuid,
    expected: &[&str],
    status: &str,
    downloaded_records: Option<i64>,
    verified_sessions: Option<i32>,
    error_code: Option<&str>,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        UPDATE research_download_jobs
        SET status = $3,
            downloaded_records = COALESCE($4, downloaded_records),
            verified_sessions = COALESCE($5, verified_sessions),
            error_code = $6,
            updated_at = now()
        WHERE id = $1 AND status = ANY($2)
        "#,
    )
    .bind(id)
    .bind(expected)
    .bind(status)
    .bind(downloaded_records)
    .bind(verified_sessions)
    .bind(error_code)
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

pub async fn upsert_daily_manifests(
    pool: &DatabasePool,
    manifests: &[NewDailyManifest],
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    for manifest in manifests {
        sqlx::query(
            r#"
            INSERT INTO research_daily_manifests (
                id, download_job_id, provider, dataset, symbol, session_date, schema_name,
                record_count, sha256, storage_path, first_bar_at, last_bar_at
            ) VALUES ($1, $2, 'alpaca', 'iex_1min_bars', $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (provider, dataset, symbol, session_date) DO UPDATE SET
                download_job_id = EXCLUDED.download_job_id,
                schema_name = EXCLUDED.schema_name,
                record_count = EXCLUDED.record_count,
                sha256 = EXCLUDED.sha256,
                storage_path = EXCLUDED.storage_path,
                first_bar_at = EXCLUDED.first_bar_at,
                last_bar_at = EXCLUDED.last_bar_at,
                verified_at = now()
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(manifest.download_job_id)
        .bind(&manifest.symbol)
        .bind(manifest.session_date)
        .bind(&manifest.schema_name)
        .bind(manifest.record_count)
        .bind(&manifest.sha256)
        .bind(&manifest.storage_path)
        .bind(manifest.first_bar_at)
        .bind(manifest.last_bar_at)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await
}

pub async fn list_daily_manifests(
    pool: &DatabasePool,
    symbol: &str,
    limit: i64,
) -> Result<Vec<StoredDailyManifest>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM research_daily_manifests
        WHERE provider = 'alpaca' AND dataset = 'iex_1min_bars' AND symbol = $1
        ORDER BY session_date DESC LIMIT $2
        "#,
    )
    .bind(symbol)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_manifest).collect()
}

pub async fn create_validation_run(
    pool: &DatabasePool,
    case: &StoredValidationCase,
    strategy_version_ids: Vec<Uuid>,
) -> Result<StoredValidationRun, sqlx::Error> {
    let id = Uuid::now_v7();
    let snapshot =
        serde_json::to_value(case).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    sqlx::query(
        r#"
        INSERT INTO validation_runs (
            id, case_id, case_snapshot, strategy_version_ids, status, data_quality
        ) VALUES ($1, $2, $3, $4, 'queued', 'iex_proxy')
        "#,
    )
    .bind(id)
    .bind(case.id)
    .bind(snapshot)
    .bind(strategy_version_ids)
    .execute(pool)
    .await?;
    get_validation_run(pool, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn list_validation_runs(
    pool: &DatabasePool,
) -> Result<Vec<StoredValidationRun>, sqlx::Error> {
    let rows = sqlx::query("SELECT * FROM validation_runs ORDER BY created_at DESC LIMIT 100")
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_run).collect()
}

pub async fn get_validation_run(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredValidationRun>, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM validation_runs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(row_to_run).transpose()
}

pub async fn get_validation_runs_by_ids(
    pool: &DatabasePool,
    ids: &[Uuid],
) -> Result<Vec<StoredValidationRun>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT * FROM validation_runs
        WHERE id = ANY($1)
        ORDER BY created_at DESC
        "#,
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_run).collect()
}

pub async fn latest_validation_decisions(
    pool: &DatabasePool,
    run_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, StoredValidationDecision>, sqlx::Error> {
    use std::collections::HashMap;
    if run_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT ON (run_id) *
        FROM validation_decisions
        WHERE run_id = ANY($1)
        ORDER BY run_id, sequence DESC
        "#,
    )
    .bind(run_ids)
    .fetch_all(pool)
    .await?;
    let mut map = HashMap::new();
    for row in rows {
        let decision = row_to_decision(&row)?;
        map.insert(decision.run_id, decision);
    }
    Ok(map)
}

pub async fn set_validation_run_state(
    pool: &DatabasePool,
    id: Uuid,
    expected: &[&str],
    status: &str,
    report: Option<&serde_json::Value>,
    report_sha256: Option<&str>,
    error_code: Option<&str>,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        UPDATE validation_runs
        SET status = $3,
            report = $4,
            report_sha256 = $5,
            error_code = $6,
            started_at = CASE WHEN $3 = 'running' THEN COALESCE(started_at, now()) ELSE started_at END,
            completed_at = CASE WHEN $3 IN ('succeeded', 'failed', 'cancelled') THEN now() ELSE completed_at END,
            updated_at = now()
        WHERE id = $1 AND status = ANY($2)
        "#,
    )
    .bind(id)
    .bind(expected)
    .bind(status)
    .bind(report)
    .bind(report_sha256)
    .bind(error_code)
    .execute(pool)
    .await?
    .rows_affected() == 1)
}

pub async fn replace_session_results(
    pool: &DatabasePool,
    run_id: Uuid,
    report: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM validation_session_results WHERE run_id = $1")
        .bind(run_id)
        .execute(&mut *transaction)
        .await?;
    if let Some(sessions) = report.get("sessions").and_then(serde_json::Value::as_array) {
        for session in sessions {
            let Some(date) = session.get("date").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
            if let Some(candidates) = session
                .get("candidates")
                .and_then(serde_json::Value::as_array)
            {
                for candidate in candidates {
                    let Some(name) = candidate
                        .get("strategy")
                        .and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    sqlx::query(
                        "INSERT INTO validation_session_results (run_id, session_date, candidate, result) VALUES ($1, $2, $3, $4)",
                    )
                    .bind(run_id)
                    .bind(date)
                    .bind(name)
                    .bind(candidate)
                    .execute(&mut *transaction)
                    .await?;
                }
            }
        }
    }
    transaction.commit().await
}

pub async fn append_validation_decision(
    pool: &DatabasePool,
    run_id: Uuid,
    decision: &str,
    note: &str,
) -> Result<StoredValidationDecision, sqlx::Error> {
    let row = sqlx::query(
        "INSERT INTO validation_decisions (run_id, decision, note) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(run_id)
    .bind(decision)
    .bind(note)
    .fetch_one(pool)
    .await?;
    row_to_decision(&row)
}

pub async fn list_validation_decisions(
    pool: &DatabasePool,
    run_id: Uuid,
) -> Result<Vec<StoredValidationDecision>, sqlx::Error> {
    let rows =
        sqlx::query("SELECT * FROM validation_decisions WHERE run_id = $1 ORDER BY sequence DESC")
            .bind(run_id)
            .fetch_all(pool)
            .await?;
    rows.iter().map(row_to_decision).collect()
}

fn row_to_case(row: &sqlx::postgres::PgRow) -> Result<StoredValidationCase, sqlx::Error> {
    Ok(StoredValidationCase {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        version: row.try_get("version")?,
        symbol: row.try_get("symbol")?,
        asset_class: row.try_get("asset_class")?,
        instrument_kind: row.try_get("instrument_kind")?,
        currency: row.try_get("currency")?,
        market_data_provider: row.try_get("market_data_provider")?,
        market_data_dataset: row.try_get("market_data_dataset")?,
        execution_venue: row.try_get("execution_venue")?,
        side: row.try_get("side")?,
        target_notional_usd: row.try_get("target_notional_usd")?,
        timezone: row.try_get("timezone")?,
        start_time: row.try_get("start_time")?,
        end_time: row.try_get("end_time")?,
        max_participation_rate: row.try_get("max_participation_rate")?,
        warmup_sessions: row.try_get("warmup_sessions")?,
        evaluation_sessions: row.try_get("evaluation_sessions")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_download_job(row: &sqlx::postgres::PgRow) -> Result<StoredDownloadJob, sqlx::Error> {
    Ok(StoredDownloadJob {
        id: row.try_get("id")?,
        provider: row.try_get("provider")?,
        dataset: row.try_get("dataset")?,
        symbol: row.try_get("symbol")?,
        start_at: row.try_get("start_at")?,
        end_at: row.try_get("end_at")?,
        requested_sessions: row.try_get("requested_sessions")?,
        status: row.try_get("status")?,
        downloaded_records: row.try_get("downloaded_records")?,
        verified_sessions: row.try_get("verified_sessions")?,
        error_code: row.try_get("error_code")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_manifest(row: &sqlx::postgres::PgRow) -> Result<StoredDailyManifest, sqlx::Error> {
    Ok(StoredDailyManifest {
        id: row.try_get("id")?,
        download_job_id: row.try_get("download_job_id")?,
        provider: row.try_get("provider")?,
        dataset: row.try_get("dataset")?,
        symbol: row.try_get("symbol")?,
        session_date: row.try_get("session_date")?,
        schema_name: row.try_get("schema_name")?,
        record_count: row.try_get("record_count")?,
        sha256: row.try_get("sha256")?,
        storage_path: row.try_get("storage_path")?,
        first_bar_at: row.try_get("first_bar_at")?,
        last_bar_at: row.try_get("last_bar_at")?,
        verified_at: row.try_get("verified_at")?,
    })
}

fn row_to_run(row: &sqlx::postgres::PgRow) -> Result<StoredValidationRun, sqlx::Error> {
    Ok(StoredValidationRun {
        id: row.try_get("id")?,
        case_id: row.try_get("case_id")?,
        case_snapshot: row.try_get("case_snapshot")?,
        strategy_version_ids: row.try_get("strategy_version_ids")?,
        status: row.try_get("status")?,
        data_quality: row.try_get("data_quality")?,
        report: row.try_get("report")?,
        report_sha256: row.try_get("report_sha256")?,
        error_code: row.try_get("error_code")?,
        created_at: row.try_get("created_at")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_decision(row: &sqlx::postgres::PgRow) -> Result<StoredValidationDecision, sqlx::Error> {
    Ok(StoredValidationDecision {
        sequence: row.try_get("sequence")?,
        run_id: row.try_get("run_id")?,
        decision: row.try_get("decision")?,
        note: row.try_get("note")?,
        created_at: row.try_get("created_at")?,
    })
}
