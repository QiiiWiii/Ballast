// Original file: ../proto/exchange_gateway.proto

import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';
import type { ContractKind as _ballast_gateway_v1_ContractKind, ContractKind__Output as _ballast_gateway_v1_ContractKind__Output } from '../../../ballast/gateway/v1/ContractKind';

export interface Instrument {
  'key'?: (_ballast_gateway_v1_InstrumentKey | null);
  'exchangeSymbol'?: (string);
  'baseAsset'?: (string);
  'quoteAsset'?: (string);
  'settleAsset'?: (string);
  'contractKind'?: (_ballast_gateway_v1_ContractKind);
  'contractSize'?: (string);
  'priceTick'?: (string);
  'quantityStep'?: (string);
  'minimumQuantity'?: (string);
  'minimumNotional'?: (string);
  'makerFeeRate'?: (string);
  'takerFeeRate'?: (string);
  'active'?: (boolean);
  '_settleAsset'?: "settleAsset";
  '_contractSize'?: "contractSize";
  '_minimumQuantity'?: "minimumQuantity";
  '_minimumNotional'?: "minimumNotional";
  '_makerFeeRate'?: "makerFeeRate";
  '_takerFeeRate'?: "takerFeeRate";
}

export interface Instrument__Output {
  'key'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'exchangeSymbol'?: (string);
  'baseAsset'?: (string);
  'quoteAsset'?: (string);
  'settleAsset'?: (string);
  'contractKind'?: (_ballast_gateway_v1_ContractKind__Output);
  'contractSize'?: (string);
  'priceTick'?: (string);
  'quantityStep'?: (string);
  'minimumQuantity'?: (string);
  'minimumNotional'?: (string);
  'makerFeeRate'?: (string);
  'takerFeeRate'?: (string);
  'active'?: (boolean);
}
