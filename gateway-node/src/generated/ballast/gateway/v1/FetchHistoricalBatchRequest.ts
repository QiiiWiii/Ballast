// Original file: ../proto/exchange_gateway.proto

import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';
import type { HistoricalDataType as _ballast_gateway_v1_HistoricalDataType, HistoricalDataType__Output as _ballast_gateway_v1_HistoricalDataType__Output } from '../../../ballast/gateway/v1/HistoricalDataType';
import type { Long } from '@grpc/proto-loader';

export interface FetchHistoricalBatchRequest {
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'dataType'?: (_ballast_gateway_v1_HistoricalDataType);
  'timeframe'?: (string);
  'cursorMs'?: (number | string | Long);
  'endMs'?: (number | string | Long);
  'limit'?: (number);
  '_timeframe'?: "timeframe";
}

export interface FetchHistoricalBatchRequest__Output {
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'dataType'?: (_ballast_gateway_v1_HistoricalDataType__Output);
  'timeframe'?: (string);
  'cursorMs'?: (Long);
  'endMs'?: (Long);
  'limit'?: (number);
}
