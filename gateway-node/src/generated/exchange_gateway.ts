import type * as grpc from '@grpc/grpc-js';
import type { EnumTypeDefinition, MessageTypeDefinition } from '@grpc/proto-loader';

import type { MarketDataServiceClient as _ballast_gateway_v1_MarketDataServiceClient, MarketDataServiceDefinition as _ballast_gateway_v1_MarketDataServiceDefinition } from './ballast/gateway/v1/MarketDataService';

type SubtypeConstructor<Constructor extends new (...args: any) => any, Subtype> = {
  new(...args: ConstructorParameters<Constructor>): Subtype;
};

export interface ProtoGrpcType {
  ballast: {
    gateway: {
      v1: {
        AdapterHealth: MessageTypeDefinition
        BookLevel: MessageTypeDefinition
        ContractKind: EnumTypeDefinition
        Exchange: EnumTypeDefinition
        ExchangeCapabilities: MessageTypeDefinition
        GetCapabilitiesRequest: MessageTypeDefinition
        GetOrderBookRequest: MessageTypeDefinition
        HealthRequest: MessageTypeDefinition
        HealthResponse: MessageTypeDefinition
        Instrument: MessageTypeDefinition
        InstrumentKey: MessageTypeDefinition
        ListInstrumentsRequest: MessageTypeDefinition
        ListInstrumentsResponse: MessageTypeDefinition
        MarketDataService: SubtypeConstructor<typeof grpc.Client, _ballast_gateway_v1_MarketDataServiceClient> & { service: _ballast_gateway_v1_MarketDataServiceDefinition }
        MarketKind: EnumTypeDefinition
        OrderBook: MessageTypeDefinition
        OrderBookStreamEvent: MessageTypeDefinition
        Side: EnumTypeDefinition
        StreamState: EnumTypeDefinition
        StreamStatus: MessageTypeDefinition
        Trade: MessageTypeDefinition
        TradeStreamEvent: MessageTypeDefinition
        WatchMarketRequest: MessageTypeDefinition
      }
    }
  }
}

