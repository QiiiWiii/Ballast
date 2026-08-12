import * as grpc from "@grpc/grpc-js";
import { randomUUID } from "node:crypto";
import { pathToFileURL } from "node:url";

import { AccountEnvironment } from "./generated/ballast/gateway/v1/AccountEnvironment.js";
import type { AccountEnvironment as AccountEnvironmentValue } from "./generated/ballast/gateway/v1/AccountEnvironment.js";
import type { AccountSnapshot__Output } from "./generated/ballast/gateway/v1/AccountSnapshot.js";
import { Exchange } from "./generated/ballast/gateway/v1/Exchange.js";
import { accountService } from "./grpc/proto.js";

export interface PrivateSmokeConfig {
  readonly gateway: string;
  readonly accountId: string;
  readonly environment: "demo" | "production";
  readonly timeoutMs: number;
}

export function loadPrivateSmokeConfig(env: NodeJS.ProcessEnv): PrivateSmokeConfig {
  const gateway = requiredText(env.BALLAST_PRIVATE_SMOKE_GATEWAY, "BALLAST_PRIVATE_SMOKE_GATEWAY");
  const accountId = requiredText(env.BALLAST_PRIVATE_SMOKE_ACCOUNT_ID, "BALLAST_PRIVATE_SMOKE_ACCOUNT_ID");
  const environment = requiredText(
    env.BALLAST_PRIVATE_SMOKE_ACCOUNT_ENVIRONMENT,
    "BALLAST_PRIVATE_SMOKE_ACCOUNT_ENVIRONMENT",
  );
  if (environment !== "demo" && environment !== "production") {
    throw new Error("BALLAST_PRIVATE_SMOKE_ACCOUNT_ENVIRONMENT must be demo or production");
  }
  return {
    gateway,
    accountId,
    environment,
    timeoutMs: positiveInteger(env.BALLAST_PRIVATE_SMOKE_TIMEOUT_MS, 15_000),
  };
}

export function summarizeSnapshot(
  snapshot: AccountSnapshot__Output,
  config: PrivateSmokeConfig,
): Record<string, unknown> {
  const expectedEnvironment = accountEnvironment(config.environment);
  if (
    snapshot.account?.accountId !== config.accountId
    || snapshot.account.exchange !== Exchange.EXCHANGE_OKX
    || snapshot.account.environment !== expectedEnvironment
  ) {
    throw new Error("private smoke account reference mismatch");
  }
  if (!snapshot.balances || !snapshot.positions || !snapshot.openOrders) {
    throw new Error("private smoke snapshot collections are missing");
  }
  const exchangeTimeMs = Number(snapshot.exchangeTimeMs);
  const gatewayReceivedAtMs = Number(snapshot.gatewayReceivedAtMs);
  if (!Number.isSafeInteger(exchangeTimeMs) || exchangeTimeMs <= 0) {
    throw new Error("private smoke exchange timestamp is invalid");
  }
  if (!Number.isSafeInteger(gatewayReceivedAtMs) || gatewayReceivedAtMs <= 0) {
    throw new Error("private smoke gateway timestamp is invalid");
  }
  return {
    status: "ok",
    account_id: config.accountId,
    exchange: "okx",
    environment: config.environment,
    balance_count: snapshot.balances.length,
    position_count: snapshot.positions.length,
    open_order_count: snapshot.openOrders.length,
    exchange_time_ms: exchangeTimeMs,
    gateway_received_at_ms: gatewayReceivedAtMs,
  };
}

async function run(): Promise<void> {
  const config = loadPrivateSmokeConfig(process.env);
  const client = new accountService(config.gateway, grpc.credentials.createInsecure());
  try {
    const snapshot = await new Promise<AccountSnapshot__Output>((resolve, reject) => {
      client.getAccountSnapshot({
        account: {
          accountId: config.accountId,
          exchange: Exchange.EXCHANGE_OKX,
          environment: accountEnvironment(config.environment),
        },
        requestId: randomUUID(),
      }, { deadline: Date.now() + config.timeoutMs }, (error, response) => {
        if (error) reject(error);
        else if (!response) reject(new Error("private smoke response is missing"));
        else resolve(response);
      });
    });
    process.stdout.write(`${JSON.stringify(summarizeSnapshot(snapshot, config), null, 2)}\n`);
  } finally {
    client.close();
  }
}

function accountEnvironment(environment: PrivateSmokeConfig["environment"]): AccountEnvironmentValue {
  return environment === "demo"
    ? AccountEnvironment.ACCOUNT_ENVIRONMENT_DEMO
    : AccountEnvironment.ACCOUNT_ENVIRONMENT_PRODUCTION;
}

function requiredText(value: string | undefined, name: string): string {
  if (!value?.trim() || value !== value.trim()) throw new Error(`${name} is required`);
  return value;
}

function positiveInteger(value: string | undefined, fallback: number): number {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error("BALLAST_PRIVATE_SMOKE_TIMEOUT_MS must be a positive integer");
  }
  return parsed;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  run().catch((error: unknown) => {
    const message = error instanceof Error ? error.message : "private readonly smoke failed";
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  });
}
