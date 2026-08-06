// Original file: ../proto/private_gateway.proto

import type { AccountRef as _ballast_gateway_v1_AccountRef, AccountRef__Output as _ballast_gateway_v1_AccountRef__Output } from '../../../ballast/gateway/v1/AccountRef';
import type { Balance as _ballast_gateway_v1_Balance, Balance__Output as _ballast_gateway_v1_Balance__Output } from '../../../ballast/gateway/v1/Balance';
import type { Position as _ballast_gateway_v1_Position, Position__Output as _ballast_gateway_v1_Position__Output } from '../../../ballast/gateway/v1/Position';
import type { OrderSnapshot as _ballast_gateway_v1_OrderSnapshot, OrderSnapshot__Output as _ballast_gateway_v1_OrderSnapshot__Output } from '../../../ballast/gateway/v1/OrderSnapshot';
import type { Long } from '@grpc/proto-loader';

export interface AccountSnapshot {
  'account'?: (_ballast_gateway_v1_AccountRef | null);
  'balances'?: (_ballast_gateway_v1_Balance)[];
  'positions'?: (_ballast_gateway_v1_Position)[];
  'openOrders'?: (_ballast_gateway_v1_OrderSnapshot)[];
  'exchangeTimeMs'?: (number | string | Long);
  'gatewayReceivedAtMs'?: (number | string | Long);
}

export interface AccountSnapshot__Output {
  'account'?: (_ballast_gateway_v1_AccountRef__Output);
  'balances'?: (_ballast_gateway_v1_Balance__Output)[];
  'positions'?: (_ballast_gateway_v1_Position__Output)[];
  'openOrders'?: (_ballast_gateway_v1_OrderSnapshot__Output)[];
  'exchangeTimeMs'?: (Long);
  'gatewayReceivedAtMs'?: (Long);
}
