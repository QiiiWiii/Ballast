import assert from "node:assert/strict";
import test from "node:test";
import * as grpc from "@grpc/grpc-js";
import {
  AuthenticationError,
  NetworkError,
  NotSupported,
  RateLimitExceeded,
} from "ccxt";
import pino from "pino";

import { AccountEnvironment } from "../generated/ballast/gateway/v1/AccountEnvironment.js";
import { Exchange } from "../generated/ballast/gateway/v1/Exchange.js";
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

test("snapshot errors preserve transient and permanent failure semantics", () => {
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
