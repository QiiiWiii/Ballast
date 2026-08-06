import type { Logger } from "pino";

import type { ExchangeId, MarketDataAdapter } from "./adapter.js";
import { CcxtMarketDataAdapter } from "./ccxt-adapter.js";

export interface AdapterStatus {
  readonly exchange: ExchangeId;
  readonly status: "idle" | "ready" | "degraded";
  readonly lastErrorCode?: string;
  readonly lastSuccessAtMs?: number;
}

const EXCHANGES: readonly ExchangeId[] = [
  "binance",
  "okx",
  "bybit",
  "gate_io",
  "bitget",
];

export class AdapterRegistry {
  readonly #adapters = new Map<ExchangeId, MarketDataAdapter>();
  readonly #statuses = new Map<ExchangeId, AdapterStatus>();
  readonly #logger: Logger;

  constructor(timeoutMs: number, logger: Logger) {
    this.#logger = logger;
    for (const exchange of EXCHANGES) {
      this.#adapters.set(exchange, new CcxtMarketDataAdapter(exchange, timeoutMs));
      this.#statuses.set(exchange, { exchange, status: "idle" });
    }
  }

  get(exchange: ExchangeId): MarketDataAdapter {
    const adapter = this.#adapters.get(exchange);
    if (!adapter) throw new Error(`unsupported exchange: ${exchange}`);
    return adapter;
  }

  statuses(): readonly AdapterStatus[] {
    return EXCHANGES.map((exchange) => this.#statuses.get(exchange) ?? { exchange, status: "idle" });
  }

  markSuccess(exchange: ExchangeId): void {
    this.#statuses.set(exchange, {
      exchange,
      status: "ready",
      lastSuccessAtMs: Date.now(),
    });
  }

  markFailure(exchange: ExchangeId, error: unknown): string {
    const code = errorCode(error);
    this.#statuses.set(exchange, compactOptional({
      exchange,
      status: "degraded",
      lastErrorCode: code,
      lastSuccessAtMs: this.#statuses.get(exchange)?.lastSuccessAtMs,
    }) as AdapterStatus);
    this.#logger.warn({ exchange, code, error }, "exchange adapter operation failed");
    return code;
  }

  async close(): Promise<void> {
    await Promise.allSettled([...this.#adapters.values()].map((adapter) => adapter.close()));
  }
}

function compactOptional<T extends Record<string, unknown>>(value: T): T {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined),
  ) as T;
}

function errorCode(error: unknown): string {
  if (error instanceof Error && error.name) return error.name.toUpperCase();
  return "UNKNOWN_ERROR";
}
