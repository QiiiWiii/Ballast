// Original file: ../proto/exchange_gateway.proto

import type * as grpc from '@grpc/grpc-js'
import type { MethodDefinition } from '@grpc/proto-loader'
import type { ExchangeCapabilities as _ballast_gateway_v1_ExchangeCapabilities, ExchangeCapabilities__Output as _ballast_gateway_v1_ExchangeCapabilities__Output } from '../../../ballast/gateway/v1/ExchangeCapabilities';
import type { GetCapabilitiesRequest as _ballast_gateway_v1_GetCapabilitiesRequest, GetCapabilitiesRequest__Output as _ballast_gateway_v1_GetCapabilitiesRequest__Output } from '../../../ballast/gateway/v1/GetCapabilitiesRequest';
import type { GetOrderBookRequest as _ballast_gateway_v1_GetOrderBookRequest, GetOrderBookRequest__Output as _ballast_gateway_v1_GetOrderBookRequest__Output } from '../../../ballast/gateway/v1/GetOrderBookRequest';
import type { HealthRequest as _ballast_gateway_v1_HealthRequest, HealthRequest__Output as _ballast_gateway_v1_HealthRequest__Output } from '../../../ballast/gateway/v1/HealthRequest';
import type { HealthResponse as _ballast_gateway_v1_HealthResponse, HealthResponse__Output as _ballast_gateway_v1_HealthResponse__Output } from '../../../ballast/gateway/v1/HealthResponse';
import type { ListInstrumentsRequest as _ballast_gateway_v1_ListInstrumentsRequest, ListInstrumentsRequest__Output as _ballast_gateway_v1_ListInstrumentsRequest__Output } from '../../../ballast/gateway/v1/ListInstrumentsRequest';
import type { ListInstrumentsResponse as _ballast_gateway_v1_ListInstrumentsResponse, ListInstrumentsResponse__Output as _ballast_gateway_v1_ListInstrumentsResponse__Output } from '../../../ballast/gateway/v1/ListInstrumentsResponse';
import type { OrderBook as _ballast_gateway_v1_OrderBook, OrderBook__Output as _ballast_gateway_v1_OrderBook__Output } from '../../../ballast/gateway/v1/OrderBook';
import type { OrderBookStreamEvent as _ballast_gateway_v1_OrderBookStreamEvent, OrderBookStreamEvent__Output as _ballast_gateway_v1_OrderBookStreamEvent__Output } from '../../../ballast/gateway/v1/OrderBookStreamEvent';
import type { TradeStreamEvent as _ballast_gateway_v1_TradeStreamEvent, TradeStreamEvent__Output as _ballast_gateway_v1_TradeStreamEvent__Output } from '../../../ballast/gateway/v1/TradeStreamEvent';
import type { WatchMarketRequest as _ballast_gateway_v1_WatchMarketRequest, WatchMarketRequest__Output as _ballast_gateway_v1_WatchMarketRequest__Output } from '../../../ballast/gateway/v1/WatchMarketRequest';

export interface MarketDataServiceClient extends grpc.Client {
  GetCapabilities(argument: _ballast_gateway_v1_GetCapabilitiesRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ExchangeCapabilities__Output>): grpc.ClientUnaryCall;
  GetCapabilities(argument: _ballast_gateway_v1_GetCapabilitiesRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_ExchangeCapabilities__Output>): grpc.ClientUnaryCall;
  GetCapabilities(argument: _ballast_gateway_v1_GetCapabilitiesRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ExchangeCapabilities__Output>): grpc.ClientUnaryCall;
  GetCapabilities(argument: _ballast_gateway_v1_GetCapabilitiesRequest, callback: grpc.requestCallback<_ballast_gateway_v1_ExchangeCapabilities__Output>): grpc.ClientUnaryCall;
  getCapabilities(argument: _ballast_gateway_v1_GetCapabilitiesRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ExchangeCapabilities__Output>): grpc.ClientUnaryCall;
  getCapabilities(argument: _ballast_gateway_v1_GetCapabilitiesRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_ExchangeCapabilities__Output>): grpc.ClientUnaryCall;
  getCapabilities(argument: _ballast_gateway_v1_GetCapabilitiesRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ExchangeCapabilities__Output>): grpc.ClientUnaryCall;
  getCapabilities(argument: _ballast_gateway_v1_GetCapabilitiesRequest, callback: grpc.requestCallback<_ballast_gateway_v1_ExchangeCapabilities__Output>): grpc.ClientUnaryCall;
  
  GetOrderBook(argument: _ballast_gateway_v1_GetOrderBookRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderBook__Output>): grpc.ClientUnaryCall;
  GetOrderBook(argument: _ballast_gateway_v1_GetOrderBookRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_OrderBook__Output>): grpc.ClientUnaryCall;
  GetOrderBook(argument: _ballast_gateway_v1_GetOrderBookRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderBook__Output>): grpc.ClientUnaryCall;
  GetOrderBook(argument: _ballast_gateway_v1_GetOrderBookRequest, callback: grpc.requestCallback<_ballast_gateway_v1_OrderBook__Output>): grpc.ClientUnaryCall;
  getOrderBook(argument: _ballast_gateway_v1_GetOrderBookRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderBook__Output>): grpc.ClientUnaryCall;
  getOrderBook(argument: _ballast_gateway_v1_GetOrderBookRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_OrderBook__Output>): grpc.ClientUnaryCall;
  getOrderBook(argument: _ballast_gateway_v1_GetOrderBookRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderBook__Output>): grpc.ClientUnaryCall;
  getOrderBook(argument: _ballast_gateway_v1_GetOrderBookRequest, callback: grpc.requestCallback<_ballast_gateway_v1_OrderBook__Output>): grpc.ClientUnaryCall;
  
  Health(argument: _ballast_gateway_v1_HealthRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_HealthResponse__Output>): grpc.ClientUnaryCall;
  Health(argument: _ballast_gateway_v1_HealthRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_HealthResponse__Output>): grpc.ClientUnaryCall;
  Health(argument: _ballast_gateway_v1_HealthRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_HealthResponse__Output>): grpc.ClientUnaryCall;
  Health(argument: _ballast_gateway_v1_HealthRequest, callback: grpc.requestCallback<_ballast_gateway_v1_HealthResponse__Output>): grpc.ClientUnaryCall;
  health(argument: _ballast_gateway_v1_HealthRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_HealthResponse__Output>): grpc.ClientUnaryCall;
  health(argument: _ballast_gateway_v1_HealthRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_HealthResponse__Output>): grpc.ClientUnaryCall;
  health(argument: _ballast_gateway_v1_HealthRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_HealthResponse__Output>): grpc.ClientUnaryCall;
  health(argument: _ballast_gateway_v1_HealthRequest, callback: grpc.requestCallback<_ballast_gateway_v1_HealthResponse__Output>): grpc.ClientUnaryCall;
  
  ListInstruments(argument: _ballast_gateway_v1_ListInstrumentsRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ListInstrumentsResponse__Output>): grpc.ClientUnaryCall;
  ListInstruments(argument: _ballast_gateway_v1_ListInstrumentsRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_ListInstrumentsResponse__Output>): grpc.ClientUnaryCall;
  ListInstruments(argument: _ballast_gateway_v1_ListInstrumentsRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ListInstrumentsResponse__Output>): grpc.ClientUnaryCall;
  ListInstruments(argument: _ballast_gateway_v1_ListInstrumentsRequest, callback: grpc.requestCallback<_ballast_gateway_v1_ListInstrumentsResponse__Output>): grpc.ClientUnaryCall;
  listInstruments(argument: _ballast_gateway_v1_ListInstrumentsRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ListInstrumentsResponse__Output>): grpc.ClientUnaryCall;
  listInstruments(argument: _ballast_gateway_v1_ListInstrumentsRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_ListInstrumentsResponse__Output>): grpc.ClientUnaryCall;
  listInstruments(argument: _ballast_gateway_v1_ListInstrumentsRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ListInstrumentsResponse__Output>): grpc.ClientUnaryCall;
  listInstruments(argument: _ballast_gateway_v1_ListInstrumentsRequest, callback: grpc.requestCallback<_ballast_gateway_v1_ListInstrumentsResponse__Output>): grpc.ClientUnaryCall;
  
  WatchOrderBook(argument: _ballast_gateway_v1_WatchMarketRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_OrderBookStreamEvent__Output>;
  WatchOrderBook(argument: _ballast_gateway_v1_WatchMarketRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_OrderBookStreamEvent__Output>;
  watchOrderBook(argument: _ballast_gateway_v1_WatchMarketRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_OrderBookStreamEvent__Output>;
  watchOrderBook(argument: _ballast_gateway_v1_WatchMarketRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_OrderBookStreamEvent__Output>;
  
  WatchTrades(argument: _ballast_gateway_v1_WatchMarketRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_TradeStreamEvent__Output>;
  WatchTrades(argument: _ballast_gateway_v1_WatchMarketRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_TradeStreamEvent__Output>;
  watchTrades(argument: _ballast_gateway_v1_WatchMarketRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_TradeStreamEvent__Output>;
  watchTrades(argument: _ballast_gateway_v1_WatchMarketRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_TradeStreamEvent__Output>;
  
}

export interface MarketDataServiceHandlers extends grpc.UntypedServiceImplementation {
  GetCapabilities: grpc.handleUnaryCall<_ballast_gateway_v1_GetCapabilitiesRequest__Output, _ballast_gateway_v1_ExchangeCapabilities>;
  
  GetOrderBook: grpc.handleUnaryCall<_ballast_gateway_v1_GetOrderBookRequest__Output, _ballast_gateway_v1_OrderBook>;
  
  Health: grpc.handleUnaryCall<_ballast_gateway_v1_HealthRequest__Output, _ballast_gateway_v1_HealthResponse>;
  
  ListInstruments: grpc.handleUnaryCall<_ballast_gateway_v1_ListInstrumentsRequest__Output, _ballast_gateway_v1_ListInstrumentsResponse>;
  
  WatchOrderBook: grpc.handleServerStreamingCall<_ballast_gateway_v1_WatchMarketRequest__Output, _ballast_gateway_v1_OrderBookStreamEvent>;
  
  WatchTrades: grpc.handleServerStreamingCall<_ballast_gateway_v1_WatchMarketRequest__Output, _ballast_gateway_v1_TradeStreamEvent>;
  
}

export interface MarketDataServiceDefinition extends grpc.ServiceDefinition {
  GetCapabilities: MethodDefinition<_ballast_gateway_v1_GetCapabilitiesRequest, _ballast_gateway_v1_ExchangeCapabilities, _ballast_gateway_v1_GetCapabilitiesRequest__Output, _ballast_gateway_v1_ExchangeCapabilities__Output>
  GetOrderBook: MethodDefinition<_ballast_gateway_v1_GetOrderBookRequest, _ballast_gateway_v1_OrderBook, _ballast_gateway_v1_GetOrderBookRequest__Output, _ballast_gateway_v1_OrderBook__Output>
  Health: MethodDefinition<_ballast_gateway_v1_HealthRequest, _ballast_gateway_v1_HealthResponse, _ballast_gateway_v1_HealthRequest__Output, _ballast_gateway_v1_HealthResponse__Output>
  ListInstruments: MethodDefinition<_ballast_gateway_v1_ListInstrumentsRequest, _ballast_gateway_v1_ListInstrumentsResponse, _ballast_gateway_v1_ListInstrumentsRequest__Output, _ballast_gateway_v1_ListInstrumentsResponse__Output>
  WatchOrderBook: MethodDefinition<_ballast_gateway_v1_WatchMarketRequest, _ballast_gateway_v1_OrderBookStreamEvent, _ballast_gateway_v1_WatchMarketRequest__Output, _ballast_gateway_v1_OrderBookStreamEvent__Output>
  WatchTrades: MethodDefinition<_ballast_gateway_v1_WatchMarketRequest, _ballast_gateway_v1_TradeStreamEvent, _ballast_gateway_v1_WatchMarketRequest__Output, _ballast_gateway_v1_TradeStreamEvent__Output>
}
