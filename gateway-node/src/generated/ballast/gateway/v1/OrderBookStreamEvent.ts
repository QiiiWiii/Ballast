// Original file: ../proto/exchange_gateway.proto

import type { StreamStatus as _ballast_gateway_v1_StreamStatus, StreamStatus__Output as _ballast_gateway_v1_StreamStatus__Output } from '../../../ballast/gateway/v1/StreamStatus';
import type { OrderBook as _ballast_gateway_v1_OrderBook, OrderBook__Output as _ballast_gateway_v1_OrderBook__Output } from '../../../ballast/gateway/v1/OrderBook';

export interface OrderBookStreamEvent {
  'status'?: (_ballast_gateway_v1_StreamStatus | null);
  'orderBook'?: (_ballast_gateway_v1_OrderBook | null);
  'payload'?: "status"|"orderBook";
}

export interface OrderBookStreamEvent__Output {
  'status'?: (_ballast_gateway_v1_StreamStatus__Output);
  'orderBook'?: (_ballast_gateway_v1_OrderBook__Output);
}
