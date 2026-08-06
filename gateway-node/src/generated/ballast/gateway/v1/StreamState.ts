// Original file: ../proto/exchange_gateway.proto

export const StreamState = {
  STREAM_STATE_UNSPECIFIED: 0,
  STREAM_STATE_CONNECTING: 1,
  STREAM_STATE_CONNECTED: 2,
  STREAM_STATE_RECONNECTING: 3,
  STREAM_STATE_STALE: 4,
  STREAM_STATE_CLOSED: 5,
} as const;

export type StreamState =
  | 'STREAM_STATE_UNSPECIFIED'
  | 0
  | 'STREAM_STATE_CONNECTING'
  | 1
  | 'STREAM_STATE_CONNECTED'
  | 2
  | 'STREAM_STATE_RECONNECTING'
  | 3
  | 'STREAM_STATE_STALE'
  | 4
  | 'STREAM_STATE_CLOSED'
  | 5

export type StreamState__Output = typeof StreamState[keyof typeof StreamState]
