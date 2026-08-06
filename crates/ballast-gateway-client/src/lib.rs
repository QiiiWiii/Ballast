#![forbid(unsafe_code)]

use std::str::FromStr;

use ballast_core::{ContractKind, Exchange, Instrument, InstrumentId, MarketKind, Side};
use rust_decimal::Decimal;
use thiserror::Error;
use tonic::{Status, transport::Channel};

pub mod proto {
    tonic::include_proto!("ballast.gateway.v1");
}

pub use proto::market_data_service_client::MarketDataServiceClient;
pub use proto::{
    account_service_client::AccountServiceClient,
    algorithmic_trading_service_client::AlgorithmicTradingServiceClient,
    trading_service_client::TradingServiceClient,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBook {
    pub instrument: InstrumentId,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
    pub exchange_time_ms: i64,
    pub gateway_received_at_ms: i64,
    pub sequence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trade {
    pub event_id: String,
    pub instrument: InstrumentId,
    pub exchange_trade_id: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub taker_side: Side,
    pub exchange_time_ms: i64,
    pub gateway_received_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub exchange: Exchange,
    pub spot: bool,
    pub perpetual_linear: bool,
    pub perpetual_inverse: bool,
    pub fetch_order_book: bool,
    pub watch_order_book: bool,
    pub watch_trades: bool,
}

#[derive(Debug, Clone)]
pub struct GatewayClient {
    market_data: MarketDataServiceClient<Channel>,
    account: AccountServiceClient<Channel>,
    trading: TradingServiceClient<Channel>,
    algorithmic: AlgorithmicTradingServiceClient<Channel>,
}

impl GatewayClient {
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self, GatewayClientError> {
        let endpoint = endpoint.into();
        Ok(Self {
            market_data: MarketDataServiceClient::connect(endpoint.clone()).await?,
            account: AccountServiceClient::connect(endpoint.clone()).await?,
            trading: TradingServiceClient::connect(endpoint.clone()).await?,
            algorithmic: AlgorithmicTradingServiceClient::connect(endpoint).await?,
        })
    }

    pub async fn health(&self) -> Result<proto::HealthResponse, GatewayClientError> {
        let mut client = self.market_data.clone();
        Ok(client.health(proto::HealthRequest {}).await?.into_inner())
    }

    pub async fn capabilities(
        &self,
        exchange: Exchange,
    ) -> Result<Capabilities, GatewayClientError> {
        let mut client = self.market_data.clone();
        let response = client
            .get_capabilities(proto::GetCapabilitiesRequest {
                exchange: exchange_to_proto(exchange),
            })
            .await?
            .into_inner();
        Ok(Capabilities {
            exchange: exchange_from_proto(response.exchange)?,
            spot: response.spot,
            perpetual_linear: response.perpetual_linear,
            perpetual_inverse: response.perpetual_inverse,
            fetch_order_book: response.fetch_order_book,
            watch_order_book: response.watch_order_book,
            watch_trades: response.watch_trades,
        })
    }

    pub async fn list_instruments(
        &self,
        exchange: Exchange,
        market_kind: Option<MarketKind>,
        contract_kind: Option<ContractKind>,
        active_only: bool,
        reload: bool,
    ) -> Result<Vec<Instrument>, GatewayClientError> {
        let mut client = self.market_data.clone();
        let response = client
            .list_instruments(proto::ListInstrumentsRequest {
                exchange: exchange_to_proto(exchange),
                market_kind: market_kind.map_or(0, market_kind_to_proto),
                contract_kind: contract_kind.map_or(0, contract_kind_to_proto),
                active_only,
                reload,
            })
            .await?
            .into_inner();
        response
            .instruments
            .into_iter()
            .map(instrument_from_proto)
            .collect()
    }

    pub async fn get_order_book(
        &self,
        instrument: &InstrumentId,
        depth: u32,
    ) -> Result<OrderBook, GatewayClientError> {
        let mut client = self.market_data.clone();
        let response = client
            .get_order_book(proto::GetOrderBookRequest {
                instrument: Some(instrument_key_to_proto(instrument)),
                depth,
            })
            .await?
            .into_inner();
        order_book_from_proto(response)
    }

    pub async fn watch_trades(
        &self,
        instrument: &InstrumentId,
    ) -> Result<tonic::Streaming<proto::TradeStreamEvent>, GatewayClientError> {
        let mut client = self.market_data.clone();
        Ok(client
            .watch_trades(proto::WatchMarketRequest {
                instrument: Some(instrument_key_to_proto(instrument)),
                depth: 0,
            })
            .await?
            .into_inner())
    }

    pub async fn watch_order_book(
        &self,
        instrument: &InstrumentId,
        depth: u32,
    ) -> Result<tonic::Streaming<proto::OrderBookStreamEvent>, GatewayClientError> {
        let mut client = self.market_data.clone();
        Ok(client
            .watch_order_book(proto::WatchMarketRequest {
                instrument: Some(instrument_key_to_proto(instrument)),
                depth,
            })
            .await?
            .into_inner())
    }

    pub async fn trading_capabilities(
        &self,
        exchange: Exchange,
    ) -> Result<proto::TradingCapabilities, GatewayClientError> {
        let mut client = self.trading.clone();
        Ok(client
            .get_trading_capabilities(proto::GetTradingCapabilitiesRequest {
                exchange: exchange_to_proto(exchange),
            })
            .await?
            .into_inner())
    }

    pub async fn algorithmic_capabilities(
        &self,
        exchange: Exchange,
    ) -> Result<proto::AlgoCapabilities, GatewayClientError> {
        let mut client = self.algorithmic.clone();
        Ok(client
            .get_algo_capabilities(proto::GetAlgoCapabilitiesRequest {
                exchange: exchange_to_proto(exchange),
            })
            .await?
            .into_inner())
    }

    pub async fn account_snapshot(
        &self,
        request: proto::GetAccountSnapshotRequest,
    ) -> Result<proto::AccountSnapshot, GatewayClientError> {
        let mut client = self.account.clone();
        Ok(client.get_account_snapshot(request).await?.into_inner())
    }

    pub async fn place_ioc(
        &self,
        request: proto::PlaceIocOrderRequest,
    ) -> Result<proto::OrderSnapshot, GatewayClientError> {
        let mut client = self.trading.clone();
        Ok(client.place_ioc_order(request).await?.into_inner())
    }

    pub async fn get_order_by_client_id(
        &self,
        request: proto::GetOrderByClientIdRequest,
    ) -> Result<proto::OrderSnapshot, GatewayClientError> {
        let mut client = self.trading.clone();
        Ok(client.get_order_by_client_id(request).await?.into_inner())
    }

    pub async fn submit_algorithm(
        &self,
        request: proto::SubmitAlgoOrderRequest,
    ) -> Result<proto::AlgoOrderSnapshot, GatewayClientError> {
        let mut client = self.algorithmic.clone();
        Ok(client.submit_algo_order(request).await?.into_inner())
    }

    pub async fn get_algorithm(
        &self,
        request: proto::GetAlgoOrderRequest,
    ) -> Result<proto::AlgoOrderSnapshot, GatewayClientError> {
        let mut client = self.algorithmic.clone();
        Ok(client.get_algo_order(request).await?.into_inner())
    }
}

pub fn order_book_from_proto(value: proto::OrderBook) -> Result<OrderBook, GatewayClientError> {
    Ok(OrderBook {
        instrument: instrument_id_from_proto(required(value.instrument, "order_book.instrument")?)?,
        bids: value
            .bids
            .into_iter()
            .map(book_level_from_proto)
            .collect::<Result<_, _>>()?,
        asks: value
            .asks
            .into_iter()
            .map(book_level_from_proto)
            .collect::<Result<_, _>>()?,
        exchange_time_ms: value.exchange_time_ms,
        gateway_received_at_ms: value.gateway_received_at_ms,
        sequence: value.sequence,
    })
}

pub fn trade_from_proto(value: proto::Trade) -> Result<Trade, GatewayClientError> {
    Ok(Trade {
        event_id: required_text(value.event_id, "trade.event_id")?,
        instrument: instrument_id_from_proto(required(value.instrument, "trade.instrument")?)?,
        exchange_trade_id: required_text(value.exchange_trade_id, "trade.exchange_trade_id")?,
        price: parse_positive_decimal(&value.price, "trade.price")?,
        quantity: parse_positive_decimal(&value.quantity, "trade.quantity")?,
        taker_side: side_from_proto(value.taker_side)?,
        exchange_time_ms: value.exchange_time_ms,
        gateway_received_at_ms: value.gateway_received_at_ms,
    })
}

fn instrument_from_proto(value: proto::Instrument) -> Result<Instrument, GatewayClientError> {
    let id = instrument_id_from_proto(required(value.key, "instrument.key")?)?;
    let contract_kind = match id.market_kind {
        MarketKind::Spot => {
            if value.contract_kind != 0 || value.contract_size.is_some() {
                return Err(GatewayClientError::InvalidField("instrument.contract_kind"));
            }
            None
        }
        MarketKind::Perpetual => Some(contract_kind_from_proto(value.contract_kind)?),
    };
    Ok(Instrument {
        id,
        exchange_symbol: required_text(value.exchange_symbol, "instrument.exchange_symbol")?,
        base_asset: required_text(value.base_asset, "instrument.base_asset")?,
        quote_asset: required_text(value.quote_asset, "instrument.quote_asset")?,
        settle_asset: optional_text(value.settle_asset),
        contract_kind,
        contract_size: parse_optional_positive_decimal(
            value.contract_size.as_deref(),
            "instrument.contract_size",
        )?,
        price_tick: parse_positive_decimal(&value.price_tick, "instrument.price_tick")?,
        quantity_step: parse_positive_decimal(&value.quantity_step, "instrument.quantity_step")?,
        minimum_quantity: parse_optional_positive_decimal(
            value.minimum_quantity.as_deref(),
            "instrument.minimum_quantity",
        )?,
        minimum_notional: parse_optional_positive_decimal(
            value.minimum_notional.as_deref(),
            "instrument.minimum_notional",
        )?,
        maker_fee_rate: parse_optional_non_negative_decimal(
            value.maker_fee_rate.as_deref(),
            "instrument.maker_fee_rate",
        )?,
        taker_fee_rate: parse_optional_non_negative_decimal(
            value.taker_fee_rate.as_deref(),
            "instrument.taker_fee_rate",
        )?,
        active: value.active,
    })
}

fn book_level_from_proto(value: proto::BookLevel) -> Result<BookLevel, GatewayClientError> {
    Ok(BookLevel {
        price: parse_positive_decimal(&value.price, "book_level.price")?,
        quantity: parse_positive_decimal(&value.quantity, "book_level.quantity")?,
    })
}

fn instrument_key_to_proto(value: &InstrumentId) -> proto::InstrumentKey {
    proto::InstrumentKey {
        exchange: exchange_to_proto(value.exchange),
        market_kind: market_kind_to_proto(value.market_kind),
        symbol: value.symbol.clone(),
    }
}

fn instrument_id_from_proto(
    value: proto::InstrumentKey,
) -> Result<InstrumentId, GatewayClientError> {
    InstrumentId::new(
        exchange_from_proto(value.exchange)?,
        market_kind_from_proto(value.market_kind)?,
        value.symbol,
    )
    .map_err(|_| GatewayClientError::InvalidField("instrument.symbol"))
}

const fn exchange_to_proto(value: Exchange) -> i32 {
    match value {
        Exchange::Binance => proto::Exchange::Binance as i32,
        Exchange::Okx => proto::Exchange::Okx as i32,
        Exchange::Bybit => proto::Exchange::Bybit as i32,
        Exchange::GateIo => proto::Exchange::GateIo as i32,
        Exchange::Bitget => proto::Exchange::Bitget as i32,
    }
}

fn exchange_from_proto(value: i32) -> Result<Exchange, GatewayClientError> {
    match proto::Exchange::try_from(value).ok() {
        Some(proto::Exchange::Binance) => Ok(Exchange::Binance),
        Some(proto::Exchange::Okx) => Ok(Exchange::Okx),
        Some(proto::Exchange::Bybit) => Ok(Exchange::Bybit),
        Some(proto::Exchange::GateIo) => Ok(Exchange::GateIo),
        Some(proto::Exchange::Bitget) => Ok(Exchange::Bitget),
        _ => Err(GatewayClientError::InvalidField("exchange")),
    }
}

const fn market_kind_to_proto(value: MarketKind) -> i32 {
    match value {
        MarketKind::Spot => proto::MarketKind::Spot as i32,
        MarketKind::Perpetual => proto::MarketKind::Perpetual as i32,
    }
}

fn market_kind_from_proto(value: i32) -> Result<MarketKind, GatewayClientError> {
    match proto::MarketKind::try_from(value).ok() {
        Some(proto::MarketKind::Spot) => Ok(MarketKind::Spot),
        Some(proto::MarketKind::Perpetual) => Ok(MarketKind::Perpetual),
        _ => Err(GatewayClientError::InvalidField("market_kind")),
    }
}

const fn contract_kind_to_proto(value: ContractKind) -> i32 {
    match value {
        ContractKind::Linear => proto::ContractKind::Linear as i32,
        ContractKind::Inverse => proto::ContractKind::Inverse as i32,
    }
}

fn contract_kind_from_proto(value: i32) -> Result<ContractKind, GatewayClientError> {
    match proto::ContractKind::try_from(value).ok() {
        Some(proto::ContractKind::Linear) => Ok(ContractKind::Linear),
        Some(proto::ContractKind::Inverse) => Ok(ContractKind::Inverse),
        _ => Err(GatewayClientError::InvalidField("contract_kind")),
    }
}

fn side_from_proto(value: i32) -> Result<Side, GatewayClientError> {
    match proto::Side::try_from(value).ok() {
        Some(proto::Side::Buy) => Ok(Side::Buy),
        Some(proto::Side::Sell) => Ok(Side::Sell),
        _ => Err(GatewayClientError::InvalidField("trade.taker_side")),
    }
}

fn parse_positive_decimal(value: &str, field: &'static str) -> Result<Decimal, GatewayClientError> {
    let value = parse_decimal(value, field)?;
    if value <= Decimal::ZERO {
        return Err(GatewayClientError::InvalidField(field));
    }
    Ok(value)
}

fn parse_optional_positive_decimal(
    value: Option<&str>,
    field: &'static str,
) -> Result<Option<Decimal>, GatewayClientError> {
    value
        .map(|value| parse_positive_decimal(value, field))
        .transpose()
}

fn parse_optional_non_negative_decimal(
    value: Option<&str>,
    field: &'static str,
) -> Result<Option<Decimal>, GatewayClientError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let parsed = parse_decimal(value, field)?;
    if parsed < Decimal::ZERO {
        return Err(GatewayClientError::InvalidField(field));
    }
    Ok(Some(parsed))
}

fn parse_decimal(value: &str, field: &'static str) -> Result<Decimal, GatewayClientError> {
    Decimal::from_str(value).map_err(|_| GatewayClientError::InvalidDecimal {
        field,
        value: value.to_owned(),
    })
}

fn required<T>(value: Option<T>, field: &'static str) -> Result<T, GatewayClientError> {
    value.ok_or(GatewayClientError::MissingField(field))
}

fn required_text(value: String, field: &'static str) -> Result<String, GatewayClientError> {
    if value.trim().is_empty() {
        Err(GatewayClientError::MissingField(field))
    } else {
        Ok(value)
    }
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[derive(Debug, Error)]
pub enum GatewayClientError {
    #[error("gateway transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("gateway RPC failed: {0}")]
    Rpc(#[from] Status),
    #[error("gateway response is missing {0}")]
    MissingField(&'static str),
    #[error("gateway response contains invalid {0}")]
    InvalidField(&'static str),
    #[error("gateway response contains invalid decimal for {field}: {value}")]
    InvalidDecimal { field: &'static str, value: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_spot_contract_metadata() {
        let result = instrument_from_proto(proto::Instrument {
            key: Some(proto::InstrumentKey {
                exchange: proto::Exchange::Binance as i32,
                market_kind: proto::MarketKind::Spot as i32,
                symbol: "BTC/USDT".to_owned(),
            }),
            exchange_symbol: "BTCUSDT".to_owned(),
            base_asset: "BTC".to_owned(),
            quote_asset: "USDT".to_owned(),
            settle_asset: None,
            contract_kind: proto::ContractKind::Linear as i32,
            contract_size: Some("1".to_owned()),
            price_tick: "0.1".to_owned(),
            quantity_step: "0.001".to_owned(),
            minimum_quantity: None,
            minimum_notional: None,
            maker_fee_rate: None,
            taker_fee_rate: None,
            active: true,
        });

        assert!(matches!(
            result,
            Err(GatewayClientError::InvalidField("instrument.contract_kind"))
        ));
    }
}
