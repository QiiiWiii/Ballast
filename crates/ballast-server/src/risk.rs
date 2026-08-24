use ballast_storage::{
    DatabasePool, StoredAccountSnapshot, StoredKillSwitch, StoredRiskLimit, count_active_tasks,
    get_latest_account_snapshot, list_kill_switches, list_risk_limits, record_risk_decision,
};
use chrono::Utc;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RiskStage {
    Create,
    Approve,
    Submit,
}

impl RiskStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Approve => "approve",
            Self::Submit => "submit",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RiskContext {
    pub task_id: Option<Uuid>,
    pub account_id: String,
    pub exchange: String,
    pub instrument_id: Uuid,
    pub backend: String,
    pub order_notional: Decimal,
    pub task_notional: Decimal,
    pub market_age_ms: i64,
    pub account_age_ms: i64,
    pub max_slippage_bps: i32,
    pub daily_notional: Decimal,
    pub net_exposure: Decimal,
    pub native_algo_orders: i32,
    pub native_duration_seconds: i64,
}

#[derive(Debug, Error)]
pub(crate) enum RiskError {
    #[error("risk denied: {0}")]
    Denied(&'static str),
    #[error("risk database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub(crate) async fn acquire_submit_lock(
    database: &DatabasePool,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
    ballast_storage::acquire_live_risk_lock(database).await
}

pub(crate) async fn check(
    database: &DatabasePool,
    stage: RiskStage,
    context: &RiskContext,
) -> Result<(), RiskError> {
    let inputs = context.inputs().map_err(RiskError::Database)?;
    let switches = list_kill_switches(database).await?;
    for switch in switches.iter().filter(|switch| switch.enabled) {
        if switch_matches(switch, context) {
            return deny(database, stage, context, inputs, "kill_switch_enabled").await;
        }
    }

    let limits = list_risk_limits(database).await?;
    let active_limits: Vec<&StoredRiskLimit> = limits
        .iter()
        .filter(|limit| limit_matches(limit, context))
        .collect();
    if active_limits.len() < 3 {
        return deny(
            database,
            stage,
            context,
            inputs,
            "risk_limit_not_configured",
        )
        .await;
    }

    let active_tasks = count_active_tasks(database, &context.account_id).await?;
    let daily_notional = daily_allowed_notional(database, &context.account_id).await?;
    for limit in active_limits {
        if !allowed(&limit.allowed_exchanges, &context.exchange)
            || !allowed(&limit.allowed_accounts, &context.account_id)
            || !allowed(
                &limit.allowed_instruments,
                &context.instrument_id.to_string(),
            )
            || !allowed(&limit.allowed_backends, &context.backend)
        {
            return deny(database, stage, context, inputs, "risk_scope_not_allowed").await;
        }
        if context.order_notional > limit.max_order_notional {
            return deny(
                database,
                stage,
                context,
                inputs,
                "max_order_notional_exceeded",
            )
            .await;
        }
        if context.task_notional > limit.max_task_notional {
            return deny(
                database,
                stage,
                context,
                inputs,
                "max_task_notional_exceeded",
            )
            .await;
        }
        if daily_notional + context.daily_notional > limit.max_daily_notional {
            return deny(
                database,
                stage,
                context,
                inputs,
                "max_daily_notional_exceeded",
            )
            .await;
        }
        if active_tasks > i64::from(limit.max_active_tasks) {
            return deny(
                database,
                stage,
                context,
                inputs,
                "max_active_tasks_exceeded",
            )
            .await;
        }
        if context.max_slippage_bps > limit.max_slippage_bps {
            return deny(database, stage, context, inputs, "max_slippage_exceeded").await;
        }
        if context.market_age_ms > limit.max_market_age_ms {
            return deny(database, stage, context, inputs, "market_too_old").await;
        }
        if context.account_age_ms > limit.max_account_age_ms {
            return deny(database, stage, context, inputs, "account_snapshot_too_old").await;
        }
        if context.net_exposure > limit.max_net_exposure {
            return deny(
                database,
                stage,
                context,
                inputs,
                "max_net_exposure_exceeded",
            )
            .await;
        }
        if context.backend == "venue_native_algo" {
            if context.native_algo_orders > limit.max_native_algo_orders {
                return deny(
                    database,
                    stage,
                    context,
                    inputs,
                    "max_native_algo_orders_exceeded",
                )
                .await;
            }
            if context.native_duration_seconds > limit.max_native_duration_seconds {
                return deny(
                    database,
                    stage,
                    context,
                    inputs,
                    "max_native_duration_exceeded",
                )
                .await;
            }
        }
    }

    record_risk_decision(
        database,
        audited_task_id(stage, context),
        Some(&context.account_id),
        stage.as_str(),
        "allowed",
        "risk_checks_passed",
        inputs,
    )
    .await?;
    Ok(())
}

async fn deny(
    database: &DatabasePool,
    stage: RiskStage,
    context: &RiskContext,
    inputs: Value,
    code: &'static str,
) -> Result<(), RiskError> {
    record_risk_decision(
        database,
        audited_task_id(stage, context),
        Some(&context.account_id),
        stage.as_str(),
        "denied",
        code,
        inputs,
    )
    .await?;
    Err(RiskError::Denied(code))
}

fn audited_task_id(stage: RiskStage, context: &RiskContext) -> Option<Uuid> {
    match stage {
        RiskStage::Create => None,
        RiskStage::Approve | RiskStage::Submit => context.task_id,
    }
}

fn switch_matches(switch: &StoredKillSwitch, context: &RiskContext) -> bool {
    match switch.scope_type.as_str() {
        "global" => switch.scope_id == "global",
        "exchange" => switch.scope_id == context.exchange,
        "account" => switch.scope_id == context.account_id,
        _ => false,
    }
}

fn limit_matches(limit: &StoredRiskLimit, context: &RiskContext) -> bool {
    match limit.scope_type.as_str() {
        "global" => limit.scope_id == "global",
        "exchange" => limit.scope_id == context.exchange,
        "account" => limit.scope_id == context.account_id,
        _ => false,
    }
}

fn allowed(values: &Value, expected: &str) -> bool {
    values
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(expected)))
}

async fn daily_allowed_notional(
    database: &DatabasePool,
    account_id: &str,
) -> Result<Decimal, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT COALESCE(
            SUM(COALESCE(NULLIF(inputs->>'task_notional', '')::numeric, 0)),
            0
        )
        FROM risk_decisions
        WHERE account_id = $1
          AND stage = 'submit'
          AND decision = 'allowed'
          AND created_at >= date_trunc('day', now())
        "#,
    )
    .bind(account_id)
    .fetch_one(database)
    .await
}

impl RiskContext {
    fn inputs(&self) -> Result<Value, sqlx::Error> {
        Ok(json!({
            "account_id": self.account_id,
            "exchange": self.exchange,
            "instrument_id": self.instrument_id,
            "backend": self.backend,
            "order_notional": self.order_notional.to_string(),
            "task_notional": self.task_notional.to_string(),
            "market_age_ms": self.market_age_ms,
            "account_age_ms": self.account_age_ms,
            "max_slippage_bps": self.max_slippage_bps,
            "daily_notional": self.daily_notional.to_string(),
            "net_exposure": self.net_exposure.to_string(),
            "native_algo_orders": self.native_algo_orders,
            "native_duration_seconds": self.native_duration_seconds,
        }))
    }
}

pub(crate) fn snapshot_exposure(
    snapshot: Option<&StoredAccountSnapshot>,
    instrument_id: &str,
) -> Result<Decimal, &'static str> {
    let Some(snapshot) = snapshot else {
        return Err("account_snapshot_required");
    };
    let positions = snapshot
        .positions
        .as_array()
        .ok_or("account_snapshot_positions_invalid")?;
    let mut exposure = Decimal::ZERO;
    for position in positions {
        let object = position
            .as_object()
            .ok_or("account_snapshot_position_invalid")?;
        let position_instrument = object
            .get("instrument")
            .and_then(Value::as_object)
            .and_then(|instrument| instrument.get("symbol"))
            .and_then(Value::as_str);
        if position_instrument != Some(instrument_id) {
            continue;
        }
        let quantity = object
            .get("quantity")
            .and_then(Value::as_str)
            .ok_or("account_snapshot_position_quantity_invalid")?
            .parse::<Decimal>()
            .map_err(|_| "account_snapshot_position_quantity_invalid")?;
        exposure += quantity.abs();
    }
    Ok(exposure)
}

pub(crate) fn snapshot_age_ms(
    snapshot: Option<&StoredAccountSnapshot>,
) -> Result<i64, &'static str> {
    let snapshot = snapshot.ok_or("account_snapshot_required")?;
    Ok((Utc::now().timestamp_millis() - snapshot.observed_at.timestamp_millis()).max(0))
}

pub(crate) async fn latest_snapshot(
    database: &DatabasePool,
    account_id: &str,
) -> Result<Option<StoredAccountSnapshot>, sqlx::Error> {
    get_latest_account_snapshot(database, account_id).await
}

#[cfg(test)]
mod tests {
    use super::{RiskContext, RiskStage, allowed, audited_task_id};
    use rust_decimal::Decimal;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn allow_lists_are_explicit() {
        assert!(allowed(&json!(["okx"]), "okx"));
        assert!(!allowed(&json!([]), "okx"));
        assert!(!allowed(&json!(["okx"]), "binance"));
    }

    #[test]
    fn create_risk_decisions_do_not_reference_unpersisted_tasks() {
        let context = RiskContext {
            task_id: Some(Uuid::nil()),
            account_id: "account".to_owned(),
            exchange: "okx".to_owned(),
            instrument_id: Uuid::nil(),
            backend: "managed_ioc".to_owned(),
            order_notional: Decimal::ZERO,
            task_notional: Decimal::ZERO,
            market_age_ms: 0,
            account_age_ms: 0,
            max_slippage_bps: 0,
            daily_notional: Decimal::ZERO,
            net_exposure: Decimal::ZERO,
            native_algo_orders: 0,
            native_duration_seconds: 0,
        };
        assert_eq!(audited_task_id(RiskStage::Create, &context), None);
        assert_eq!(
            audited_task_id(RiskStage::Approve, &context),
            context.task_id
        );
    }
}
