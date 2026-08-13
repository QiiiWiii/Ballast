#![forbid(unsafe_code)]

use sqlx::postgres::PgPoolOptions;

mod historical_repository;
mod instrument_repository;
mod live_repository;
mod order_repository;
mod strategy_repository;
mod task_repository;

pub use historical_repository::{
    HistoricalCandle, HistoricalTrade, NewHistoricalBackfill, StoredHistoricalBackfill,
    create_or_get_historical_backfill, get_historical_backfill, list_historical_candles,
    list_historical_trades, mark_historical_backfill_failed, mark_historical_backfill_running,
    persist_candle_batch, persist_trade_batch,
};
pub use instrument_repository::{
    StoredInstrument, get_instrument, get_instrument_by_key, list_instruments,
    list_instruments_page, upsert_instruments,
};
pub use live_repository::{
    LocalOpenOrderSummary, NewAccountSnapshot, StoredAccount, StoredAccountReconciliation,
    StoredAccountSnapshot, StoredReconciliationDifference, StoredReconciliationRun,
    StoredRiskDecision, StoredTaskApproval, StoredWsTicket, approve_task, consume_ws_ticket,
    create_ws_ticket, list_accounts, list_recent_reconciliation_runs,
    list_reconciliation_differences, list_risk_decisions, list_task_approvals,
    record_account_reconciliation, record_failed_account_reconciliation, reject_task,
};
pub use order_repository::{
    ChildOrderStateUpdate, ClaimedChildOrderReconciliation, NewChildOrder, StoredChildOrder,
    claim_child_orders_for_reconciliation, compare_and_set_child_order_state,
    complete_child_order_reconciliation, create_or_get_child_order,
    fail_child_order_reconciliation, get_child_order_by_client_order_id, list_account_open_orders,
    suspend_child_order_reconciliation,
};
pub use strategy_repository::{
    NewStrategyTemplate, NewStrategyTemplateVersion, StoredStrategyTemplate,
    StoredStrategyTemplateVersion, archive_strategy_template, create_strategy_template,
    create_strategy_template_version, get_strategy_template, get_strategy_template_version,
    list_strategy_template_versions, list_strategy_templates,
};
pub use task_repository::{
    NewExecutionTask, SliceRecord, StoredExecutionEvent, StoredExecutionSlice, StoredExecutionTask,
    cancel_task, claim_runnable_tasks, create_task, defer_task_tick, get_task, list_events_after,
    list_runnable_tasks, list_slices, list_task_events, list_tasks, mark_task_state,
    next_slice_sequence, record_slice,
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
