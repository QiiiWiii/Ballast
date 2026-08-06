// Original file: ../proto/private_gateway.proto

import type { StreamStatus as _ballast_gateway_v1_StreamStatus, StreamStatus__Output as _ballast_gateway_v1_StreamStatus__Output } from '../../../ballast/gateway/v1/StreamStatus';
import type { AlgoOrderSnapshot as _ballast_gateway_v1_AlgoOrderSnapshot, AlgoOrderSnapshot__Output as _ballast_gateway_v1_AlgoOrderSnapshot__Output } from '../../../ballast/gateway/v1/AlgoOrderSnapshot';
import type { AlgoSubOrder as _ballast_gateway_v1_AlgoSubOrder, AlgoSubOrder__Output as _ballast_gateway_v1_AlgoSubOrder__Output } from '../../../ballast/gateway/v1/AlgoSubOrder';
import type { Fill as _ballast_gateway_v1_Fill, Fill__Output as _ballast_gateway_v1_Fill__Output } from '../../../ballast/gateway/v1/Fill';

export interface AlgoEvent {
  'status'?: (_ballast_gateway_v1_StreamStatus | null);
  'algoOrder'?: (_ballast_gateway_v1_AlgoOrderSnapshot | null);
  'subOrder'?: (_ballast_gateway_v1_AlgoSubOrder | null);
  'fill'?: (_ballast_gateway_v1_Fill | null);
  'payload'?: "status"|"algoOrder"|"subOrder"|"fill";
}

export interface AlgoEvent__Output {
  'status'?: (_ballast_gateway_v1_StreamStatus__Output);
  'algoOrder'?: (_ballast_gateway_v1_AlgoOrderSnapshot__Output);
  'subOrder'?: (_ballast_gateway_v1_AlgoSubOrder__Output);
  'fill'?: (_ballast_gateway_v1_Fill__Output);
}
