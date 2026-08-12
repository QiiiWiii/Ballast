import assert from "node:assert/strict";
import test from "node:test";

import { PrivateAccountRegistry } from "./accounts.js";

const account = {
  accountId: "okx-demo",
  exchange: "okx" as const,
  environment: "demo" as const,
  apiKey: "api-key",
  secret: "secret",
  password: "passphrase",
};

test("private account registry selects an explicitly configured account", () => {
  const registry = new PrivateAccountRegistry([account]);
  assert.equal(registry.get("okx-demo")?.environment, "demo");
  assert.equal(registry.get("missing"), undefined);
});

test("private account registry rejects duplicate account ids", () => {
  assert.throws(() => new PrivateAccountRegistry([account, account]), /duplicate private account id/);
});
