// Original file: ../proto/exchange_gateway.proto

export const Side = {
  SIDE_UNSPECIFIED: 0,
  SIDE_BUY: 1,
  SIDE_SELL: 2,
} as const;

export type Side =
  | 'SIDE_UNSPECIFIED'
  | 0
  | 'SIDE_BUY'
  | 1
  | 'SIDE_SELL'
  | 2

export type Side__Output = typeof Side[keyof typeof Side]
