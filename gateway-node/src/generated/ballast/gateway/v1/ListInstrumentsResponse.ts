// Original file: ../proto/exchange_gateway.proto

import type { Instrument as _ballast_gateway_v1_Instrument, Instrument__Output as _ballast_gateway_v1_Instrument__Output } from '../../../ballast/gateway/v1/Instrument';
import type { Long } from '@grpc/proto-loader';

export interface ListInstrumentsResponse {
  'instruments'?: (_ballast_gateway_v1_Instrument)[];
  'observedAtMs'?: (number | string | Long);
}

export interface ListInstrumentsResponse__Output {
  'instruments'?: (_ballast_gateway_v1_Instrument__Output)[];
  'observedAtMs'?: (Long);
}
