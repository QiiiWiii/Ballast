#![forbid(unsafe_code)]

use sqlx::postgres::PgPoolOptions;

mod instrument_repository;
mod live_repository;
mod strategy_repository;
mod task_repository;

pub use instrument_repository::{
    StoredInstrument, get_instrument, list_instruments, upsert_instruments,
};
pub use live_repository::{
    StoredAccount, StoredRiskDecision, StoredTaskApproval, approve_task, list_accounts,
    list_risk_decisions, list_task_approvals, reject_task,
};
pub use strategy_repository::{
    NewStrategyTemplate, NewStrategyTemplateVersion, StoredStrategyTemplate,
    StoredStrategyTemplateVersion, archive_strategy_template, create_strategy_template,
    create_strategy_template_version, get_strategy_template, get_strategy_template_version,
    list_strategy_template_versions, list_strategy_templates,
};
pub use task_repository::{
    NewExecutionTask, SliceRecord, StoredExecutionEvent, StoredExecutionSlice, StoredExecutionTask,
    cancel_task, claim_runnable_tasks, create_task, get_task, list_events_after,
    list_runnable_tasks, list_slices, list_tasks, mark_task_state, next_slice_sequence,
    record_slice,
};

pub type DatabasePool = sqlx::PgPool;

pub async fn connect(database_url: &str) -> Result<DatabasePool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

pub async fn migrate(pool: &DatabasePool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}

pub async fn ping(pool: &DatabasePool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}
