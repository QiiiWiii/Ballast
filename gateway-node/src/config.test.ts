import assert from "node:assert/strict";
import test from "node:test";

import { loadConfig } from "./config.js";

test("loads explicit gateway bind", () => {
  assert.deepEqual(
    loadConfig({ BALLAST_GATEWAY_BIND: "127.0.0.1:51000", LOG_LEVEL: "debug" }),
    {
      bind: "127.0.0.1:51000",
      logLevel: "debug",
      exchangeTimeoutMs: 15_000,
      streamStaleAfterMs: 30_000,
    },
  );
});

test("loads the optional OKX account secret file path", () => {
  assert.equal(
    loadConfig({ BALLAST_OKX_ACCOUNTS_FILE: "/run/secrets/ballast_okx_accounts" }).okxAccountsFile,
    "/run/secrets/ballast_okx_accounts",
  );
});

test("rejects an invalid gateway port", () => {
  assert.throws(() => loadConfig({ BALLAST_GATEWAY_BIND: "127.0.0.1:70000" }));
});
