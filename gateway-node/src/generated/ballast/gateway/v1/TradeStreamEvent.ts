// Original file: ../proto/exchange_gateway.proto

import type { StreamStatus as _ballast_gateway_v1_StreamStatus, StreamStatus__Output as _ballast_gateway_v1_StreamStatus__Output } from '../../../ballast/gateway/v1/StreamStatus';
import type { Trade as _ballast_gateway_v1_Trade, Trade__Output as _ballast_gateway_v1_Trade__Output } from '../../../ballast/gateway/v1/Trade';

export interface TradeStreamEvent {
  'status'?: (_ballast_gateway_v1_StreamStatus | null);
  'trade'?: (_ballast_gateway_v1_Trade | null);
  'payload'?: "status"|"trade";
}

export interface TradeStreamEvent__Output {
  'status'?: (_ballast_gateway_v1_StreamStatus__Output);
  'trade'?: (_ballast_gateway_v1_Trade__Output);
}
