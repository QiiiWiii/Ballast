// Original file: ../proto/private_gateway.proto

import type { AccountRef as _ballast_gateway_v1_AccountRef, AccountRef__Output as _ballast_gateway_v1_AccountRef__Output } from '../../../ballast/gateway/v1/AccountRef';
import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';
import type { AlgoOrderState as _ballast_gateway_v1_AlgoOrderState, AlgoOrderState__Output as _ballast_gateway_v1_AlgoOrderState__Output } from '../../../ballast/gateway/v1/AlgoOrderState';
import type { Long } from '@grpc/proto-loader';

export interface AlgoOrderSnapshot {
  'account'?: (_ballast_gateway_v1_AccountRef | null);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'clientAlgoId'?: (string);
  'exchangeAlgoId'?: (string);
  'algorithm'?: (string);
  'state'?: (_ballast_gateway_v1_AlgoOrderState);
  'requestedQuantity'?: (string);
  'executedQuantity'?: (string);
  'limitPrice'?: (string);
  'reasonCode'?: (string);
  'exchangeTimeMs'?: (number | string | Long);
  'gatewayReceivedAtMs'?: (number | string | Long);
  '_exchangeAlgoId'?: "exchangeAlgoId";
  '_reasonCode'?: "reasonCode";
}

export interface AlgoOrderSnapshot__Output {
  'account'?: (_ballast_gateway_v1_AccountRef__Output);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'clientAlgoId'?: (string);
  'exchangeAlgoId'?: (string);
  'algorithm'?: (string);
  'state'?: (_ballast_gateway_v1_AlgoOrderState__Output);
  'requestedQuantity'?: (string);
  'executedQuantity'?: (string);
  'limitPrice'?: (string);
  'reasonCode'?: (string);
  'exchangeTimeMs'?: (Long);
  'gatewayReceivedAtMs'?: (Long);
}
