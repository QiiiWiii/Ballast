// Original file: ../proto/private_gateway.proto

import type { StreamStatus as _ballast_gateway_v1_StreamStatus, StreamStatus__Output as _ballast_gateway_v1_StreamStatus__Output } from '../../../ballast/gateway/v1/StreamStatus';
import type { OrderSnapshot as _ballast_gateway_v1_OrderSnapshot, OrderSnapshot__Output as _ballast_gateway_v1_OrderSnapshot__Output } from '../../../ballast/gateway/v1/OrderSnapshot';
import type { Fill as _ballast_gateway_v1_Fill, Fill__Output as _ballast_gateway_v1_Fill__Output } from '../../../ballast/gateway/v1/Fill';

export interface OrderEvent {
  'status'?: (_ballast_gateway_v1_StreamStatus | null);
  'order'?: (_ballast_gateway_v1_OrderSnapshot | null);
  'fill'?: (_ballast_gateway_v1_Fill | null);
  'payload'?: "status"|"order"|"fill";
}

export interface OrderEvent__Output {
  'status'?: (_ballast_gateway_v1_StreamStatus__Output);
  'order'?: (_ballast_gateway_v1_OrderSnapshot__Output);
  'fill'?: (_ballast_gateway_v1_Fill__Output);
}
