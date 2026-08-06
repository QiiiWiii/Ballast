// Original file: ../proto/exchange_gateway.proto

import type { AdapterHealth as _ballast_gateway_v1_AdapterHealth, AdapterHealth__Output as _ballast_gateway_v1_AdapterHealth__Output } from '../../../ballast/gateway/v1/AdapterHealth';
import type { Long } from '@grpc/proto-loader';

export interface HealthResponse {
  'status'?: (string);
  'serviceVersion'?: (string);
  'gatewayTimeMs'?: (number | string | Long);
  'adapters'?: (_ballast_gateway_v1_AdapterHealth)[];
}

export interface HealthResponse__Output {
  'status'?: (string);
  'serviceVersion'?: (string);
  'gatewayTimeMs'?: (Long);
  'adapters'?: (_ballast_gateway_v1_AdapterHealth__Output)[];
}
