// Original file: ../proto/private_gateway.proto

import type * as grpc from '@grpc/grpc-js'
import type { MethodDefinition } from '@grpc/proto-loader'
import type { AlgoCapabilities as _ballast_gateway_v1_AlgoCapabilities, AlgoCapabilities__Output as _ballast_gateway_v1_AlgoCapabilities__Output } from '../../../ballast/gateway/v1/AlgoCapabilities';
import type { AlgoEvent as _ballast_gateway_v1_AlgoEvent, AlgoEvent__Output as _ballast_gateway_v1_AlgoEvent__Output } from '../../../ballast/gateway/v1/AlgoEvent';
import type { AlgoOrderSnapshot as _ballast_gateway_v1_AlgoOrderSnapshot, AlgoOrderSnapshot__Output as _ballast_gateway_v1_AlgoOrderSnapshot__Output } from '../../../ballast/gateway/v1/AlgoOrderSnapshot';
import type { CancelAlgoOrderRequest as _ballast_gateway_v1_CancelAlgoOrderRequest, CancelAlgoOrderRequest__Output as _ballast_gateway_v1_CancelAlgoOrderRequest__Output } from '../../../ballast/gateway/v1/CancelAlgoOrderRequest';
import type { GetAlgoCapabilitiesRequest as _ballast_gateway_v1_GetAlgoCapabilitiesRequest, GetAlgoCapabilitiesRequest__Output as _ballast_gateway_v1_GetAlgoCapabilitiesRequest__Output } from '../../../ballast/gateway/v1/GetAlgoCapabilitiesRequest';
import type { GetAlgoOrderRequest as _ballast_gateway_v1_GetAlgoOrderRequest, GetAlgoOrderRequest__Output as _ballast_gateway_v1_GetAlgoOrderRequest__Output } from '../../../ballast/gateway/v1/GetAlgoOrderRequest';
import type { ListAlgoSubOrdersRequest as _ballast_gateway_v1_ListAlgoSubOrdersRequest, ListAlgoSubOrdersRequest__Output as _ballast_gateway_v1_ListAlgoSubOrdersRequest__Output } from '../../../ballast/gateway/v1/ListAlgoSubOrdersRequest';
import type { ListAlgoSubOrdersResponse as _ballast_gateway_v1_ListAlgoSubOrdersResponse, ListAlgoSubOrdersResponse__Output as _ballast_gateway_v1_ListAlgoSubOrdersResponse__Output } from '../../../ballast/gateway/v1/ListAlgoSubOrdersResponse';
import type { SubmitAlgoOrderRequest as _ballast_gateway_v1_SubmitAlgoOrderRequest, SubmitAlgoOrderRequest__Output as _ballast_gateway_v1_SubmitAlgoOrderRequest__Output } from '../../../ballast/gateway/v1/SubmitAlgoOrderRequest';
import type { WatchAlgoEventsRequest as _ballast_gateway_v1_WatchAlgoEventsRequest, WatchAlgoEventsRequest__Output as _ballast_gateway_v1_WatchAlgoEventsRequest__Output } from '../../../ballast/gateway/v1/WatchAlgoEventsRequest';

export interface AlgorithmicTradingServiceClient extends grpc.Client {
  CancelAlgoOrder(argument: _ballast_gateway_v1_CancelAlgoOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  CancelAlgoOrder(argument: _ballast_gateway_v1_CancelAlgoOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  CancelAlgoOrder(argument: _ballast_gateway_v1_CancelAlgoOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  CancelAlgoOrder(argument: _ballast_gateway_v1_CancelAlgoOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  cancelAlgoOrder(argument: _ballast_gateway_v1_CancelAlgoOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  cancelAlgoOrder(argument: _ballast_gateway_v1_CancelAlgoOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  cancelAlgoOrder(argument: _ballast_gateway_v1_CancelAlgoOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  cancelAlgoOrder(argument: _ballast_gateway_v1_CancelAlgoOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  
  GetAlgoCapabilities(argument: _ballast_gateway_v1_GetAlgoCapabilitiesRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoCapabilities__Output>): grpc.ClientUnaryCall;
  GetAlgoCapabilities(argument: _ballast_gateway_v1_GetAlgoCapabilitiesRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoCapabilities__Output>): grpc.ClientUnaryCall;
  GetAlgoCapabilities(argument: _ballast_gateway_v1_GetAlgoCapabilitiesRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoCapabilities__Output>): grpc.ClientUnaryCall;
  GetAlgoCapabilities(argument: _ballast_gateway_v1_GetAlgoCapabilitiesRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoCapabilities__Output>): grpc.ClientUnaryCall;
  getAlgoCapabilities(argument: _ballast_gateway_v1_GetAlgoCapabilitiesRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoCapabilities__Output>): grpc.ClientUnaryCall;
  getAlgoCapabilities(argument: _ballast_gateway_v1_GetAlgoCapabilitiesRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoCapabilities__Output>): grpc.ClientUnaryCall;
  getAlgoCapabilities(argument: _ballast_gateway_v1_GetAlgoCapabilitiesRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoCapabilities__Output>): grpc.ClientUnaryCall;
  getAlgoCapabilities(argument: _ballast_gateway_v1_GetAlgoCapabilitiesRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoCapabilities__Output>): grpc.ClientUnaryCall;
  
  GetAlgoOrder(argument: _ballast_gateway_v1_GetAlgoOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  GetAlgoOrder(argument: _ballast_gateway_v1_GetAlgoOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  GetAlgoOrder(argument: _ballast_gateway_v1_GetAlgoOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  GetAlgoOrder(argument: _ballast_gateway_v1_GetAlgoOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  getAlgoOrder(argument: _ballast_gateway_v1_GetAlgoOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  getAlgoOrder(argument: _ballast_gateway_v1_GetAlgoOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  getAlgoOrder(argument: _ballast_gateway_v1_GetAlgoOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  getAlgoOrder(argument: _ballast_gateway_v1_GetAlgoOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  
  ListAlgoSubOrders(argument: _ballast_gateway_v1_ListAlgoSubOrdersRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>): grpc.ClientUnaryCall;
  ListAlgoSubOrders(argument: _ballast_gateway_v1_ListAlgoSubOrdersRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>): grpc.ClientUnaryCall;
  ListAlgoSubOrders(argument: _ballast_gateway_v1_ListAlgoSubOrdersRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>): grpc.ClientUnaryCall;
  ListAlgoSubOrders(argument: _ballast_gateway_v1_ListAlgoSubOrdersRequest, callback: grpc.requestCallback<_ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>): grpc.ClientUnaryCall;
  listAlgoSubOrders(argument: _ballast_gateway_v1_ListAlgoSubOrdersRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>): grpc.ClientUnaryCall;
  listAlgoSubOrders(argument: _ballast_gateway_v1_ListAlgoSubOrdersRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>): grpc.ClientUnaryCall;
  listAlgoSubOrders(argument: _ballast_gateway_v1_ListAlgoSubOrdersRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>): grpc.ClientUnaryCall;
  listAlgoSubOrders(argument: _ballast_gateway_v1_ListAlgoSubOrdersRequest, callback: grpc.requestCallback<_ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>): grpc.ClientUnaryCall;
  
  SubmitAlgoOrder(argument: _ballast_gateway_v1_SubmitAlgoOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  SubmitAlgoOrder(argument: _ballast_gateway_v1_SubmitAlgoOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  SubmitAlgoOrder(argument: _ballast_gateway_v1_SubmitAlgoOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  SubmitAlgoOrder(argument: _ballast_gateway_v1_SubmitAlgoOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  submitAlgoOrder(argument: _ballast_gateway_v1_SubmitAlgoOrderRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  submitAlgoOrder(argument: _ballast_gateway_v1_SubmitAlgoOrderRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  submitAlgoOrder(argument: _ballast_gateway_v1_SubmitAlgoOrderRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  submitAlgoOrder(argument: _ballast_gateway_v1_SubmitAlgoOrderRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AlgoOrderSnapshot__Output>): grpc.ClientUnaryCall;
  
  WatchAlgoEvents(argument: _ballast_gateway_v1_WatchAlgoEventsRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_AlgoEvent__Output>;
  WatchAlgoEvents(argument: _ballast_gateway_v1_WatchAlgoEventsRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_AlgoEvent__Output>;
  watchAlgoEvents(argument: _ballast_gateway_v1_WatchAlgoEventsRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_AlgoEvent__Output>;
  watchAlgoEvents(argument: _ballast_gateway_v1_WatchAlgoEventsRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_AlgoEvent__Output>;
  
}

export interface AlgorithmicTradingServiceHandlers extends grpc.UntypedServiceImplementation {
  CancelAlgoOrder: grpc.handleUnaryCall<_ballast_gateway_v1_CancelAlgoOrderRequest__Output, _ballast_gateway_v1_AlgoOrderSnapshot>;
  
  GetAlgoCapabilities: grpc.handleUnaryCall<_ballast_gateway_v1_GetAlgoCapabilitiesRequest__Output, _ballast_gateway_v1_AlgoCapabilities>;
  
  GetAlgoOrder: grpc.handleUnaryCall<_ballast_gateway_v1_GetAlgoOrderRequest__Output, _ballast_gateway_v1_AlgoOrderSnapshot>;
  
  ListAlgoSubOrders: grpc.handleUnaryCall<_ballast_gateway_v1_ListAlgoSubOrdersRequest__Output, _ballast_gateway_v1_ListAlgoSubOrdersResponse>;
  
  SubmitAlgoOrder: grpc.handleUnaryCall<_ballast_gateway_v1_SubmitAlgoOrderRequest__Output, _ballast_gateway_v1_AlgoOrderSnapshot>;
  
  WatchAlgoEvents: grpc.handleServerStreamingCall<_ballast_gateway_v1_WatchAlgoEventsRequest__Output, _ballast_gateway_v1_AlgoEvent>;
  
}

export interface AlgorithmicTradingServiceDefinition extends grpc.ServiceDefinition {
  CancelAlgoOrder: MethodDefinition<_ballast_gateway_v1_CancelAlgoOrderRequest, _ballast_gateway_v1_AlgoOrderSnapshot, _ballast_gateway_v1_CancelAlgoOrderRequest__Output, _ballast_gateway_v1_AlgoOrderSnapshot__Output>
  GetAlgoCapabilities: MethodDefinition<_ballast_gateway_v1_GetAlgoCapabilitiesRequest, _ballast_gateway_v1_AlgoCapabilities, _ballast_gateway_v1_GetAlgoCapabilitiesRequest__Output, _ballast_gateway_v1_AlgoCapabilities__Output>
  GetAlgoOrder: MethodDefinition<_ballast_gateway_v1_GetAlgoOrderRequest, _ballast_gateway_v1_AlgoOrderSnapshot, _ballast_gateway_v1_GetAlgoOrderRequest__Output, _ballast_gateway_v1_AlgoOrderSnapshot__Output>
  ListAlgoSubOrders: MethodDefinition<_ballast_gateway_v1_ListAlgoSubOrdersRequest, _ballast_gateway_v1_ListAlgoSubOrdersResponse, _ballast_gateway_v1_ListAlgoSubOrdersRequest__Output, _ballast_gateway_v1_ListAlgoSubOrdersResponse__Output>
  SubmitAlgoOrder: MethodDefinition<_ballast_gateway_v1_SubmitAlgoOrderRequest, _ballast_gateway_v1_AlgoOrderSnapshot, _ballast_gateway_v1_SubmitAlgoOrderRequest__Output, _ballast_gateway_v1_AlgoOrderSnapshot__Output>
  WatchAlgoEvents: MethodDefinition<_ballast_gateway_v1_WatchAlgoEventsRequest, _ballast_gateway_v1_AlgoEvent, _ballast_gateway_v1_WatchAlgoEventsRequest__Output, _ballast_gateway_v1_AlgoEvent__Output>
}
