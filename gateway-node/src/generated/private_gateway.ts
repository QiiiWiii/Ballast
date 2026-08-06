import type * as grpc from '@grpc/grpc-js';
import type { EnumTypeDefinition, MessageTypeDefinition } from '@grpc/proto-loader';

import type { AccountServiceClient as _ballast_gateway_v1_AccountServiceClient, AccountServiceDefinition as _ballast_gateway_v1_AccountServiceDefinition } from './ballast/gateway/v1/AccountService';
import type { AlgorithmicTradingServiceClient as _ballast_gateway_v1_AlgorithmicTradingServiceClient, AlgorithmicTradingServiceDefinition as _ballast_gateway_v1_AlgorithmicTradingServiceDefinition } from './ballast/gateway/v1/AlgorithmicTradingService';
import type { MarketDataServiceClient as _ballast_gateway_v1_MarketDataServiceClient, MarketDataServiceDefinition as _ballast_gateway_v1_MarketDataServiceDefinition } from './ballast/gateway/v1/MarketDataService';
import type { TradingServiceClient as _ballast_gateway_v1_TradingServiceClient, TradingServiceDefinition as _ballast_gateway_v1_TradingServiceDefinition } from './ballast/gateway/v1/TradingService';

type SubtypeConstructor<Constructor extends new (...args: any) => any, Subtype> = {
  new(...args: ConstructorParameters<Constructor>): Subtype;
};

export interface ProtoGrpcType {
  ballast: {
    gateway: {
      v1: {
        AccountEnvironment: EnumTypeDefinition
        AccountEvent: MessageTypeDefinition
        AccountRef: MessageTypeDefinition
        AccountService: SubtypeConstructor<typeof grpc.Client, _ballast_gateway_v1_AccountServiceClient> & { service: _ballast_gateway_v1_AccountServiceDefinition }
        AccountSnapshot: MessageTypeDefinition
        AdapterHealth: MessageTypeDefinition
        AlgoCapabilities: MessageTypeDefinition
        AlgoCapability: MessageTypeDefinition
        AlgoEvent: MessageTypeDefinition
        AlgoOrderSnapshot: MessageTypeDefinition
        AlgoOrderState: EnumTypeDefinition
        AlgoSubOrder: MessageTypeDefinition
        AlgorithmicTradingService: SubtypeConstructor<typeof grpc.Client, _ballast_gateway_v1_AlgorithmicTradingServiceClient> & { service: _ballast_gateway_v1_AlgorithmicTradingServiceDefinition }
        Balance: MessageTypeDefinition
        BinanceFuturesTwapConfig: MessageTypeDefinition
        BinanceFuturesVpConfig: MessageTypeDefinition
        BinanceSpotTwapConfig: MessageTypeDefinition
        BookLevel: MessageTypeDefinition
        CancelAlgoOrderRequest: MessageTypeDefinition
        CancelOrderRequest: MessageTypeDefinition
        ContractKind: EnumTypeDefinition
        Exchange: EnumTypeDefinition
        ExchangeCapabilities: MessageTypeDefinition
        Fill: MessageTypeDefinition
        GetAccountSnapshotRequest: MessageTypeDefinition
        GetAlgoCapabilitiesRequest: MessageTypeDefinition
        GetAlgoOrderRequest: MessageTypeDefinition
        GetCapabilitiesRequest: MessageTypeDefinition
        GetOrderBookRequest: MessageTypeDefinition
        GetOrderByClientIdRequest: MessageTypeDefinition
        GetTradingCapabilitiesRequest: MessageTypeDefinition
        HealthRequest: MessageTypeDefinition
        HealthResponse: MessageTypeDefinition
        Instrument: MessageTypeDefinition
        InstrumentKey: MessageTypeDefinition
        ListAlgoSubOrdersRequest: MessageTypeDefinition
        ListAlgoSubOrdersResponse: MessageTypeDefinition
        ListInstrumentsRequest: MessageTypeDefinition
        ListInstrumentsResponse: MessageTypeDefinition
        MarketDataService: SubtypeConstructor<typeof grpc.Client, _ballast_gateway_v1_MarketDataServiceClient> & { service: _ballast_gateway_v1_MarketDataServiceDefinition }
        MarketKind: EnumTypeDefinition
        OkxTwapConfig: MessageTypeDefinition
        OrderBook: MessageTypeDefinition
        OrderBookStreamEvent: MessageTypeDefinition
        OrderEvent: MessageTypeDefinition
        OrderSnapshot: MessageTypeDefinition
        OrderState: EnumTypeDefinition
        PlaceIocOrderRequest: MessageTypeDefinition
        Position: MessageTypeDefinition
        Side: EnumTypeDefinition
        StreamState: EnumTypeDefinition
        StreamStatus: MessageTypeDefinition
        SubmitAlgoOrderRequest: MessageTypeDefinition
        Trade: MessageTypeDefinition
        TradeStreamEvent: MessageTypeDefinition
        TradingCapabilities: MessageTypeDefinition
        TradingService: SubtypeConstructor<typeof grpc.Client, _ballast_gateway_v1_TradingServiceClient> & { service: _ballast_gateway_v1_TradingServiceDefinition }
        WatchAccountEventsRequest: MessageTypeDefinition
        WatchAlgoEventsRequest: MessageTypeDefinition
        WatchMarketRequest: MessageTypeDefinition
        WatchOrderEventsRequest: MessageTypeDefinition
      }
    }
  }
}

