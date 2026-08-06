// Original file: ../proto/private_gateway.proto

import type * as grpc from '@grpc/grpc-js'
import type { MethodDefinition } from '@grpc/proto-loader'
import type { AccountEvent as _ballast_gateway_v1_AccountEvent, AccountEvent__Output as _ballast_gateway_v1_AccountEvent__Output } from '../../../ballast/gateway/v1/AccountEvent';
import type { AccountSnapshot as _ballast_gateway_v1_AccountSnapshot, AccountSnapshot__Output as _ballast_gateway_v1_AccountSnapshot__Output } from '../../../ballast/gateway/v1/AccountSnapshot';
import type { GetAccountSnapshotRequest as _ballast_gateway_v1_GetAccountSnapshotRequest, GetAccountSnapshotRequest__Output as _ballast_gateway_v1_GetAccountSnapshotRequest__Output } from '../../../ballast/gateway/v1/GetAccountSnapshotRequest';
import type { WatchAccountEventsRequest as _ballast_gateway_v1_WatchAccountEventsRequest, WatchAccountEventsRequest__Output as _ballast_gateway_v1_WatchAccountEventsRequest__Output } from '../../../ballast/gateway/v1/WatchAccountEventsRequest';

export interface AccountServiceClient extends grpc.Client {
  GetAccountSnapshot(argument: _ballast_gateway_v1_GetAccountSnapshotRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AccountSnapshot__Output>): grpc.ClientUnaryCall;
  GetAccountSnapshot(argument: _ballast_gateway_v1_GetAccountSnapshotRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AccountSnapshot__Output>): grpc.ClientUnaryCall;
  GetAccountSnapshot(argument: _ballast_gateway_v1_GetAccountSnapshotRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AccountSnapshot__Output>): grpc.ClientUnaryCall;
  GetAccountSnapshot(argument: _ballast_gateway_v1_GetAccountSnapshotRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AccountSnapshot__Output>): grpc.ClientUnaryCall;
  getAccountSnapshot(argument: _ballast_gateway_v1_GetAccountSnapshotRequest, metadata: grpc.Metadata, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AccountSnapshot__Output>): grpc.ClientUnaryCall;
  getAccountSnapshot(argument: _ballast_gateway_v1_GetAccountSnapshotRequest, metadata: grpc.Metadata, callback: grpc.requestCallback<_ballast_gateway_v1_AccountSnapshot__Output>): grpc.ClientUnaryCall;
  getAccountSnapshot(argument: _ballast_gateway_v1_GetAccountSnapshotRequest, options: grpc.CallOptions, callback: grpc.requestCallback<_ballast_gateway_v1_AccountSnapshot__Output>): grpc.ClientUnaryCall;
  getAccountSnapshot(argument: _ballast_gateway_v1_GetAccountSnapshotRequest, callback: grpc.requestCallback<_ballast_gateway_v1_AccountSnapshot__Output>): grpc.ClientUnaryCall;
  
  WatchAccountEvents(argument: _ballast_gateway_v1_WatchAccountEventsRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_AccountEvent__Output>;
  WatchAccountEvents(argument: _ballast_gateway_v1_WatchAccountEventsRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_AccountEvent__Output>;
  watchAccountEvents(argument: _ballast_gateway_v1_WatchAccountEventsRequest, metadata: grpc.Metadata, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_AccountEvent__Output>;
  watchAccountEvents(argument: _ballast_gateway_v1_WatchAccountEventsRequest, options?: grpc.CallOptions): grpc.ClientReadableStream<_ballast_gateway_v1_AccountEvent__Output>;
  
}

export interface AccountServiceHandlers extends grpc.UntypedServiceImplementation {
  GetAccountSnapshot: grpc.handleUnaryCall<_ballast_gateway_v1_GetAccountSnapshotRequest__Output, _ballast_gateway_v1_AccountSnapshot>;
  
  WatchAccountEvents: grpc.handleServerStreamingCall<_ballast_gateway_v1_WatchAccountEventsRequest__Output, _ballast_gateway_v1_AccountEvent>;
  
}

export interface AccountServiceDefinition extends grpc.ServiceDefinition {
  GetAccountSnapshot: MethodDefinition<_ballast_gateway_v1_GetAccountSnapshotRequest, _ballast_gateway_v1_AccountSnapshot, _ballast_gateway_v1_GetAccountSnapshotRequest__Output, _ballast_gateway_v1_AccountSnapshot__Output>
  WatchAccountEvents: MethodDefinition<_ballast_gateway_v1_WatchAccountEventsRequest, _ballast_gateway_v1_AccountEvent, _ballast_gateway_v1_WatchAccountEventsRequest__Output, _ballast_gateway_v1_AccountEvent__Output>
}
