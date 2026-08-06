// Original file: ../proto/exchange_gateway.proto

import type { Exchange as _ballast_gateway_v1_Exchange, Exchange__Output as _ballast_gateway_v1_Exchange__Output } from '../../../ballast/gateway/v1/Exchange';
import type { Long } from '@grpc/proto-loader';

export interface AdapterHealth {
  'exchange'?: (_ballast_gateway_v1_Exchange);
  'status'?: (string);
  'lastErrorCode'?: (string);
  'lastSuccessAtMs'?: (number | string | Long);
  '_lastErrorCode'?: "lastErrorCode";
  '_lastSuccessAtMs'?: "lastSuccessAtMs";
}

export interface AdapterHealth__Output {
  'exchange'?: (_ballast_gateway_v1_Exchange__Output);
  'status'?: (string);
  'lastErrorCode'?: (string);
  'lastSuccessAtMs'?: (Long);
}
