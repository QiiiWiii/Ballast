// Original file: ../proto/private_gateway.proto

import type { Exchange as _ballast_gateway_v1_Exchange, Exchange__Output as _ballast_gateway_v1_Exchange__Output } from '../../../ballast/gateway/v1/Exchange';

export interface TradingCapabilities {
  'exchange'?: (_ballast_gateway_v1_Exchange);
  'placeIoc'?: (boolean);
  'queryByClientOrderId'?: (boolean);
  'cancelOrder'?: (boolean);
  'privateOrderStream'?: (boolean);
  'privateFillStream'?: (boolean);
}

export interface TradingCapabilities__Output {
  'exchange'?: (_ballast_gateway_v1_Exchange__Output);
  'placeIoc'?: (boolean);
  'queryByClientOrderId'?: (boolean);
  'cancelOrder'?: (boolean);
  'privateOrderStream'?: (boolean);
  'privateFillStream'?: (boolean);
}
