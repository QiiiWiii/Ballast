// Original file: ../proto/exchange_gateway.proto

import type { Long } from '@grpc/proto-loader';

export interface HistoricalTrade {
  'exchangeTradeId'?: (string);
  'tradeTimeMs'?: (number | string | Long);
  'price'?: (string);
  'quantity'?: (string);
  'takerSide'?: (string);
}

export interface HistoricalTrade__Output {
  'exchangeTradeId'?: (string);
  'tradeTimeMs'?: (Long);
  'price'?: (string);
  'quantity'?: (string);
  'takerSide'?: (string);
}
