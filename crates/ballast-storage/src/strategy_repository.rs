use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::DatabasePool;

#[derive(Debug, Clone)]
pub struct NewStrategyTemplate {
    pub name: String,
    pub description: String,
    pub strategy_kind: String,
    pub quantity_unit: String,
    pub duration_seconds: i64,
    pub slice_interval_ms: i64,
    pub max_slippage_bps: i32,
    pub participation_rate: Option<Decimal>,
    pub max_slice_amount: Option<Decimal>,
    pub change_note: String,
    pub execution_backend: String,
    pub venue_exchange: Option<String>,
    pub venue_market_kind: Option<String>,
    pub native_algorithm: Option<String>,
    pub native_params: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct NewStrategyTemplateVersion {
    pub strategy_kind: String,
    pub quantity_unit: String,
    pub duration_seconds: i64,
    pub slice_interval_ms: i64,
    pub max_slippage_bps: i32,
    pub participation_rate: Option<Decimal>,
    pub max_slice_amount: Option<Decimal>,
    pub change_note: String,
    pub execution_backend: String,
    pub venue_exchange: Option<String>,
    pub venue_market_kind: Option<String>,
    pub native_algorithm: Option<String>,
    pub native_params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredStrategyTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
    pub current_version: i32,
    pub usage_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredStrategyTemplateVersion {
    pub id: Uuid,
    pub template_id: Uuid,
    pub version: i32,
    pub strategy_kind: String,
    pub quantity_unit: String,
    pub duration_seconds: i64,
    pub slice_interval_ms: i64,
    pub max_slippage_bps: i32,
    pub participation_rate: Option<Decimal>,
    pub max_slice_amount: Option<Decimal>,
    pub execution_backend: String,
    pub venue_exchange: Option<String>,
    pub venue_market_kind: Option<String>,
    pub native_algorithm: Option<String>,
    pub native_params: Option<serde_json::Value>,
    pub change_note: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create_strategy_template(
    pool: &DatabasePool,
    input: NewStrategyTemplate,
) -> Result<(StoredStrategyTemplate, StoredStrategyTemplateVersion), sqlx::Error> {
    let template_id = Uuid::now_v7();
    let version_id = Uuid::now_v7();
    let mut transaction = pool.begin().await?;
    sqlx::query("INSERT INTO strategy_templates (id, name, description) VALUES ($1, $2, $3)")
        .bind(template_id)
        .bind(&input.name)
        .bind(&input.description)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        r#"
        INSERT INTO strategy_template_versions (
            id, template_id, version, strategy_kind, quantity_unit,
            duration_seconds, slice_interval_ms, max_slippage_bps,
            participation_rate, max_slice_amount, change_note, execution_backend,
            venue_exchange, venue_market_kind, native_algorithm, native_params
        ) VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        "#,
    )
    .bind(version_id)
    .bind(template_id)
    .bind(input.strategy_kind)
    .bind(input.quantity_unit)
    .bind(input.duration_seconds)
    .bind(input.slice_interval_ms)
    .bind(input.max_slippage_bps)
    .bind(input.participation_rate)
    .bind(input.max_slice_amount)
    .bind(input.change_note)
    .bind(input.execution_backend)
    .bind(input.venue_exchange)
    .bind(input.venue_market_kind)
    .bind(input.native_algorithm)
    .bind(input.native_params)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    let template = get_strategy_template(pool, template_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let version = get_strategy_template_version(pool, version_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok((template, version))
}

pub async fn create_strategy_template_version(
    pool: &DatabasePool,
    template_id: Uuid,
    input: NewStrategyTemplateVersion,
) -> Result<StoredStrategyTemplateVersion, sqlx::Error> {
    let version_id = Uuid::now_v7();
    let mut transaction = pool.begin().await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM strategy_templates WHERE id = $1 FOR UPDATE")
            .bind(template_id)
            .fetch_optional(&mut *transaction)
            .await?;
    match status.as_deref() {
        None => return Err(sqlx::Error::RowNotFound),
        Some("active") => {}
        Some(_) => {
            return Err(sqlx::Error::Protocol(
                "strategy template is archived".into(),
            ));
        }
    }
    let next_version: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0)::integer + 1 FROM strategy_template_versions WHERE template_id = $1",
    )
    .bind(template_id)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO strategy_template_versions (
            id, template_id, version, strategy_kind, quantity_unit,
            duration_seconds, slice_interval_ms, max_slippage_bps,
            participation_rate, max_slice_amount, change_note, execution_backend,
            venue_exchange, venue_market_kind, native_algorithm, native_params
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        "#,
    )
    .bind(version_id)
    .bind(template_id)
    .bind(next_version)
    .bind(input.strategy_kind)
    .bind(input.quantity_unit)
    .bind(input.duration_seconds)
    .bind(input.slice_interval_ms)
    .bind(input.max_slippage_bps)
    .bind(input.participation_rate)
    .bind(input.max_slice_amount)
    .bind(input.change_note)
    .bind(input.execution_backend)
    .bind(input.venue_exchange)
    .bind(input.venue_market_kind)
    .bind(input.native_algorithm)
    .bind(input.native_params)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE strategy_templates SET updated_at = now() WHERE id = $1")
        .bind(template_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    get_strategy_template_version(pool, version_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn list_strategy_templates(
    pool: &DatabasePool,
) -> Result<Vec<StoredStrategyTemplate>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT template.*,
               COALESCE(MAX(version.version), 0)::integer AS current_version,
               COUNT(task.id)::bigint AS usage_count
        FROM strategy_templates template
        LEFT JOIN strategy_template_versions version ON version.template_id = template.id
        LEFT JOIN execution_tasks task ON task.template_version_id = version.id
        GROUP BY template.id
        ORDER BY template.status, lower(template.name)
        "#,
    )
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_template).collect()
}

pub async fn get_strategy_template(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredStrategyTemplate>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT template.*,
               COALESCE(MAX(version.version), 0)::integer AS current_version,
               COUNT(task.id)::bigint AS usage_count
        FROM strategy_templates template
        LEFT JOIN strategy_template_versions version ON version.template_id = template.id
        LEFT JOIN execution_tasks task ON task.template_version_id = version.id
        WHERE template.id = $1
        GROUP BY template.id
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(row_to_template).transpose()
}

pub async fn get_strategy_template_version(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredStrategyTemplateVersion>, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM strategy_template_versions WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(row_to_version).transpose()
}

pub async fn list_strategy_template_versions(
    pool: &DatabasePool,
    template_id: Uuid,
) -> Result<Vec<StoredStrategyTemplateVersion>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT * FROM strategy_template_versions WHERE template_id = $1 ORDER BY version DESC",
    )
    .bind(template_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_version).collect()
}

pub async fn archive_strategy_template(pool: &DatabasePool, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE strategy_templates SET status = 'archived', updated_at = now() WHERE id = $1 AND status = 'active'",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

fn row_to_template(row: &sqlx::postgres::PgRow) -> Result<StoredStrategyTemplate, sqlx::Error> {
    Ok(StoredStrategyTemplate {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        status: row.try_get("status")?,
        current_version: row.try_get("current_version")?,
        usage_count: row.try_get("usage_count")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_version(
    row: &sqlx::postgres::PgRow,
) -> Result<StoredStrategyTemplateVersion, sqlx::Error> {
    Ok(StoredStrategyTemplateVersion {
        id: row.try_get("id")?,
        template_id: row.try_get("template_id")?,
        version: row.try_get("version")?,
        strategy_kind: row.try_get("strategy_kind")?,
        quantity_unit: row.try_get("quantity_unit")?,
        duration_seconds: row.try_get("duration_seconds")?,
        slice_interval_ms: row.try_get("slice_interval_ms")?,
        max_slippage_bps: row.try_get("max_slippage_bps")?,
        participation_rate: row.try_get("participation_rate")?,
        max_slice_amount: row.try_get("max_slice_amount")?,
        execution_backend: row.try_get("execution_backend")?,
        venue_exchange: row.try_get("venue_exchange")?,
        venue_market_kind: row.try_get("venue_market_kind")?,
        native_algorithm: row.try_get("native_algorithm")?,
        native_params: row.try_get("native_params")?,
        change_note: row.try_get("change_note")?,
        created_at: row.try_get("created_at")?,
    })
}
