// Original file: ../proto/private_gateway.proto

export const AccountEnvironment = {
  ACCOUNT_ENVIRONMENT_UNSPECIFIED: 0,
  ACCOUNT_ENVIRONMENT_DEMO: 1,
  ACCOUNT_ENVIRONMENT_PRODUCTION: 2,
} as const;

export type AccountEnvironment =
  | 'ACCOUNT_ENVIRONMENT_UNSPECIFIED'
  | 0
  | 'ACCOUNT_ENVIRONMENT_DEMO'
  | 1
  | 'ACCOUNT_ENVIRONMENT_PRODUCTION'
  | 2

export type AccountEnvironment__Output = typeof AccountEnvironment[keyof typeof AccountEnvironment]
