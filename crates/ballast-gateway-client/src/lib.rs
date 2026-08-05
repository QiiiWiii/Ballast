#![forbid(unsafe_code)]

pub mod proto {
    tonic::include_proto!("ballast.gateway.v1");
}

pub use proto::exchange_gateway_client::ExchangeGatewayClient;
