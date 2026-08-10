// Original file: ../proto/exchange_gateway.proto

import type { Exchange as _ballast_gateway_v1_Exchange, Exchange__Output as _ballast_gateway_v1_Exchange__Output } from '../../../ballast/gateway/v1/Exchange';

export interface ExchangeCapabilities {
  'exchange'?: (_ballast_gateway_v1_Exchange);
  'spot'?: (boolean);
  'perpetualLinear'?: (boolean);
  'perpetualInverse'?: (boolean);
  'fetchOrderBook'?: (boolean);
  'watchOrderBook'?: (boolean);
  'watchTrades'?: (boolean);
  'fetchOhlcv'?: (boolean);
  'fetchTrades'?: (boolean);
}

export interface ExchangeCapabilities__Output {
  'exchange'?: (_ballast_gateway_v1_Exchange__Output);
  'spot'?: (boolean);
  'perpetualLinear'?: (boolean);
  'perpetualInverse'?: (boolean);
  'fetchOrderBook'?: (boolean);
  'watchOrderBook'?: (boolean);
  'watchTrades'?: (boolean);
  'fetchOhlcv'?: (boolean);
  'fetchTrades'?: (boolean);
}
