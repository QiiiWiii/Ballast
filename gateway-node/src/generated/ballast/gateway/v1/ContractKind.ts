// Original file: ../proto/exchange_gateway.proto

export const ContractKind = {
  CONTRACT_KIND_UNSPECIFIED: 0,
  CONTRACT_KIND_LINEAR: 1,
  CONTRACT_KIND_INVERSE: 2,
} as const;

export type ContractKind =
  | 'CONTRACT_KIND_UNSPECIFIED'
  | 0
  | 'CONTRACT_KIND_LINEAR'
  | 1
  | 'CONTRACT_KIND_INVERSE'
  | 2

export type ContractKind__Output = typeof ContractKind[keyof typeof ContractKind]
