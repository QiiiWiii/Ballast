use ballast_core::{ContractKind, Exchange, Instrument, InstrumentId, MarketKind};
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::DatabasePool;

#[derive(Debug, Clone)]
pub struct StoredInstrument {
    pub id: Uuid,
    pub instrument: Instrument,
    pub observed_at: DateTime<Utc>,
}

pub async fn upsert_instruments(
    pool: &DatabasePool,
    instruments: &[Instrument],
) -> Result<Vec<StoredInstrument>, sqlx::Error> {
    let observed_at = Utc::now();
    let mut transaction = pool.begin().await?;
    let mut stored = Vec::with_capacity(instruments.len());
    for instrument in instruments {
        let id = upsert_instrument(&mut transaction, instrument, observed_at).await?;
        stored.push(StoredInstrument {
            id,
            instrument: instrument.clone(),
            observed_at,
        });
    }
    transaction.commit().await?;
    Ok(stored)
}

async fn upsert_instrument(
    transaction: &mut Transaction<'_, Postgres>,
    instrument: &Instrument,
    observed_at: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        INSERT INTO instruments (
            id, exchange, market_kind, symbol, exchange_symbol, base_asset, quote_asset,
            settle_asset, contract_kind, contract_size, price_tick, quantity_step,
            minimum_quantity, minimum_notional, maker_fee_rate, taker_fee_rate,
            active, observed_at
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18
        )
        ON CONFLICT (exchange, market_kind, symbol) DO UPDATE SET
            exchange_symbol = EXCLUDED.exchange_symbol,
            base_asset = EXCLUDED.base_asset,
            quote_asset = EXCLUDED.quote_asset,
            settle_asset = EXCLUDED.settle_asset,
            contract_kind = EXCLUDED.contract_kind,
            contract_size = EXCLUDED.contract_size,
            price_tick = EXCLUDED.price_tick,
            quantity_step = EXCLUDED.quantity_step,
            minimum_quantity = EXCLUDED.minimum_quantity,
            minimum_notional = EXCLUDED.minimum_notional,
            maker_fee_rate = EXCLUDED.maker_fee_rate,
            taker_fee_rate = EXCLUDED.taker_fee_rate,
            active = EXCLUDED.active,
            observed_at = EXCLUDED.observed_at,
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(exchange_text(instrument.id.exchange))
    .bind(market_kind_text(instrument.id.market_kind))
    .bind(&instrument.id.symbol)
    .bind(&instrument.exchange_symbol)
    .bind(&instrument.base_asset)
    .bind(&instrument.quote_asset)
    .bind(&instrument.settle_asset)
    .bind(instrument.contract_kind.map(contract_kind_text))
    .bind(instrument.contract_size)
    .bind(instrument.price_tick)
    .bind(instrument.quantity_step)
    .bind(instrument.minimum_quantity)
    .bind(instrument.minimum_notional)
    .bind(instrument.maker_fee_rate)
    .bind(instrument.taker_fee_rate)
    .bind(instrument.active)
    .bind(observed_at)
    .fetch_one(&mut **transaction)
    .await
}

pub async fn list_instruments(pool: &DatabasePool) -> Result<Vec<StoredInstrument>, sqlx::Error> {
    let rows = sqlx::query("SELECT * FROM instruments ORDER BY exchange, market_kind, symbol")
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_instrument).collect()
}

pub async fn get_instrument(
    pool: &DatabasePool,
    id: Uuid,
) -> Result<Option<StoredInstrument>, sqlx::Error> {
    sqlx::query("SELECT * FROM instruments WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_instrument)
        .transpose()
}

fn row_to_instrument(row: &sqlx::postgres::PgRow) -> Result<StoredInstrument, sqlx::Error> {
    let exchange = parse_exchange(row.try_get("exchange")?)?;
    let market_kind = parse_market_kind(row.try_get("market_kind")?)?;
    let contract_kind = row
        .try_get::<Option<&str>, _>("contract_kind")?
        .map(parse_contract_kind)
        .transpose()?;
    let id = InstrumentId::new(exchange, market_kind, row.try_get::<String, _>("symbol")?)
        .map_err(decode_error)?;
    Ok(StoredInstrument {
        id: row.try_get("id")?,
        instrument: Instrument {
            id,
            exchange_symbol: row.try_get("exchange_symbol")?,
            base_asset: row.try_get("base_asset")?,
            quote_asset: row.try_get("quote_asset")?,
            settle_asset: row.try_get("settle_asset")?,
            contract_kind,
            contract_size: row.try_get("contract_size")?,
            price_tick: row.try_get("price_tick")?,
            quantity_step: row.try_get("quantity_step")?,
            minimum_quantity: row.try_get("minimum_quantity")?,
            minimum_notional: row.try_get("minimum_notional")?,
            maker_fee_rate: row.try_get("maker_fee_rate")?,
            taker_fee_rate: row.try_get("taker_fee_rate")?,
            active: row.try_get("active")?,
        },
        observed_at: row.try_get("observed_at")?,
    })
}

pub(crate) const fn exchange_text(value: Exchange) -> &'static str {
    match value {
        Exchange::Binance => "binance",
        Exchange::Okx => "okx",
        Exchange::Bybit => "bybit",
        Exchange::GateIo => "gate_io",
        Exchange::Bitget => "bitget",
    }
}

pub(crate) const fn market_kind_text(value: MarketKind) -> &'static str {
    match value {
        MarketKind::Spot => "spot",
        MarketKind::Perpetual => "perpetual",
    }
}

const fn contract_kind_text(value: ContractKind) -> &'static str {
    match value {
        ContractKind::Linear => "linear",
        ContractKind::Inverse => "inverse",
    }
}

fn parse_exchange(value: &str) -> Result<Exchange, sqlx::Error> {
    match value {
        "binance" => Ok(Exchange::Binance),
        "okx" => Ok(Exchange::Okx),
        "bybit" => Ok(Exchange::Bybit),
        "gate_io" => Ok(Exchange::GateIo),
        "bitget" => Ok(Exchange::Bitget),
        _ => Err(decode_error(format!("unknown exchange: {value}"))),
    }
}

fn parse_market_kind(value: &str) -> Result<MarketKind, sqlx::Error> {
    match value {
        "spot" => Ok(MarketKind::Spot),
        "perpetual" => Ok(MarketKind::Perpetual),
        _ => Err(decode_error(format!("unknown market kind: {value}"))),
    }
}

fn parse_contract_kind(value: &str) -> Result<ContractKind, sqlx::Error> {
    match value {
        "linear" => Ok(ContractKind::Linear),
        "inverse" => Ok(ContractKind::Inverse),
        _ => Err(decode_error(format!("unknown contract kind: {value}"))),
    }
}

fn decode_error(error: impl std::fmt::Display + Send + Sync + 'static) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        error.to_string(),
    )))
}
