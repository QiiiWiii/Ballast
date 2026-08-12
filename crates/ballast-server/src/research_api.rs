use std::{path::PathBuf, str::FromStr};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use ballast_alpaca::{AlpacaClient, DailyManifest};
use ballast_research::{StrategyConfig, ValidationCase};
use ballast_storage::{
    NewDailyManifest, NewDownloadJob, StoredDownloadJob, StoredValidationCase,
    StoredValidationDecision, StoredValidationRun, ValidationCaseUpdate,
};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AppState,
    api::{ApiError, ApiResult},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/research/coverage", post(check_coverage))
        .route(
            "/api/v1/research/download-jobs",
            get(list_download_jobs).post(create_download_job),
        )
        .route(
            "/api/v1/research/download-jobs/{job_id}",
            get(get_download_job),
        )
        .route(
            "/api/v1/research/download-jobs/{job_id}/cancel",
            post(cancel_download_job),
        )
        .route("/api/v1/validation-cases", get(list_validation_cases))
        .route(
            "/api/v1/validation-cases/{case_id}",
            get(get_validation_case).patch(update_validation_case),
        )
        .route(
            "/api/v1/validation-runs",
            get(list_validation_runs).post(create_validation_run),
        )
        .route(
            "/api/v1/validation-runs/compare",
            get(compare_validation_runs),
        )
        .route("/api/v1/validation-runs/{run_id}", get(get_validation_run))
        .route(
            "/api/v1/validation-runs/{run_id}/cancel",
            post(cancel_validation_run),
        )
        .route(
            "/api/v1/validation-runs/{run_id}/report",
            get(get_validation_report),
        )
        .route(
            "/api/v1/validation-runs/{run_id}/export",
            get(export_validation_run),
        )
        .route(
            "/api/v1/validation-runs/{run_id}/decisions",
            get(list_validation_decisions).post(create_validation_decision),
        )
}

#[derive(Debug, Deserialize)]
struct CoverageInput {
    symbol: String,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct DownloadInput {
    symbol: String,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    requested_sessions: Option<i32>,
}

#[derive(Debug, Serialize)]
struct ValidationCaseView {
    id: Uuid,
    name: String,
    version: i32,
    symbol: String,
    asset_class: String,
    instrument_kind: String,
    currency: String,
    market_data_provider: String,
    market_data_dataset: String,
    execution_venue: Option<String>,
    side: String,
    target_notional_usd: String,
    timezone: String,
    start_time: String,
    end_time: String,
    max_participation_rate: String,
    warmup_sessions: i32,
    evaluation_sessions: i32,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<StoredValidationCase> for ValidationCaseView {
    fn from(value: StoredValidationCase) -> Self {
        Self {
            id: value.id,
            name: value.name,
            version: value.version,
            symbol: value.symbol,
            asset_class: value.asset_class,
            instrument_kind: value.instrument_kind,
            currency: value.currency,
            market_data_provider: value.market_data_provider,
            market_data_dataset: value.market_data_dataset,
            execution_venue: value.execution_venue,
            side: value.side,
            target_notional_usd: value.target_notional_usd.to_string(),
            timezone: value.timezone,
            start_time: value.start_time.format("%H:%M:%S").to_string(),
            end_time: value.end_time.format("%H:%M:%S").to_string(),
            max_participation_rate: value.max_participation_rate.to_string(),
            warmup_sessions: value.warmup_sessions,
            evaluation_sessions: value.evaluation_sessions,
            status: value.status,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CaseUpdateInput {
    name: String,
    target_notional_usd: String,
    max_participation_rate: String,
    evaluation_sessions: i32,
}

#[derive(Debug, Deserialize)]
struct RunInput {
    case_id: Uuid,
    strategy_version_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
struct DecisionInput {
    decision: String,
    note: Option<String>,
}

async fn check_coverage(Json(input): Json<CoverageInput>) -> ApiResult<Json<Value>> {
    let symbol = validate_symbol(&input.symbol)?;
    let (start, end) = date_range(input.start, input.end)?;
    let client = alpaca_client()?;
    let coverage = client
        .coverage(symbol, start, end)
        .await
        .map_err(map_alpaca_error)?;
    Ok(Json(serde_json::to_value(coverage).map_err(|_| {
        ApiError::internal("coverage_encode_failed")
    })?))
}

async fn create_download_job(
    State(state): State<AppState>,
    Json(input): Json<DownloadInput>,
) -> ApiResult<(StatusCode, Json<StoredDownloadJob>)> {
    let symbol = validate_symbol(&input.symbol)?.to_owned();
    let (start, end) = date_range(input.start, input.end)?;
    let requested_sessions = input.requested_sessions.unwrap_or(120);
    if requested_sessions != 120 {
        return Err(ApiError::validation(
            "research_sessions_must_equal_120",
            json!({ "required": 120 }),
        ));
    }
    alpaca_client()?;
    let job = ballast_storage::create_download_job(
        &state.database,
        NewDownloadJob {
            symbol,
            start_at: start,
            end_at: end,
            requested_sessions,
        },
    )
    .await
    .map_err(ApiError::database)?;
    if job.status == "queued" {
        tokio::spawn(run_download_job(state.clone(), job.id));
    }
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn list_download_jobs(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<StoredDownloadJob>>> {
    Ok(Json(
        ballast_storage::list_download_jobs(&state.database)
            .await
            .map_err(ApiError::database)?,
    ))
}

async fn get_download_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> ApiResult<Json<StoredDownloadJob>> {
    Ok(Json(
        ballast_storage::get_download_job(&state.database, job_id)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::not_found("research_download_job_not_found"))?,
    ))
}

async fn cancel_download_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> ApiResult<Json<StoredDownloadJob>> {
    ballast_storage::set_download_job_state(
        &state.database,
        job_id,
        &["queued", "downloading", "verifying"],
        "cancelled",
        None,
        None,
        None,
    )
    .await
    .map_err(ApiError::database)?;
    get_download_job(State(state), Path(job_id)).await
}

async fn list_validation_cases(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ValidationCaseView>>> {
    Ok(Json(
        ballast_storage::list_validation_cases(&state.database)
            .await
            .map_err(ApiError::database)?
            .into_iter()
            .map(ValidationCaseView::from)
            .collect(),
    ))
}

async fn get_validation_case(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> ApiResult<Json<ValidationCaseView>> {
    Ok(Json(
        ballast_storage::get_validation_case(&state.database, case_id)
            .await
            .map_err(ApiError::database)?
            .map(ValidationCaseView::from)
            .ok_or_else(|| ApiError::not_found("validation_case_not_found"))?,
    ))
}

async fn update_validation_case(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(input): Json<CaseUpdateInput>,
) -> ApiResult<Json<ValidationCaseView>> {
    let target = positive_decimal(&input.target_notional_usd, "target_notional_usd")?;
    let participation = positive_decimal(&input.max_participation_rate, "max_participation_rate")?;
    if participation > Decimal::ONE {
        return Err(ApiError::validation(
            "participation_rate_out_of_range",
            json!({ "maximum": "1" }),
        ));
    }
    if input.evaluation_sessions <= 0 || input.evaluation_sessions > 200 {
        return Err(ApiError::validation(
            "evaluation_sessions_out_of_range",
            json!({ "maximum": 200 }),
        ));
    }
    let updated = ballast_storage::update_validation_case(
        &state.database,
        case_id,
        ValidationCaseUpdate {
            name: input.name.trim().to_owned(),
            target_notional_usd: target,
            max_participation_rate: participation,
            evaluation_sessions: input.evaluation_sessions,
        },
    )
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("validation_case_not_found"))?;
    Ok(Json(updated.into()))
}

async fn create_validation_run(
    State(state): State<AppState>,
    Json(input): Json<RunInput>,
) -> ApiResult<(StatusCode, Json<StoredValidationRun>)> {
    if input.strategy_version_ids.len() != 4 {
        return Err(ApiError::validation(
            "four_strategy_versions_required",
            json!({ "required": 4 }),
        ));
    }
    let case = ballast_storage::get_validation_case(&state.database, input.case_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("validation_case_not_found"))?;
    let configs = load_strategy_configs(&state, &input.strategy_version_ids).await?;
    require_candidate_set(&configs)?;
    let required_manifests = i64::from(case.warmup_sessions + case.evaluation_sessions);
    let manifests =
        ballast_storage::list_daily_manifests(&state.database, &case.symbol, required_manifests)
            .await
            .map_err(ApiError::database)?;
    if manifests.len() < usize::try_from(required_manifests).unwrap_or(usize::MAX) {
        return Err(ApiError::conflict("research_dataset_incomplete"));
    }
    let run =
        ballast_storage::create_validation_run(&state.database, &case, input.strategy_version_ids)
            .await
            .map_err(ApiError::database)?;
    tokio::spawn(run_validation_job(state, run.id));
    Ok((StatusCode::ACCEPTED, Json(run)))
}

async fn list_validation_runs(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<StoredValidationRun>>> {
    Ok(Json(
        ballast_storage::list_validation_runs(&state.database)
            .await
            .map_err(ApiError::database)?,
    ))
}

async fn get_validation_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<StoredValidationRun>> {
    Ok(Json(
        ballast_storage::get_validation_run(&state.database, run_id)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::not_found("validation_run_not_found"))?,
    ))
}

async fn cancel_validation_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<StoredValidationRun>> {
    ballast_storage::set_validation_run_state(
        &state.database,
        run_id,
        &["queued", "preparing", "running"],
        "cancelled",
        None,
        None,
        None,
    )
    .await
    .map_err(ApiError::database)?;
    get_validation_run(State(state), Path(run_id)).await
}

async fn get_validation_report(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let run = ballast_storage::get_validation_run(&state.database, run_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("validation_run_not_found"))?;
    Ok(Json(run.report.ok_or_else(|| {
        ApiError::conflict("validation_report_not_ready")
    })?))
}

async fn create_validation_decision(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(input): Json<DecisionInput>,
) -> ApiResult<(StatusCode, Json<StoredValidationDecision>)> {
    if !matches!(input.decision.as_str(), "validated" | "rejected") {
        return Err(ApiError::validation(
            "validation_decision_invalid",
            json!({}),
        ));
    }
    let note = input.note.as_deref().unwrap_or("").trim();
    if note.chars().count() > 1_000 {
        return Err(ApiError::validation(
            "validation_decision_note_too_long",
            json!({ "maximum": 1000 }),
        ));
    }
    let run = ballast_storage::get_validation_run(&state.database, run_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("validation_run_not_found"))?;
    if run.status != "succeeded" {
        return Err(ApiError::conflict("validation_run_not_succeeded"));
    }
    let decision = ballast_storage::append_validation_decision(
        &state.database,
        run_id,
        &input.decision,
        note,
    )
    .await
    .map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(decision)))
}

async fn list_validation_decisions(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<Vec<StoredValidationDecision>>> {
    Ok(Json(
        ballast_storage::list_validation_decisions(&state.database, run_id)
            .await
            .map_err(ApiError::database)?,
    ))
}

async fn run_download_job(state: AppState, job_id: Uuid) {
    let result = async {
        if !ballast_storage::set_download_job_state(
            &state.database,
            job_id,
            &["queued"],
            "downloading",
            None,
            None,
            None,
        )
        .await?
        {
            return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
        }
        let job = ballast_storage::get_download_job(&state.database, job_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let client = AlpacaClient::from_env()?;
        let bars = client
            .download_bars(&job.symbol, job.start_at, job.end_at)
            .await?;
        let record_count = i64::try_from(bars.len())?;
        if !ballast_storage::set_download_job_state(
            &state.database,
            job_id,
            &["downloading"],
            "verifying",
            Some(record_count),
            None,
            None,
        )
        .await?
        {
            return Ok(());
        }
        let manifests =
            ballast_alpaca::store_daily_files(&state.research_data_dir, &job.symbol, &bars).await?;
        let stored = manifests
            .iter()
            .map(|manifest| NewDailyManifest {
                download_job_id: job_id,
                symbol: manifest.symbol.clone(),
                session_date: manifest.session_date,
                schema_name: manifest.schema.clone(),
                record_count: i32::try_from(manifest.record_count).unwrap_or(i32::MAX),
                sha256: manifest.sha256.clone(),
                storage_path: manifest.path.to_string_lossy().into_owned(),
                first_bar_at: manifest.first_bar_at,
                last_bar_at: manifest.last_bar_at,
            })
            .collect::<Vec<_>>();
        ballast_storage::upsert_daily_manifests(&state.database, &stored).await?;
        ballast_storage::set_download_job_state(
            &state.database,
            job_id,
            &["verifying"],
            "completed",
            Some(record_count),
            Some(i32::try_from(manifests.len()).unwrap_or(i32::MAX)),
            None,
        )
        .await?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::error!(job_id = %job_id, %error, "research download job failed");
        let _ = ballast_storage::set_download_job_state(
            &state.database,
            job_id,
            &["queued", "downloading", "verifying"],
            "failed",
            None,
            None,
            Some(download_error_code(&error)),
        )
        .await;
    }
}

async fn run_validation_job(state: AppState, run_id: Uuid) {
    let result = async {
        if !ballast_storage::set_validation_run_state(
            &state.database,
            run_id,
            &["queued"],
            "preparing",
            None,
            None,
            None,
        )
        .await?
        {
            return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
        }
        let run = ballast_storage::get_validation_run(&state.database, run_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        // Replay uses the immutable case snapshot taken at run creation, not later edits.
        let research_case = research_case_from_snapshot(&run.case_snapshot)?;
        let configs = load_strategy_configs_internal(&state, &run.strategy_version_ids).await?;
        let limit = i64::try_from(research_case.warmup_sessions + research_case.evaluation_sessions)?;
        let mut manifests = ballast_storage::list_daily_manifests(
            &state.database,
            &research_case.symbol,
            limit,
        )
        .await?;
        manifests.reverse();
        let mut bars = Vec::new();
        for manifest in manifests {
            let source = DailyManifest {
                provider: manifest.provider,
                feed: "iex".to_owned(),
                symbol: manifest.symbol,
                session_date: manifest.session_date,
                schema: manifest.schema_name,
                record_count: usize::try_from(manifest.record_count)?,
                sha256: manifest.sha256,
                path: PathBuf::from(manifest.storage_path),
                first_bar_at: manifest.first_bar_at,
                last_bar_at: manifest.last_bar_at,
            };
            bars.extend(ballast_alpaca::read_manifest_file(&source).await?);
        }
        if !ballast_storage::set_validation_run_state(
            &state.database,
            run_id,
            &["preparing"],
            "running",
            None,
            None,
            None,
        )
        .await?
        {
            return Ok(());
        }
        let report = ballast_research::validate(research_case, configs, bars)?;
        let report_value = serde_json::to_value(report)?;
        let encoded = serde_json::to_vec(&report_value)?;
        let hash = hex::encode(Sha256::digest(&encoded));
        ballast_storage::replace_session_results(&state.database, run_id, &report_value).await?;
        ballast_storage::set_validation_run_state(
            &state.database,
            run_id,
            &["running"],
            "succeeded",
            Some(&report_value),
            Some(&hash),
            None,
        )
        .await?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::error!(run_id = %run_id, %error, "validation run failed");
        let _ = ballast_storage::set_validation_run_state(
            &state.database,
            run_id,
            &["queued", "preparing", "running"],
            "failed",
            None,
            None,
            Some("validation_failed"),
        )
        .await;
    }
}

async fn load_strategy_configs(state: &AppState, ids: &[Uuid]) -> ApiResult<Vec<StrategyConfig>> {
    load_strategy_configs_internal(state, ids)
        .await
        .map_err(|error| {
            tracing::error!(%error, "research strategy configuration failed");
            ApiError::validation("research_strategy_invalid", json!({}))
        })
}

async fn load_strategy_configs_internal(
    state: &AppState,
    ids: &[Uuid],
) -> Result<Vec<StrategyConfig>, Box<dyn std::error::Error + Send + Sync>> {
    let mut configs = Vec::with_capacity(ids.len());
    for id in ids {
        let version = ballast_storage::get_strategy_template_version(&state.database, *id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let scope: String =
            sqlx::query_scalar("SELECT scope FROM strategy_templates WHERE id = $1")
                .bind(version.template_id)
                .fetch_one(&state.database)
                .await?;
        if scope != "research" {
            return Err("paper execution strategy cannot be used in research".into());
        }
        configs.push(serde_json::from_value(version.algorithm_config)?);
    }
    Ok(configs)
}

fn require_candidate_set(configs: &[StrategyConfig]) -> ApiResult<()> {
    let mut names = configs.iter().map(StrategyConfig::name).collect::<Vec<_>>();
    names.sort_unstable();
    if names != ["immediate", "pov", "twap", "vwap"] {
        return Err(ApiError::validation(
            "required_candidate_set_mismatch",
            json!({ "required": ["immediate", "twap", "pov", "vwap"] }),
        ));
    }
    Ok(())
}


#[derive(Debug, Deserialize)]
struct CompareQuery {
    ids: String,
}

#[derive(Debug, Deserialize)]
struct ExportQuery {
    format: Option<String>,
    view: Option<String>,
}

#[derive(Debug, Serialize)]
struct CompareRunView {
    run: StoredValidationRun,
    latest_decision: Option<StoredValidationDecision>,
    aggregates: Value,
    case_summary: Value,
}

async fn compare_validation_runs(
    State(state): State<AppState>,
    Query(query): Query<CompareQuery>,
) -> ApiResult<Json<Value>> {
    let ids = parse_run_ids(&query.ids)?;
    if ids.len() < 2 || ids.len() > 8 {
        return Err(ApiError::validation(
            "compare_run_count_out_of_range",
            json!({ "minimum": 2, "maximum": 8 }),
        ));
    }
    let runs = ballast_storage::get_validation_runs_by_ids(&state.database, &ids)
        .await
        .map_err(ApiError::database)?;
    if runs.len() != ids.len() {
        return Err(ApiError::not_found("validation_run_not_found"));
    }
    let decisions = ballast_storage::latest_validation_decisions(&state.database, &ids)
        .await
        .map_err(ApiError::database)?;
    let mut items = Vec::with_capacity(runs.len());
    for run in runs {
        let aggregates = run
            .report
            .as_ref()
            .and_then(|report| report.get("aggregates"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        let case_summary = case_summary_from_snapshot(&run.case_snapshot);
        items.push(CompareRunView {
            latest_decision: decisions.get(&run.id).cloned(),
            aggregates,
            case_summary,
            run,
        });
    }
    Ok(Json(json!({ "items": items })))
}

async fn export_validation_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> ApiResult<Response> {
    let run = ballast_storage::get_validation_run(&state.database, run_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("validation_run_not_found"))?;
    if run.status != "succeeded" {
        return Err(ApiError::conflict("validation_report_not_ready"));
    }
    let report = run
        .report
        .clone()
        .ok_or_else(|| ApiError::conflict("validation_report_not_ready"))?;
    let format = query.format.as_deref().unwrap_or("json");
    match format {
        "json" => {
            let body = serde_json::to_vec_pretty(&json!({
                "run_id": run.id,
                "case_id": run.case_id,
                "case_snapshot": run.case_snapshot,
                "strategy_version_ids": run.strategy_version_ids,
                "data_quality": run.data_quality,
                "report_sha256": run.report_sha256,
                "created_at": run.created_at,
                "completed_at": run.completed_at,
                "report": report,
            }))
            .map_err(|_| ApiError::internal("export_encode_failed"))?;
            Ok(download_response(
                body,
                "application/json; charset=utf-8",
                &format!("validation-run-{}.json", run.id),
            ))
        }
        "csv" => {
            let view = query.view.as_deref().unwrap_or("aggregates");
            let csv = match view {
                "sessions" => sessions_csv(&report)?,
                _ => aggregates_csv(&report)?,
            };
            Ok(download_response(
                csv.into_bytes(),
                "text/csv; charset=utf-8",
                &format!("validation-run-{}-{}.csv", run.id, view),
            ))
        }
        _ => Err(ApiError::validation(
            "export_format_invalid",
            json!({ "allowed": ["json", "csv"] }),
        )),
    }
}

fn download_response(body: Vec<u8>, content_type: &str, filename: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type).unwrap_or(HeaderValue::from_static("text/plain")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    (StatusCode::OK, headers, body).into_response()
}

fn parse_run_ids(value: &str) -> ApiResult<Vec<Uuid>> {
    let mut ids = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        ids.push(
            Uuid::parse_str(part)
                .map_err(|_| ApiError::validation("invalid_run_id", json!({ "run_id": part })))?,
        );
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

fn case_summary_from_snapshot(snapshot: &Value) -> Value {
    json!({
        "id": snapshot.get("id"),
        "name": snapshot.get("name"),
        "version": snapshot.get("version"),
        "symbol": snapshot.get("symbol"),
        "side": snapshot.get("side"),
        "target_notional_usd": snapshot_decimal_text(snapshot, "target_notional_usd"),
        "max_participation_rate": snapshot_decimal_text(snapshot, "max_participation_rate"),
        "warmup_sessions": snapshot.get("warmup_sessions"),
        "evaluation_sessions": snapshot.get("evaluation_sessions"),
        "start_time": snapshot_time_text(snapshot, "start_time"),
        "end_time": snapshot_time_text(snapshot, "end_time"),
    })
}

fn research_case_from_snapshot(
    snapshot: &Value,
) -> Result<ValidationCase, Box<dyn std::error::Error + Send + Sync>> {
    Ok(ValidationCase {
        symbol: required_string(snapshot, "symbol")?,
        side: required_string(snapshot, "side")?,
        target_notional_usd: required_decimal(snapshot, "target_notional_usd")?,
        timezone: required_string(snapshot, "timezone")?,
        start_time: normalize_time_text(&required_string(snapshot, "start_time")?),
        end_time: normalize_time_text(&required_string(snapshot, "end_time")?),
        max_participation_rate: required_decimal(snapshot, "max_participation_rate")?,
        warmup_sessions: required_usize(snapshot, "warmup_sessions")?,
        evaluation_sessions: required_usize(snapshot, "evaluation_sessions")?,
    })
}

fn required_string(
    value: &Value,
    field: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("case snapshot missing {field}").into())
}

fn required_decimal(
    value: &Value,
    field: &str,
) -> Result<Decimal, Box<dyn std::error::Error + Send + Sync>> {
    let raw = value
        .get(field)
        .ok_or_else(|| format!("case snapshot missing {field}"))?;
    if let Some(text) = raw.as_str() {
        return Ok(Decimal::from_str(text)?);
    }
    if let Some(number) = raw.as_f64() {
        return Ok(Decimal::from_str(&number.to_string())?);
    }
    Err(format!("case snapshot field {field} is not a decimal").into())
}

fn required_usize(
    value: &Value,
    field: &str,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .and_then(|number| usize::try_from(number).ok())
        .ok_or_else(|| format!("case snapshot missing {field}").into())
}

fn snapshot_decimal_text(value: &Value, field: &str) -> Value {
    value
        .get(field)
        .map(|item| match item {
            Value::String(text) => json!(text),
            other => json!(other.to_string()),
        })
        .unwrap_or(Value::Null)
}

fn snapshot_time_text(value: &Value, field: &str) -> Value {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(|text| json!(normalize_time_text(text)))
        .unwrap_or(Value::Null)
}

fn normalize_time_text(value: &str) -> String {
    value.split('.').next().unwrap_or(value).to_owned()
}

fn aggregates_csv(report: &Value) -> ApiResult<String> {
    let mut out = String::from(
        "strategy,evaluated_sessions,completed_sessions,violation_count,median_shortfall_bps,p75_shortfall_bps,p95_shortfall_bps,eligible_for_validation\n",
    );
    let aggregates = report
        .get("aggregates")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::internal("export_report_invalid"))?;
    for item in aggregates {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_escape(item.get("strategy").and_then(Value::as_str).unwrap_or("")),
            item.get("evaluated_sessions").and_then(Value::as_u64).unwrap_or(0),
            item.get("completed_sessions").and_then(Value::as_u64).unwrap_or(0),
            item.get("violation_count").and_then(Value::as_u64).unwrap_or(0),
            csv_escape(&value_as_text(item.get("median_shortfall_bps"))),
            csv_escape(&value_as_text(item.get("p75_shortfall_bps"))),
            csv_escape(&value_as_text(item.get("p95_shortfall_bps"))),
            item.get("eligible_for_validation").and_then(Value::as_bool).unwrap_or(false),
        ));
    }
    Ok(out)
}

fn sessions_csv(report: &Value) -> ApiResult<String> {
    let mut out = String::from(
        "date,strategy,completed,target_quantity,filled_quantity,base_implementation_shortfall_bps,constraint_violations\n",
    );
    let sessions = report
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::internal("export_report_invalid"))?;
    for session in sessions {
        let date = session.get("date").and_then(Value::as_str).unwrap_or("");
        let candidates = session
            .get("candidates")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for candidate in candidates {
            let base_is = candidate
                .get("scenarios")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|scenario| scenario.get("scenario").and_then(Value::as_str) == Some("base"))
                .map(|scenario| value_as_text(scenario.get("implementation_shortfall_bps")))
                .unwrap_or_default();
            let violations = candidate
                .get("constraint_violations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("|");
            out.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                csv_escape(date),
                csv_escape(candidate.get("strategy").and_then(Value::as_str).unwrap_or("")),
                candidate.get("completed").and_then(Value::as_bool).unwrap_or(false),
                csv_escape(&value_as_text(candidate.get("target_quantity"))),
                csv_escape(&value_as_text(candidate.get("filled_quantity"))),
                csv_escape(&base_is),
                csv_escape(&violations),
            ));
        }
    }
    Ok(out)
}

fn value_as_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string().trim_matches('"').to_owned(),
        None => String::new(),
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn alpaca_client() -> ApiResult<AlpacaClient> {
    AlpacaClient::from_env().map_err(|error| {
        tracing::warn!(%error, "alpaca credentials unavailable");
        ApiError::conflict("alpaca_credentials_missing")
    })
}

fn map_alpaca_error(error: ballast_alpaca::AlpacaError) -> ApiError {
    tracing::warn!(%error, "alpaca coverage request failed");
    ApiError::internal("alpaca_request_failed")
}

fn validate_symbol(value: &str) -> ApiResult<&str> {
    let value = value.trim();
    if value != "TSLA" {
        return Err(ApiError::validation(
            "v1_research_symbol_must_be_tsla",
            json!({}),
        ));
    }
    Ok(value)
}

fn date_range(
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> ApiResult<(DateTime<Utc>, DateTime<Utc>)> {
    let end = end.unwrap_or_else(|| Utc::now() - Duration::days(1));
    let start = start.unwrap_or_else(|| end - Duration::days(200));
    if end <= start || end - start > Duration::days(370) {
        return Err(ApiError::validation(
            "research_date_range_invalid",
            json!({ "maximum_days": 370 }),
        ));
    }
    Ok((start, end))
}

fn positive_decimal(value: &str, field: &'static str) -> ApiResult<Decimal> {
    let parsed = Decimal::from_str(value)
        .map_err(|_| ApiError::validation("invalid_decimal", json!({ "field": field })))?;
    if parsed <= Decimal::ZERO {
        return Err(ApiError::validation(
            "decimal_must_be_positive",
            json!({ "field": field }),
        ));
    }
    Ok(parsed)
}

fn download_error_code(error: &Box<dyn std::error::Error + Send + Sync>) -> &'static str {
    let message = error.to_string();
    if message.contains("ALPACA_API") {
        "alpaca_credentials_missing"
    } else if message.contains("alpaca returned") {
        "alpaca_api_error"
    } else if message.contains("research data file") {
        "research_storage_error"
    } else {
        "research_download_failed"
    }
}
