import { readFileSync } from "node:fs";

export type AccountEnvironment = "demo" | "production";

export interface PrivateAccountConfig {
  readonly accountId: string;
  readonly exchange: "okx";
  readonly environment: AccountEnvironment;
  readonly apiKey: string;
  readonly secret: string;
  readonly password: string;
}

export class PrivateAccountRegistry {
  readonly #accounts = new Map<string, PrivateAccountConfig>();

  static fromFile(path: string | undefined): PrivateAccountRegistry {
    if (path === undefined) return new PrivateAccountRegistry([]);
    const raw = readFileSync(path, "utf8");
    const parsed = JSON.parse(raw) as unknown;
    if (!isRecord(parsed) || !Array.isArray(parsed.accounts)) {
      throw new Error("OKX account secret must contain an accounts array");
    }
    return new PrivateAccountRegistry(parsed.accounts.map(parseAccount));
  }

  constructor(accounts: readonly PrivateAccountConfig[]) {
    for (const account of accounts) {
      if (this.#accounts.has(account.accountId)) {
        throw new Error(`duplicate private account id: ${account.accountId}`);
      }
      this.#accounts.set(account.accountId, account);
    }
  }

  get(accountId: string): PrivateAccountConfig | undefined {
    return this.#accounts.get(accountId);
  }
}

function parseAccount(value: unknown): PrivateAccountConfig {
  if (!isRecord(value)) throw new Error("private account entry must be an object");
  const accountId = requiredText(value.account_id, "account_id");
  const exchange = requiredText(value.exchange, "exchange");
  const environment = requiredText(value.environment, "environment");
  if (exchange !== "okx") throw new Error(`unsupported private account exchange: ${exchange}`);
  if (environment !== "demo" && environment !== "production") {
    throw new Error(`unsupported private account environment: ${environment}`);
  }
  return {
    accountId,
    exchange,
    environment,
    apiKey: requiredText(value.api_key, "api_key"),
    secret: requiredText(value.secret, "secret"),
    password: requiredText(value.password, "password"),
  };
}

function requiredText(value: unknown, field: string): string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`private account ${field} is required`);
  }
  return value.trim();
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
