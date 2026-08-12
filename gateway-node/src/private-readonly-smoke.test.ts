import assert from "node:assert/strict";
import test from "node:test";

import { AccountEnvironment } from "./generated/ballast/gateway/v1/AccountEnvironment.js";
import { Exchange } from "./generated/ballast/gateway/v1/Exchange.js";
import { loadPrivateSmokeConfig, summarizeSnapshot } from "./private-readonly-smoke.js";

test("private smoke requires an explicit gateway and account reference", () => {
  assert.throws(() => loadPrivateSmokeConfig({}), /BALLAST_PRIVATE_SMOKE_GATEWAY is required/);
  assert.throws(() => loadPrivateSmokeConfig({
    BALLAST_PRIVATE_SMOKE_GATEWAY: "127.0.0.1:50051",
    BALLAST_PRIVATE_SMOKE_ACCOUNT_ID: "okx-demo",
  }), /BALLAST_PRIVATE_SMOKE_ACCOUNT_ENVIRONMENT is required/);
});

test("private smoke accepts only explicit OKX account environments", () => {
  assert.deepEqual(loadPrivateSmokeConfig({
    BALLAST_PRIVATE_SMOKE_GATEWAY: "127.0.0.1:50051",
    BALLAST_PRIVATE_SMOKE_ACCOUNT_ID: "okx-demo",
    BALLAST_PRIVATE_SMOKE_ACCOUNT_ENVIRONMENT: "demo",
  }), {
    gateway: "127.0.0.1:50051",
    accountId: "okx-demo",
    environment: "demo",
    timeoutMs: 15_000,
  });
  assert.throws(() => loadPrivateSmokeConfig({
    BALLAST_PRIVATE_SMOKE_GATEWAY: "127.0.0.1:50051",
    BALLAST_PRIVATE_SMOKE_ACCOUNT_ID: "okx-demo",
    BALLAST_PRIVATE_SMOKE_ACCOUNT_ENVIRONMENT: "paper",
  }), /must be demo or production/);
});

test("private smoke output exposes counts without financial values", () => {
  const summary = summarizeSnapshot({
    account: {
      accountId: "okx-demo",
      exchange: Exchange.EXCHANGE_OKX,
      environment: AccountEnvironment.ACCOUNT_ENVIRONMENT_DEMO,
    },
    balances: [{ asset: "USDT", total: "100", available: "90" }],
    positions: [],
    openOrders: [],
    exchangeTimeMs: 1_700_000_000_000 as never,
    gatewayReceivedAtMs: 1_700_000_000_001 as never,
  }, {
    gateway: "127.0.0.1:50051",
    accountId: "okx-demo",
    environment: "demo",
    timeoutMs: 15_000,
  });

  assert.equal(summary.balance_count, 1);
  assert.equal("balances" in summary, false);
  assert.equal("total" in summary, false);
  assert.equal("available" in summary, false);
});
