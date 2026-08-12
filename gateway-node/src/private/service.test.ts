import assert from "node:assert/strict";
import test from "node:test";
import * as grpc from "@grpc/grpc-js";
import {
  AuthenticationError,
  NetworkError,
  NotSupported,
  OrderNotFound,
  RateLimitExceeded,
} from "ccxt";
import pino from "pino";

import { AccountEnvironment } from "../generated/ballast/gateway/v1/AccountEnvironment.js";
import { Exchange } from "../generated/ballast/gateway/v1/Exchange.js";
import { MarketKind } from "../generated/ballast/gateway/v1/MarketKind.js";
import type { GetOrderByClientIdRequest__Output } from "../generated/ballast/gateway/v1/GetOrderByClientIdRequest.js";
import { PrivateAccountRegistry } from "./accounts.js";
import { PrivateAccountService, snapshotServiceError } from "./service.js";

test("account snapshot fails closed when no secret-backed account is configured", async () => {
  const service = new PrivateAccountService(new PrivateAccountRegistry([]), 1_000, pino({ enabled: false }));
  await assert.rejects(
    service.getSnapshot({
      account: {
        accountId: "okx-demo",
        exchange: Exchange.EXCHANGE_OKX,
        environment: AccountEnvironment.ACCOUNT_ENVIRONMENT_DEMO,
      },
      requestId: "request-1",
    }),
    (error: unknown) => error instanceof Error
      && "code" in error
      && error.code === 9
      && error.message === "private_account_not_configured",
  );
});

test("account snapshot requires a request id", async () => {
  const service = new PrivateAccountService(new PrivateAccountRegistry([]), 1_000, pino({ enabled: false }));
  await assert.rejects(
    service.getSnapshot({ requestId: "" }),
    (error: unknown) => error instanceof Error && error.message === "request_id_required",
  );
});

test("order query strictly validates account, instrument, request, and client ids", async () => {
  const registry = new PrivateAccountRegistry([{
    accountId: "okx-demo",
    exchange: "okx",
    environment: "demo",
    apiKey: "api-key",
    secret: "secret",
    password: "passphrase",
  }]);
  const service = new PrivateAccountService(registry, 1_000, pino({ enabled: false }));
  const valid = {
    account: {
      accountId: "okx-demo",
      exchange: Exchange.EXCHANGE_OKX,
      environment: AccountEnvironment.ACCOUNT_ENVIRONMENT_DEMO,
    },
    instrument: { exchange: Exchange.EXCHANGE_OKX, marketKind: MarketKind.MARKET_KIND_SPOT, symbol: "BTC/USDT" },
    requestId: "request-1",
    clientOrderId: "clientOrder1",
  };

  for (const [request, message] of [
    [{ ...valid, requestId: " request-1" }, "request_id_required"],
    [{ ...valid, clientOrderId: "client-order-1" }, "client_order_id_invalid"],
    [{ ...valid, account: { ...valid.account, environment: AccountEnvironment.ACCOUNT_ENVIRONMENT_PRODUCTION } }, "private_account_reference_mismatch"],
    [{ ...valid, instrument: { ...valid.instrument, exchange: Exchange.EXCHANGE_BINANCE } }, "private_order_exchange_invalid"],
    [{ ...valid, instrument: { ...valid.instrument, marketKind: 0 } }, "private_order_market_kind_invalid"],
    [{ ...valid, instrument: { ...valid.instrument, symbol: " BTC/USDT" } }, "private_order_symbol_invalid"],
  ] as readonly [GetOrderByClientIdRequest__Output, string][]) {
    await assert.rejects(service.getOrderByClientId(request), (error: unknown) =>
      error instanceof Error && error.message === message && "code" in error && error.code === grpc.status.INVALID_ARGUMENT,
    );
  }
});

test("snapshot errors preserve transient and permanent failure semantics", () => {
  assert.deepEqual(errorSummary(snapshotServiceError(new OrderNotFound("secret response"))), {
    code: grpc.status.NOT_FOUND,
    details: "private_order_not_found",
  });
  assert.deepEqual(errorSummary(snapshotServiceError(new RateLimitExceeded("secret response"))), {
    code: grpc.status.RESOURCE_EXHAUSTED,
    details: "private_account_rate_limited",
  });
  assert.deepEqual(errorSummary(snapshotServiceError(new NetworkError("secret response"))), {
    code: grpc.status.UNAVAILABLE,
    details: "private_account_unavailable",
  });
  assert.deepEqual(errorSummary(snapshotServiceError(new AuthenticationError("secret response"))), {
    code: grpc.status.FAILED_PRECONDITION,
    details: "private_account_access_denied",
  });
  assert.deepEqual(errorSummary(snapshotServiceError(new NotSupported("secret response"))), {
    code: grpc.status.FAILED_PRECONDITION,
    details: "private_account_capability_unsupported",
  });
  assert.deepEqual(errorSummary(snapshotServiceError(new Error("position.contracts_invalid"))), {
    code: grpc.status.INTERNAL,
    details: "private_account_protocol_error",
  });
});

function errorSummary(error: grpc.ServiceError): { code: number; details: string } {
  return { code: error.code, details: error.details };
}
