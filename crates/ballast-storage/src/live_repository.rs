use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use uuid::Uuid;

use crate::DatabasePool;
use crate::alert_repository::{clear_reconciliation_alert_state, enqueue_reconciliation_alert};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub id: String,
    pub exchange: String,
    pub label: String,
    pub environment: String,
    pub secret_name: String,
    pub enabled: bool,
    pub withdrawals_disabled: bool,
    pub ip_restricted: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTaskApproval {
    pub id: Uuid,
    pub task_id: Uuid,
    pub action: String,
    pub actor_id: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRiskDecision {
    pub sequence: i64,
    pub task_id: Option<Uuid>,
    pub account_id: Option<String>,
    pub stage: String,
    pub decision: String,
    pub code: String,
    pub inputs: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccountSnapshot {
    pub account_id: String,
    pub exchange: String,
    pub balances: Value,
    pub positions: Value,
    pub open_orders: Value,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalOpenOrderSummary {
    pub client_order_id: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredAccountSnapshot {
    pub id: Uuid,
    pub account_id: String,
    pub balances: Value,
    pub positions: Value,
    pub open_orders: Value,
    pub observed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredReconciliationRun {
    pub id: Uuid,
    pub account_id: String,
    pub status: String,
    pub difference_count: i32,
    pub differences: Value,
    pub failure_code: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredReconciliationDifference {
    pub id: Uuid,
    pub reconciliation_run_id: Uuid,
    pub difference_type: String,
    pub external_reference: Option<String>,
    pub expected: Value,
    pub observed: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredAccountReconciliation {
    pub snapshot: StoredAccountSnapshot,
    pub run: StoredReconciliationRun,
    pub differences: Vec<StoredReconciliationDifference>,
}

#[derive(Debug, Clone, Serialize)]
struct NewReconciliationDifference {
    difference_type: &'static str,
    client_order_id: String,
    expected: Value,
    observed: Value,
}

/// Persist one immutable account observation and its comparison with the caller's
/// same-cycle local non-terminal order summary. A repeated call intentionally
/// creates another historical snapshot and run; client order ids must be unique
/// within each input, so a single run never contains duplicate differences.
pub async fn record_account_reconciliation(
    pool: &DatabasePool,
    snapshot: NewAccountSnapshot,
    local_open_orders: &[LocalOpenOrderSummary],
    reconciliation_alerts_enabled: bool,
) -> Result<StoredAccountReconciliation, sqlx::Error> {
    validate_snapshot(&snapshot)?;
    let external_orders = order_states_from_snapshot(&snapshot.open_orders)?;
    let local_orders = local_order_states(local_open_orders)?;
    let new_differences = compare_order_states(&external_orders, &local_orders);

    let mut transaction = pool.begin().await?;
    let account = sqlx::query("SELECT exchange, enabled FROM accounts WHERE id = $1 FOR UPDATE")
        .bind(&snapshot.account_id)
        .fetch_optional(&mut *transaction)
        .await?;
    let Some(account) = account else {
        return Err(protocol_error("reconciliation_account_not_found"));
    };
    let account_exchange: String = account.try_get("exchange")?;
    let account_enabled: bool = account.try_get("enabled")?;
    if !account_enabled {
        return Err(protocol_error("reconciliation_account_disabled"));
    }
    if account_exchange != snapshot.exchange {
        return Err(protocol_error("reconciliation_exchange_mismatch"));
    }

    let snapshot_id = Uuid::now_v7();
    let snapshot_row = sqlx::query(
        r#"
        INSERT INTO account_snapshots (
            id, account_id, balances, positions, open_orders, observed_at
        ) VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(snapshot_id)
    .bind(&snapshot.account_id)
    .bind(&snapshot.balances)
    .bind(&snapshot.positions)
    .bind(&snapshot.open_orders)
    .bind(snapshot.observed_at)
    .fetch_one(&mut *transaction)
    .await?;

    let run_id = Uuid::now_v7();
    let status = if new_differences.is_empty() {
        "matched"
    } else {
        "differences"
    };
    let differences_json = serde_json::to_value(&new_differences)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let difference_count = i32::try_from(new_differences.len())
        .map_err(|_| protocol_error("reconciliation_difference_count_overflow"))?;
    let run_row = sqlx::query(
        r#"
        INSERT INTO reconciliation_runs (
            id, account_id, status, difference_count, differences, completed_at
        ) VALUES ($1, $2, $3, $4, $5, now())
        RETURNING *
        "#,
    )
    .bind(run_id)
    .bind(&snapshot.account_id)
    .bind(status)
    .bind(difference_count)
    .bind(&differences_json)
    .fetch_one(&mut *transaction)
    .await?;

    let mut differences = Vec::with_capacity(new_differences.len());
    for difference in &new_differences {
        let row = sqlx::query(
            r#"
            INSERT INTO reconciliation_differences (
                id, reconciliation_run_id, difference_type,
                external_reference, expected, observed
            ) VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(difference.difference_type)
        .bind(&difference.client_order_id)
        .bind(&difference.expected)
        .bind(&difference.observed)
        .fetch_one(&mut *transaction)
        .await?;
        differences.push(row_to_reconciliation_difference(&row)?);
    }
    let stored = StoredAccountReconciliation {
        snapshot: row_to_account_snapshot(&snapshot_row)?,
        run: row_to_reconciliation_run(&run_row)?,
        differences,
    };
    if new_differences.is_empty() {
        clear_reconciliation_alert_state(&mut transaction, &snapshot.account_id).await?;
    } else if reconciliation_alerts_enabled {
        let difference_types = new_differences
            .iter()
            .map(|difference| difference.difference_type.to_owned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        enqueue_reconciliation_alert(
            &mut transaction,
            &snapshot.account_id,
            &snapshot.exchange,
            run_id,
            &differences_json,
            difference_count,
            &difference_types,
        )
        .await?;
    } else {
        clear_reconciliation_alert_state(&mut transaction, &snapshot.account_id).await?;
    }
    transaction.commit().await?;
    Ok(stored)
}

pub async fn record_failed_account_reconciliation(
    pool: &DatabasePool,
    account_id: &str,
    failure_code: &str,
) -> Result<StoredReconciliationRun, sqlx::Error> {
    if account_id.trim().is_empty() || failure_code.trim().is_empty() {
        return Err(protocol_error("reconciliation_failure_invalid"));
    }
    let row = sqlx::query(
        r#"
        INSERT INTO reconciliation_runs (
            id, account_id, status, difference_count, differences,
            failure_code, completed_at
        )
        SELECT $1, id, 'failed', 0, '[]'::jsonb, $3, now()
        FROM accounts
        WHERE id = $2 AND enabled
        RETURNING *
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(account_id)
    .bind(failure_code)
    .fetch_optional(pool)
    .await?;
    row.as_ref()
        .map(row_to_reconciliation_run)
        .transpose()?
        .ok_or_else(|| protocol_error("reconciliation_account_not_enabled"))
}

pub async fn list_recent_reconciliation_runs(
    pool: &DatabasePool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<StoredReconciliationRun>, sqlx::Error> {
    if account_id.trim().is_empty() || limit <= 0 {
        return Err(protocol_error("reconciliation_run_query_invalid"));
    }
    let rows = sqlx::query(
        r#"
        SELECT * FROM reconciliation_runs
        WHERE account_id = $1
        ORDER BY started_at DESC, id DESC
        LIMIT $2
        "#,
    )
    .bind(account_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_reconciliation_run).collect()
}

pub async fn list_reconciliation_differences(
    pool: &DatabasePool,
    reconciliation_run_id: Uuid,
) -> Result<Vec<StoredReconciliationDifference>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM reconciliation_differences
        WHERE reconciliation_run_id = $1
        ORDER BY difference_type, external_reference, id
        "#,
    )
    .bind(reconciliation_run_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_reconciliation_difference).collect()
}

pub async fn list_accounts(pool: &DatabasePool) -> Result<Vec<StoredAccount>, sqlx::Error> {
    let rows = sqlx::query("SELECT * FROM accounts ORDER BY exchange, lower(label)")
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_account).collect()
}

pub async fn list_task_approvals(
    pool: &DatabasePool,
    task_id: Option<Uuid>,
) -> Result<Vec<StoredTaskApproval>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM task_approvals
        WHERE $1::uuid IS NULL OR task_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_approval).collect()
}

pub async fn approve_task(
    pool: &DatabasePool,
    task_id: Uuid,
    actor_id: &str,
) -> Result<bool, sqlx::Error> {
    decide_task(pool, task_id, actor_id, "approved", None).await
}

pub async fn reject_task(
    pool: &DatabasePool,
    task_id: Uuid,
    actor_id: &str,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    decide_task(pool, task_id, actor_id, "rejected", Some(reason)).await
}

async fn decide_task(
    pool: &DatabasePool,
    task_id: Uuid,
    actor_id: &str,
    action: &'static str,
    reason: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let requester: Option<String> = sqlx::query_scalar(
        "SELECT requested_by FROM execution_tasks WHERE id = $1 AND status = 'pending_approval' FOR UPDATE",
    )
    .bind(task_id)
    .fetch_optional(&mut *transaction)
    .await?
    .flatten();
    let Some(requester) = requester else {
        transaction.rollback().await?;
        return Ok(false);
    };
    if requester == actor_id {
        return Err(sqlx::Error::Protocol("self_approval_forbidden".into()));
    }
    let next_status = if action == "approved" {
        "scheduled"
    } else {
        "rejected"
    };
    sqlx::query(
        r#"
        UPDATE execution_tasks
        SET status = $2,
            approved_by = CASE WHEN $2 = 'scheduled' THEN $3 ELSE approved_by END,
            approved_at = CASE WHEN $2 = 'scheduled' THEN now() ELSE approved_at END,
            rejected_by = CASE WHEN $2 = 'rejected' THEN $3 ELSE rejected_by END,
            rejected_at = CASE WHEN $2 = 'rejected' THEN now() ELSE rejected_at END,
            rejection_reason = CASE WHEN $2 = 'rejected' THEN $4 ELSE rejection_reason END,
            version = version + 1,
            updated_at = now()
        WHERE id = $1 AND status = 'pending_approval'
        "#,
    )
    .bind(task_id)
    .bind(next_status)
    .bind(actor_id)
    .bind(reason)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO task_approvals (id, task_id, action, actor_id, reason) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(task_id)
    .bind(action)
    .bind(actor_id)
    .bind(reason)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO execution_events (event_id, task_id, event_type, payload) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(task_id)
    .bind(if action == "approved" { "task_approved" } else { "task_rejected" })
    .bind(serde_json::json!({ "actor_id": actor_id, "reason": reason }))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(true)
}

pub async fn list_risk_decisions(
    pool: &DatabasePool,
    limit: i64,
) -> Result<Vec<StoredRiskDecision>, sqlx::Error> {
    let rows = sqlx::query("SELECT * FROM risk_decisions ORDER BY sequence DESC LIMIT $1")
        .bind(limit)
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_risk_decision).collect()
}

fn row_to_account(row: &sqlx::postgres::PgRow) -> Result<StoredAccount, sqlx::Error> {
    Ok(StoredAccount {
        id: row.try_get("id")?,
        exchange: row.try_get("exchange")?,
        label: row.try_get("label")?,
        environment: row.try_get("environment")?,
        secret_name: row.try_get("secret_name")?,
        enabled: row.try_get("enabled")?,
        withdrawals_disabled: row.try_get("withdrawals_disabled")?,
        ip_restricted: row.try_get("ip_restricted")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_approval(row: &sqlx::postgres::PgRow) -> Result<StoredTaskApproval, sqlx::Error> {
    Ok(StoredTaskApproval {
        id: row.try_get("id")?,
        task_id: row.try_get("task_id")?,
        action: row.try_get("action")?,
        actor_id: row.try_get("actor_id")?,
        reason: row.try_get("reason")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_risk_decision(row: &sqlx::postgres::PgRow) -> Result<StoredRiskDecision, sqlx::Error> {
    Ok(StoredRiskDecision {
        sequence: row.try_get("sequence")?,
        task_id: row.try_get("task_id")?,
        account_id: row.try_get("account_id")?,
        stage: row.try_get("stage")?,
        decision: row.try_get("decision")?,
        code: row.try_get("code")?,
        inputs: row.try_get("inputs")?,
        created_at: row.try_get("created_at")?,
    })
}

fn validate_snapshot(snapshot: &NewAccountSnapshot) -> Result<(), sqlx::Error> {
    if snapshot.account_id.trim().is_empty() || snapshot.exchange.trim().is_empty() {
        return Err(protocol_error("reconciliation_snapshot_scope_invalid"));
    }
    if !snapshot.balances.is_array()
        || !snapshot.positions.is_array()
        || !snapshot.open_orders.is_array()
    {
        return Err(protocol_error("reconciliation_snapshot_shape_invalid"));
    }
    reject_binary_floats(&snapshot.balances)?;
    reject_binary_floats(&snapshot.positions)?;
    reject_binary_floats(&snapshot.open_orders)?;
    validate_decimal_strings(&snapshot.balances)?;
    validate_decimal_strings(&snapshot.positions)?;
    validate_decimal_strings(&snapshot.open_orders)?;
    Ok(())
}

fn reject_binary_floats(value: &Value) -> Result<(), sqlx::Error> {
    match value {
        Value::Number(number) if number.is_f64() => {
            Err(protocol_error("reconciliation_binary_float_forbidden"))
        }
        Value::Array(items) => {
            for item in items {
                reject_binary_floats(item)?;
            }
            Ok(())
        }
        Value::Object(fields) => {
            for item in fields.values() {
                reject_binary_floats(item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_decimal_strings(value: &Value) -> Result<(), sqlx::Error> {
    const DECIMAL_FIELDS: &[&str] = &[
        "total",
        "available",
        "quantity",
        "entry_price",
        "mark_price",
        "unrealized_pnl",
        "filled_quantity",
        "average_price",
    ];
    match value {
        Value::Array(items) => {
            for item in items {
                validate_decimal_strings(item)?;
            }
        }
        Value::Object(fields) => {
            for (field, item) in fields {
                if DECIMAL_FIELDS.contains(&field.as_str()) && !item.is_null() && !item.is_string()
                {
                    return Err(protocol_error("reconciliation_decimal_string_required"));
                }
                if DECIMAL_FIELDS.contains(&field.as_str()) {
                    if let Some(value) = item.as_str() {
                        rust_decimal::Decimal::from_str(value)
                            .map_err(|_| protocol_error("reconciliation_decimal_string_invalid"))?;
                    }
                }
                validate_decimal_strings(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn order_states_from_snapshot(
    open_orders: &Value,
) -> Result<BTreeMap<String, String>, sqlx::Error> {
    let orders = open_orders
        .as_array()
        .ok_or_else(|| protocol_error("reconciliation_open_orders_invalid"))?;
    let mut states = BTreeMap::new();
    for order in orders {
        let fields = order
            .as_object()
            .ok_or_else(|| protocol_error("reconciliation_external_order_invalid"))?;
        let client_order_id = required_order_field(fields, "client_order_id")?;
        let state = required_order_field(fields, "state")?;
        validate_non_terminal_state(state)?;
        if states
            .insert(client_order_id.to_owned(), state.to_owned())
            .is_some()
        {
            return Err(protocol_error(
                "reconciliation_external_client_order_id_duplicate",
            ));
        }
    }
    Ok(states)
}

fn local_order_states(
    orders: &[LocalOpenOrderSummary],
) -> Result<BTreeMap<String, String>, sqlx::Error> {
    let mut states = BTreeMap::new();
    for order in orders {
        if order.client_order_id.trim().is_empty() {
            return Err(protocol_error(
                "reconciliation_local_client_order_id_invalid",
            ));
        }
        validate_non_terminal_state(&order.state)?;
        if states
            .insert(order.client_order_id.clone(), order.state.clone())
            .is_some()
        {
            return Err(protocol_error(
                "reconciliation_local_client_order_id_duplicate",
            ));
        }
    }
    Ok(states)
}

fn required_order_field<'a>(
    fields: &'a serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<&'a str, sqlx::Error> {
    let value = fields
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| protocol_error("reconciliation_external_order_field_invalid"))?;
    Ok(value)
}

fn validate_non_terminal_state(state: &str) -> Result<(), sqlx::Error> {
    if matches!(
        state,
        "submission_pending"
            | "submission_unknown"
            | "open"
            | "partially_filled"
            | "cancel_pending"
    ) {
        Ok(())
    } else {
        Err(protocol_error("reconciliation_order_state_invalid"))
    }
}

fn compare_order_states(
    external: &BTreeMap<String, String>,
    local: &BTreeMap<String, String>,
) -> Vec<NewReconciliationDifference> {
    let mut differences = Vec::new();
    let client_order_ids: BTreeSet<_> = external.keys().chain(local.keys()).collect();
    for client_order_id in client_order_ids {
        match (external.get(client_order_id), local.get(client_order_id)) {
            (Some(external_state), None) => differences.push(NewReconciliationDifference {
                difference_type: "external_order_missing_locally",
                client_order_id: client_order_id.clone(),
                expected: Value::Null,
                observed: serde_json::json!({ "state": external_state }),
            }),
            (None, Some(local_state)) => differences.push(NewReconciliationDifference {
                difference_type: "local_open_order_missing_externally",
                client_order_id: client_order_id.clone(),
                expected: serde_json::json!({ "state": local_state }),
                observed: Value::Null,
            }),
            (Some(external_state), Some(local_state)) if local_state != external_state => {
                differences.push(NewReconciliationDifference {
                    difference_type: "order_state_mismatch",
                    client_order_id: client_order_id.clone(),
                    expected: serde_json::json!({ "state": local_state }),
                    observed: serde_json::json!({ "state": external_state }),
                });
            }
            _ => {}
        }
    }
    differences
}

fn row_to_account_snapshot(
    row: &sqlx::postgres::PgRow,
) -> Result<StoredAccountSnapshot, sqlx::Error> {
    Ok(StoredAccountSnapshot {
        id: row.try_get("id")?,
        account_id: row.try_get("account_id")?,
        balances: row.try_get("balances")?,
        positions: row.try_get("positions")?,
        open_orders: row.try_get("open_orders")?,
        observed_at: row.try_get("observed_at")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_reconciliation_run(
    row: &sqlx::postgres::PgRow,
) -> Result<StoredReconciliationRun, sqlx::Error> {
    Ok(StoredReconciliationRun {
        id: row.try_get("id")?,
        account_id: row.try_get("account_id")?,
        status: row.try_get("status")?,
        difference_count: row.try_get("difference_count")?,
        differences: row.try_get("differences")?,
        failure_code: row.try_get("failure_code")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
    })
}

fn row_to_reconciliation_difference(
    row: &sqlx::postgres::PgRow,
) -> Result<StoredReconciliationDifference, sqlx::Error> {
    Ok(StoredReconciliationDifference {
        id: row.try_get("id")?,
        reconciliation_run_id: row.try_get("reconciliation_run_id")?,
        difference_type: row.try_get("difference_type")?,
        external_reference: row.try_get("external_reference")?,
        expected: row.try_get("expected")?,
        observed: row.try_get("observed")?,
        created_at: row.try_get("created_at")?,
    })
}

fn protocol_error(message: &'static str) -> sqlx::Error {
    sqlx::Error::Protocol(message.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredWsTicket {
    pub subject: String,
    pub roles: Vec<String>,
    pub expires_at: DateTime<Utc>,
}

pub async fn create_ws_ticket(
    pool: &DatabasePool,
    ticket_hash: &str,
    subject: &str,
    roles: &[String],
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let roles_json =
        serde_json::to_value(roles).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    sqlx::query(
        r#"
        INSERT INTO ws_tickets (ticket_hash, subject, roles, expires_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(ticket_hash)
    .bind(subject)
    .bind(roles_json)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically consume a non-expired ticket. Returns None if missing, expired, or already used.
pub async fn consume_ws_ticket(
    pool: &DatabasePool,
    ticket_hash: &str,
) -> Result<Option<StoredWsTicket>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        UPDATE ws_tickets
        SET consumed_at = now()
        WHERE ticket_hash = $1
          AND consumed_at IS NULL
          AND expires_at > now()
        RETURNING subject, roles, expires_at
        "#,
    )
    .bind(ticket_hash)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let roles_value: Value = row.try_get("roles")?;
    let roles = match roles_value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        Value::String(text) => vec![text],
        _ => Vec::new(),
    };
    Ok(Some(StoredWsTicket {
        subject: row.try_get("subject")?,
        roles,
        expires_at: row.try_get("expires_at")?,
    }))
}
