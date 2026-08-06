// Original file: ../proto/private_gateway.proto

import type { AccountRef as _ballast_gateway_v1_AccountRef, AccountRef__Output as _ballast_gateway_v1_AccountRef__Output } from '../../../ballast/gateway/v1/AccountRef';
import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';
import type { Side as _ballast_gateway_v1_Side, Side__Output as _ballast_gateway_v1_Side__Output } from '../../../ballast/gateway/v1/Side';
import type { BinanceSpotTwapConfig as _ballast_gateway_v1_BinanceSpotTwapConfig, BinanceSpotTwapConfig__Output as _ballast_gateway_v1_BinanceSpotTwapConfig__Output } from '../../../ballast/gateway/v1/BinanceSpotTwapConfig';
import type { BinanceFuturesTwapConfig as _ballast_gateway_v1_BinanceFuturesTwapConfig, BinanceFuturesTwapConfig__Output as _ballast_gateway_v1_BinanceFuturesTwapConfig__Output } from '../../../ballast/gateway/v1/BinanceFuturesTwapConfig';
import type { BinanceFuturesVpConfig as _ballast_gateway_v1_BinanceFuturesVpConfig, BinanceFuturesVpConfig__Output as _ballast_gateway_v1_BinanceFuturesVpConfig__Output } from '../../../ballast/gateway/v1/BinanceFuturesVpConfig';
import type { OkxTwapConfig as _ballast_gateway_v1_OkxTwapConfig, OkxTwapConfig__Output as _ballast_gateway_v1_OkxTwapConfig__Output } from '../../../ballast/gateway/v1/OkxTwapConfig';

export interface SubmitAlgoOrderRequest {
  'account'?: (_ballast_gateway_v1_AccountRef | null);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'requestId'?: (string);
  'clientAlgoId'?: (string);
  'side'?: (_ballast_gateway_v1_Side);
  'quantity'?: (string);
  'binanceSpotTwap'?: (_ballast_gateway_v1_BinanceSpotTwapConfig | null);
  'binanceFuturesTwap'?: (_ballast_gateway_v1_BinanceFuturesTwapConfig | null);
  'binanceFuturesVp'?: (_ballast_gateway_v1_BinanceFuturesVpConfig | null);
  'okxTwap'?: (_ballast_gateway_v1_OkxTwapConfig | null);
  'configuration'?: "binanceSpotTwap"|"binanceFuturesTwap"|"binanceFuturesVp"|"okxTwap";
}

export interface SubmitAlgoOrderRequest__Output {
  'account'?: (_ballast_gateway_v1_AccountRef__Output);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'requestId'?: (string);
  'clientAlgoId'?: (string);
  'side'?: (_ballast_gateway_v1_Side__Output);
  'quantity'?: (string);
  'binanceSpotTwap'?: (_ballast_gateway_v1_BinanceSpotTwapConfig__Output);
  'binanceFuturesTwap'?: (_ballast_gateway_v1_BinanceFuturesTwapConfig__Output);
  'binanceFuturesVp'?: (_ballast_gateway_v1_BinanceFuturesVpConfig__Output);
  'okxTwap'?: (_ballast_gateway_v1_OkxTwapConfig__Output);
}
