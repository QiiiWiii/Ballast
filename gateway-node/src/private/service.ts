import * as grpc from "@grpc/grpc-js";
import {
  AccountNotEnabled,
  AccountSuspended,
  AuthenticationError,
  DDoSProtection,
  ExchangeNotAvailable,
  NetworkError,
  NotSupported,
  OrderNotFound,
  PermissionDenied,
  RateLimitExceeded,
  RequestTimeout,
  RestrictedLocation,
} from "ccxt";
import type { Logger } from "pino";

import type { GetAccountSnapshotRequest__Output } from "../generated/ballast/gateway/v1/GetAccountSnapshotRequest.js";
import type { AccountSnapshot } from "../generated/ballast/gateway/v1/AccountSnapshot.js";
import type { GetOrderByClientIdRequest__Output } from "../generated/ballast/gateway/v1/GetOrderByClientIdRequest.js";
import type { OrderSnapshot } from "../generated/ballast/gateway/v1/OrderSnapshot.js";
import { AccountEnvironment } from "../generated/ballast/gateway/v1/AccountEnvironment.js";
import { Exchange } from "../generated/ballast/gateway/v1/Exchange.js";
import { MarketKind } from "../generated/ballast/gateway/v1/MarketKind.js";
import { PrivateAccountRegistry } from "./accounts.js";
import { OkxAccountAdapter } from "./okx-account.js";

export class PrivateAccountService {
  readonly #registry: PrivateAccountRegistry;
  readonly #timeoutMs: number;
  readonly #logger: Logger;
  readonly #adapters = new Map<string, OkxAccountAdapter>();

  constructor(registry: PrivateAccountRegistry, timeoutMs: number, logger: Logger) {
    this.#registry = registry;
    this.#timeoutMs = timeoutMs;
    this.#logger = logger;
  }

  async getSnapshot(request: GetAccountSnapshotRequest__Output): Promise<AccountSnapshot> {
    requireRequestId(request.requestId);
    const { account, adapter } = this.#validatedAccount(request.account);
    try {
      return await adapter.snapshot();
    } catch (error) {
      this.#logger.warn({ accountId: account.accountId, code: errorCode(error) }, "private account snapshot failed");
      throw snapshotServiceError(error);
    }
  }

  async getOrderByClientId(request: GetOrderByClientIdRequest__Output): Promise<OrderSnapshot> {
    requireRequestId(request.requestId);
    const clientOrderId = requireClientOrderId(request.clientOrderId);
    const instrument = request.instrument;
    if (!instrument) throw serviceError(grpc.status.INVALID_ARGUMENT, "instrument_required");
    if (instrument.exchange !== Exchange.EXCHANGE_OKX) {
      throw serviceError(grpc.status.INVALID_ARGUMENT, "private_order_exchange_invalid");
    }
    if (instrument.marketKind !== MarketKind.MARKET_KIND_SPOT
      && instrument.marketKind !== MarketKind.MARKET_KIND_PERPETUAL) {
      throw serviceError(grpc.status.INVALID_ARGUMENT, "private_order_market_kind_invalid");
    }
    const symbol = instrument.symbol?.trim();
    if (!symbol || symbol !== instrument.symbol) {
      throw serviceError(grpc.status.INVALID_ARGUMENT, "private_order_symbol_invalid");
    }
    const { account, adapter } = this.#validatedAccount(request.account);
    try {
      return await adapter.getOrderByClientId(symbol, instrument.marketKind, clientOrderId);
    } catch (error) {
      this.#logger.warn({ accountId: account.accountId, code: errorCode(error) }, "private order query failed");
      throw snapshotServiceError(error);
    }
  }

  #validatedAccount(ref: GetAccountSnapshotRequest__Output["account"]): {
    account: NonNullable<ReturnType<PrivateAccountRegistry["get"]>>;
    adapter: OkxAccountAdapter;
  } {
    if (!ref?.accountId?.trim() || ref.accountId !== ref.accountId.trim()) {
      throw serviceError(grpc.status.INVALID_ARGUMENT, "account_required");
    }
    const account = this.#registry.get(ref.accountId);
    if (!account) throw serviceError(grpc.status.FAILED_PRECONDITION, "private_account_not_configured");
    const expectedEnvironment = account.environment === "demo"
      ? AccountEnvironment.ACCOUNT_ENVIRONMENT_DEMO
      : AccountEnvironment.ACCOUNT_ENVIRONMENT_PRODUCTION;
    if (ref.exchange !== Exchange.EXCHANGE_OKX || ref.environment !== expectedEnvironment) {
      throw serviceError(grpc.status.INVALID_ARGUMENT, "private_account_reference_mismatch");
    }
    let adapter = this.#adapters.get(account.accountId);
    try {
      if (!adapter) {
        adapter = new OkxAccountAdapter(account, this.#timeoutMs);
        this.#adapters.set(account.accountId, adapter);
      }
    } catch (error) {
      this.#logger.warn({ accountId: account.accountId, code: errorCode(error) }, "private account initialization failed");
      throw serviceError(grpc.status.FAILED_PRECONDITION, "private_account_initialization_failed");
    }
    return { account, adapter };
  }

  async close(): Promise<void> {
    const adapters = [...this.#adapters.values()];
    this.#adapters.clear();
    await Promise.all(adapters.map((adapter) => adapter.close()));
  }
}

export function snapshotServiceError(error: unknown): grpc.ServiceError {
  if (error instanceof OrderNotFound) {
    return serviceError(grpc.status.NOT_FOUND, "private_order_not_found");
  }
  if (error instanceof RateLimitExceeded || error instanceof DDoSProtection) {
    return serviceError(grpc.status.RESOURCE_EXHAUSTED, "private_account_rate_limited");
  }
  if (
    error instanceof AuthenticationError
    || error instanceof PermissionDenied
    || error instanceof AccountNotEnabled
    || error instanceof AccountSuspended
    || error instanceof RestrictedLocation
  ) {
    return serviceError(grpc.status.FAILED_PRECONDITION, "private_account_access_denied");
  }
  if (
    error instanceof RequestTimeout
    || error instanceof ExchangeNotAvailable
    || error instanceof NetworkError
  ) {
    return serviceError(grpc.status.UNAVAILABLE, "private_account_unavailable");
  }
  if (error instanceof NotSupported || isCapabilityError(error)) {
    return serviceError(grpc.status.FAILED_PRECONDITION, "private_account_capability_unsupported");
  }
  return serviceError(grpc.status.INTERNAL, "private_account_protocol_error");
}

function serviceError(code: number, details: string): grpc.ServiceError {
  return Object.assign(new Error(details), { name: "PrivateAccountError", code, details, metadata: new grpc.Metadata() });
}

function errorCode(error: unknown): string {
  return error instanceof Error && error.name ? error.name : "UnknownError";
}

function isCapabilityError(error: unknown): boolean {
  return error instanceof Error && [
    "fetch_positions_not_supported",
    "fetch_open_orders_not_supported",
    "fetch_order_not_supported",
    "private_position_market_kind_invalid",
    "private_order_market_kind_invalid",
    "private_algo_open_orders_unsupported",
  ].includes(error.message);
}

function requireRequestId(value: string | undefined): void {
  if (!value?.trim() || value !== value.trim()) {
    throw serviceError(grpc.status.INVALID_ARGUMENT, "request_id_required");
  }
}

function requireClientOrderId(value: string | undefined): string {
  if (!value || !/^[A-Za-z0-9]{1,32}$/.test(value)) {
    throw serviceError(grpc.status.INVALID_ARGUMENT, "client_order_id_invalid");
  }
  return value;
}
