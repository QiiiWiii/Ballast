// Original file: ../proto/private_gateway.proto

import type * as grpc from '@grpc/grpc-js'
import type { MethodDefinition } from '@grpc/proto-loader'
import type { CancelOrderRequest as _ballast_gateway_v1_CancelOrderRequest, CancelOrderRequest__Output as _ballast_gateway_v1_CancelOrderRequest__Output } from '../../../ballast/gateway/v1/CancelOrderRequest';
import type { GetOrderByClientIdRequest as _ballast_gateway_v1_GetOrderByClientIdRequest, GetOrderByClientIdRequest__Output as _ballast_gateway_v1_GetOrderByClientIdRequest__Output } from '../../../ballast/gateway/v1/GetOrderByClientIdRequest';
import type { GetTradingCapabilitiesRequest as _ballast_gateway_v1_GetTradingCapabilitiesRequest, GetTradingCapabilitiesRequest__Output as _ballast_gateway_v1_GetTradingCapabilitiesRequest__Output } from '../../../ballast/gateway/v1/GetTradingCapabilitiesRequest';
import type { OrderEvent as _ballast_gateway_v1_OrderEvent, OrderEvent__Output as _ballast_gateway_v1_OrderEvent__Output } from '../../../ballast/gateway/v1/OrderEvent';
import type { OrderSnapshot as _ballast_gateway_v1_OrderSnapshot, OrderSnapshot__Output as _ballast_gateway_v1_OrderSnapshot__Output } from '../../../ballast/gateway/v1/OrderSnapshot';
import type { PlaceIocOrderRequest as _ballast_gateway_v1_PlaceIocOrderRequest, PlaceIocOrderRequest__Output as _ballast_gateway_v1_PlaceIocOrderRequest__Output } from '../../../ballast/gateway/v1/PlaceIocOrderRequest';
import type { TradingCapabilities as _ballast_gateway_v1_TradingCapabilities, TradingCapabilities__Output as _ballast_gateway_v1_TradingCapabilities__Output } from '../../../ballast/gateway/v1/TradingCapabilities';
import type { WatchOrderEventsRequest as _ballast_gateway_v1_WatchOrderEventsRequest, WatchOrderEventsRequest__Output as _ballast_gateway_v1_WatchOrderEventsRequest__Output } from '../../../ballast/gateway/v1/WatchOrderEventsRequest';

export interface TradingServiceClient extends grpc.Client {
  CancelOrder(argument: _ballast_gateway_v1_CancelOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  CancelOrder(argument: _ballast_gateway_v1_CancelOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  CancelOrder(argument: _ballast_gateway_v1_CancelOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  CancelOrder(argument: _ballast_gateway_v1_CancelOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  cancelOrder(argument: _ballast_gateway_v1_CancelOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  cancelOrder(argument: _ballast_gateway_v1_CancelOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  cancelOrder(argument: _ballast_gateway_v1_CancelOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  cancelOrder(argument: _ballast_gateway_v1_CancelOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  
  GetOrderByClientId(argument: _ballast_gateway_v1_GetOrderByClientIdRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  GetOrderByClientId(argument: _ballast_gateway_v1_GetOrderByClientIdRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  GetOrderByClientId(argument: _ballast_gateway_v1_GetOrderByClientIdRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  GetOrderByClientId(argument: _ballast_gateway_v1_GetOrderByClientIdRequest, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  getOrderByClientId(argument: _ballast_gateway_v1_GetOrderByClientIdRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  getOrderByClientId(argument: _ballast_gateway_v1_GetOrderByClientIdRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  getOrderByClientId(argument: _ballast_gateway_v1_GetOrderByClientIdRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  getOrderByClientId(argument: _ballast_gateway_v1_GetOrderByClientIdRequest, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  
  GetTradingCapabilities(argument: _ballast_gateway_v1_GetTradingCapabilitiesRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_TradingCapabilities__Output>): grpc.ClientUnaryCall;
  GetTradingCapabilities(argument: _ballast_gateway_v1_GetTradingCapabilitiesRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_TradingCapabilities__Output>): grpc.ClientUnaryCall;
  GetTradingCapabilities(argument: _ballast_gateway_v1_GetTradingCapabilitiesRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_TradingCapabilities__Output>): grpc.ClientUnaryCall;
  GetTradingCapabilities(argument: _ballast_gateway_v1_GetTradingCapabilitiesRequest, callback: grpc.requestCallback<_ballast_gateway_v1_TradingCapabilities__Output>): grpc.ClientUnaryCall;
  getTradingCapabilities(argument: _ballast_gateway_v1_GetTradingCapabilitiesRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_TradingCapabilities__Output>): grpc.ClientUnaryCall;
  getTradingCapabilities(argument: _ballast_gateway_v1_GetTradingCapabilitiesRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_TradingCapabilities__Output>): grpc.ClientUnaryCall;
  getTradingCapabilities(argument: _ballast_gateway_v1_GetTradingCapabilitiesRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_TradingCapabilities__Output>): grpc.ClientUnaryCall;
  getTradingCapabilities(argument: _ballast_gateway_v1_GetTradingCapabilitiesRequest, callback: grpc.requestCallback<_ballast_gateway_v1_TradingCapabilities__Output>): grpc.ClientUnaryCall;
  
  PlaceIocOrder(argument: _ballast_gateway_v1_PlaceIocOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  PlaceIocOrder(argument: _ballast_gateway_v1_PlaceIocOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  PlaceIocOrder(argument: _ballast_gateway_v1_PlaceIocOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  PlaceIocOrder(argument: _ballast_gateway_v1_PlaceIocOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  placeIocOrder(argument: _ballast_gateway_v1_PlaceIocOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  placeIocOrder(argument: _ballast_gateway_v1_PlaceIocOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  placeIocOrder(argument: _ballast_gateway_v1_PlaceIocOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  placeIocOrder(argument: _ballast_gateway_v1_PlaceIocOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_OrderSnapshot__Output>): grpc.ClientUnaryCall;
  
  WatchOrderEvents(argument: _ballast_gateway_v1_WatchOrderEventsRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_OrderEvent__Output>;
  WatchOrderEvents(argument: _ballast_gateway_v1_WatchOrderEventsRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_OrderEvent__Output>;
  watchOrderEvents(argument: _ballast_gateway_v1_WatchOrderEventsRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_OrderEvent__Output>;
  watchOrderEvents(argument: _ballast_gateway_v1_WatchOrderEventsRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_OrderEvent__Output>;
  
}

export interface TradingServiceHandlers extends grpc.UntypedServiceImplementation {
  CancelOrder: grpc.handleUnaryCall<_ballast_gateway_v1_CancelOrderRequest__Output, _ballast_gateway_v1_OrderSnapshot>;
  
  GetOrderByClientId: grpc.handleUnaryCall<_ballast_gateway_v1_GetOrderByClientIdRequest__Output, _ballast_gateway_v1_OrderSnapshot>;
  
  GetTradingCapabilities: grpc.handleUnaryCall<_ballast_gateway_v1_GetTradingCapabilitiesRequest__Output, _ballast_gateway_v1_TradingCapabilities>;
  
  PlaceIocOrder: grpc.handleUnaryCall<_ballast_gateway_v1_PlaceIocOrderRequest__Output, _ballast_gateway_v1_OrderSnapshot>;
  
  WatchOrderEvents: grpc.handleServerStreamingCall<_ballast_gateway_v1_WatchOrderEventsRequest__Output, _ballast_gateway_v1_OrderEvent>;
  
}

export interface TradingServiceDefinition extends grpc.ServiceDefinition {
  CancelOrder: MethodDefinition<_ballast_gateway_v1_CancelOrderRequest, _ballast_gateway_v1_OrderSnapshot, _ballast_gateway_v1_CancelOrderRequest__Output, _ballast_gateway_v1_OrderSnapshot__Output>
  GetOrderByClientId: MethodDefinition<_ballast_gateway_v1_GetOrderByClientIdRequest, _ballast_gateway_v1_OrderSnapshot, _ballast_gateway_v1_GetOrderByClientIdRequest__Output, _ballast_gateway_v1_OrderSnapshot__Output>
  GetTradingCapabilities: MethodDefinition<_ballast_gateway_v1_GetTradingCapabilitiesRequest, _ballast_gateway_v1_TradingCapabilities, _ballast_gateway_v1_GetTradingCapabilitiesRequest__Output, _ballast_gateway_v1_TradingCapabilities__Output>
  PlaceIocOrder: MethodDefinition<_ballast_gateway_v1_PlaceIocOrderRequest, _ballast_gateway_v1_OrderSnapshot, _ballast_gateway_v1_PlaceIocOrderRequest__Output, _ballast_gateway_v1_OrderSnapshot__Output>
  WatchOrderEvents: MethodDefinition<_ballast_gateway_v1_WatchOrderEventsRequest, _ballast_gateway_v1_OrderEvent, _ballast_gateway_v1_WatchOrderEventsRequest__Output, _ballast_gateway_v1_OrderEvent__Output>
}
