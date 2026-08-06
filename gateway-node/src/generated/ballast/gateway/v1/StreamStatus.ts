// Original file: ../proto/exchange_gateway.proto

import type { StreamState as _ballast_gateway_v1_StreamState, StreamState__Output as _ballast_gateway_v1_StreamState__Output } from '../../../ballast/gateway/v1/StreamState';
import type { Long } from '@grpc/proto-loader';

export interface StreamStatus {
  'state'?: (_ballast_gateway_v1_StreamState);
  'gatewayTimeMs'?: (number | string | Long);
  'reconnectAttempt'?: (number);
  'errorCode'?: (string);
  '_errorCode'?: "errorCode";
}

export interface StreamStatus__Output {
  'state'?: (_ballast_gateway_v1_StreamState__Output);
  'gatewayTimeMs'?: (Long);
  'reconnectAttempt'?: (number);
  'errorCode'?: (string);
}
