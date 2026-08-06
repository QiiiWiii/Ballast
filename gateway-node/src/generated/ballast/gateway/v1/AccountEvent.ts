// Original file: ../proto/private_gateway.proto

import type { AccountRef as _ballast_gateway_v1_AccountRef, AccountRef__Output as _ballast_gateway_v1_AccountRef__Output } from '../../../ballast/gateway/v1/AccountRef';
import type { StreamStatus as _ballast_gateway_v1_StreamStatus, StreamStatus__Output as _ballast_gateway_v1_StreamStatus__Output } from '../../../ballast/gateway/v1/StreamStatus';
import type { Balance as _ballast_gateway_v1_Balance, Balance__Output as _ballast_gateway_v1_Balance__Output } from '../../../ballast/gateway/v1/Balance';
import type { Position as _ballast_gateway_v1_Position, Position__Output as _ballast_gateway_v1_Position__Output } from '../../../ballast/gateway/v1/Position';
import type { OrderSnapshot as _ballast_gateway_v1_OrderSnapshot, OrderSnapshot__Output as _ballast_gateway_v1_OrderSnapshot__Output } from '../../../ballast/gateway/v1/OrderSnapshot';
import type { Fill as _ballast_gateway_v1_Fill, Fill__Output as _ballast_gateway_v1_Fill__Output } from '../../../ballast/gateway/v1/Fill';
import type { Long } from '@grpc/proto-loader';

export interface AccountEvent {
  'eventId'?: (string);
  'account'?: (_ballast_gateway_v1_AccountRef | null);
  'status'?: (_ballast_gateway_v1_StreamStatus | null);
  'balance'?: (_ballast_gateway_v1_Balance | null);
  'position'?: (_ballast_gateway_v1_Position | null);
  'order'?: (_ballast_gateway_v1_OrderSnapshot | null);
  'fill'?: (_ballast_gateway_v1_Fill | null);
  'gatewayReceivedAtMs'?: (number | string | Long);
  'payload'?: "status"|"balance"|"position"|"order"|"fill";
}

export interface AccountEvent__Output {
  'eventId'?: (string);
  'account'?: (_ballast_gateway_v1_AccountRef__Output);
  'status'?: (_ballast_gateway_v1_StreamStatus__Output);
  'balance'?: (_ballast_gateway_v1_Balance__Output);
  'position'?: (_ballast_gateway_v1_Position__Output);
  'order'?: (_ballast_gateway_v1_OrderSnapshot__Output);
  'fill'?: (_ballast_gateway_v1_Fill__Output);
  'gatewayReceivedAtMs'?: (Long);
}
