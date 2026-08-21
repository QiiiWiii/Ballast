import { randomUUID } from "node:crypto";
import ccxt from "ccxt";
import type {
  Balances as CcxtBalances,
  Exchange as CcxtExchange,
  Order as CcxtOrder,
  Position as CcxtPosition,
  Trade as CcxtTrade,
} from "ccxt";
import type { AccountEvent } from "../generated/ballast/gateway/v1/AccountEvent.js";
import type { OrderEvent } from "../generated/ballast/gateway/v1/OrderEvent.js";
import { StreamState } from "../generated/ballast/gateway/v1/StreamState.js";
import { AccountEnvironment } from "../generated/ballast/gateway/v1/AccountEnvironment.js";
import { Exchange } from "../generated/ballast/gateway/v1/Exchange.js";
import { normalizeBalances, normalizeOrder, normalizePosition } from "./okx-account.js";
import type { PrivateAccountConfig } from "./accounts.js";

type ProExchange = CcxtExchange & {
  watchBalance(params?: Record<string, unknown>): Promise<CcxtBalances>;
  watchPositions(
    symbols?: string[],
    since?: number,
    limit?: number,
    params?: Record<string, unknown>,
  ): Promise<CcxtPosition[]>;
  watchOrders(
    symbol?: string,
    since?: number,
    limit?: number,
    params?: Record<string, unknown>,
  ): Promise<CcxtOrder[]>;
  watchMyTrades(
    symbol?: string,
    since?: number,
    limit?: number,
    params?: Record<string, unknown>,
  ): Promise<CcxtTrade[]>;
};

type StreamMode = "account" | "orders";
type EventSink = (event: AccountEvent | OrderEvent) => void;

export class OkxPrivateStream {
  readonly #account: PrivateAccountConfig;
  readonly #timeoutMs: number;

  constructor(account: PrivateAccountConfig, timeoutMs: number) {
    this.#account = account;
    this.#timeoutMs = timeoutMs;
  }

  async run(mode: StreamMode, sink: EventSink, signal: AbortSignal): Promise<void> {
    const OkxPro = (ccxt.pro as unknown as { okx: new (config?: Record<string, unknown>) => ProExchange }).okx;
    const client = new OkxPro({
      apiKey: this.#account.apiKey,
      secret: this.#account.secret,
      password: this.#account.password,
      enableRateLimit: true,
      timeout: this.#timeoutMs,
    });
    (client as unknown as { number: StringConstructor }).number = String;
    if (this.#account.environment === "demo") client.setSandboxMode(true);

    let reconnectAttempt = 0;
    let connected = false;
    try {
      await client.loadMarkets();
      sink(this.status(mode, StreamState.STREAM_STATE_CONNECTING, reconnectAttempt));
      while (!signal.aborted) {
        try {
          const result = await this.next(client, mode);
          if (!connected) {
            connected = true;
            reconnectAttempt = 0;
            sink(this.status(mode, StreamState.STREAM_STATE_CONNECTED, reconnectAttempt));
          }
          for (const event of this.eventsFromResult(result, mode, client)) sink(event);
        } catch (error) {
          connected = false;
          reconnectAttempt += 1;
          sink(this.status(mode, StreamState.STREAM_STATE_RECONNECTING, reconnectAttempt, errorCode(error)));
          await delay(Math.min(30_000, 1_000 * 2 ** Math.min(reconnectAttempt - 1, 5)));
        }
      }
    } finally {
      await client.close();
      sink(this.status(mode, StreamState.STREAM_STATE_CLOSED, reconnectAttempt));
    }
  }

  private async next(client: ProExchange, mode: StreamMode): Promise<PrivateResult> {
    const watches: Promise<PrivateResult>[] = [
      client.watchOrders().then((orders) => ({ kind: "orders", value: orders })),
      client.watchMyTrades().then((trades) => ({ kind: "trades", value: trades })),
    ];
    if (mode === "account") {
      watches.push(client.watchBalance().then((balances) => ({ kind: "balance", value: balances })));
      watches.push(client.watchPositions().then((positions) => ({ kind: "positions", value: positions })));
    }
    return Promise.race(watches);
  }

  private eventsFromResult(
    result: PrivateResult,
    mode: StreamMode,
    client: ProExchange,
  ): (AccountEvent | OrderEvent)[] {
    const receivedAt = Date.now().toString();
    if (result.kind === "balance") {
      return normalizeBalances(result.value).map((balance) => ({
        eventId: randomUUID(),
        account: accountRef(this.#account),
        balance,
        gatewayReceivedAtMs: receivedAt,
        payload: "balance",
      } satisfies AccountEvent));
    }
    if (result.kind === "positions") {
      return result.value
        .filter((position) => hasOpenContracts(position))
        .map((position) => ({
          eventId: randomUUID(),
          account: accountRef(this.#account),
          position: normalizePosition(client, position),
          gatewayReceivedAtMs: receivedAt,
          payload: "position",
        } satisfies AccountEvent));
    }
    if (result.kind === "orders") {
      return result.value.flatMap((order) => {
        const normalized = normalizeOrder(this.#account, client, order, Number(receivedAt));
        return mode === "account"
          ? [{ eventId: randomUUID(), account: accountRef(this.#account), order: normalized, gatewayReceivedAtMs: receivedAt, payload: "order" } satisfies AccountEvent]
          : [{ order: normalized, payload: "order" } satisfies OrderEvent];
      });
    }
    return result.value.map((trade) => {
      const fill = normalizeFill(trade);
      return mode === "account"
        ? { eventId: fill.eventId ?? randomUUID(), account: accountRef(this.#account), fill, gatewayReceivedAtMs: receivedAt, payload: "fill" } satisfies AccountEvent
        : { fill, payload: "fill" } satisfies OrderEvent;
    });
  }

  private status(
    mode: StreamMode,
    state: StreamState,
    reconnectAttempt: number,
    errorCodeValue?: string,
  ): AccountEvent | OrderEvent {
    const status = {
      state,
      gatewayTimeMs: Date.now().toString(),
      reconnectAttempt,
      ...(errorCodeValue === undefined ? {} : { errorCode: errorCodeValue }),
    };
    return mode === "account"
      ? { eventId: randomUUID(), account: accountRef(this.#account), status, gatewayReceivedAtMs: Date.now().toString(), payload: "status" } satisfies AccountEvent
      : { status, payload: "status" } satisfies OrderEvent;
  }
}

type PrivateResult =
  | { kind: "balance"; value: CcxtBalances }
  | { kind: "positions"; value: CcxtPosition[] }
  | { kind: "orders"; value: CcxtOrder[] }
  | { kind: "trades"; value: CcxtTrade[] };

function normalizeFill(trade: CcxtTrade): NonNullable<AccountEvent["fill"]> {
  const info = isRecord(trade.info) ? trade.info : {};
  const clientOrderId = typeof info.clOrdId === "string" ? info.clOrdId : undefined;
  if (!trade.id || !clientOrderId || !trade.symbol || trade.price === undefined || trade.amount === undefined) {
    throw new Error("private_fill_identity_invalid");
  }
  const price = decimalText(trade.price, "fill.price");
  const quantity = decimalText(trade.amount, "fill.quantity");
  const timestamp = trade.timestamp;
  if (!timestamp || !Number.isSafeInteger(timestamp) || timestamp <= 0) throw new Error("fill.timestamp_invalid");
  const fee = trade.fee?.cost === undefined ? undefined : decimalText(trade.fee.cost, "fill.fee");
  return {
    eventId: `${trade.id}:${clientOrderId}:${timestamp}`,
    exchangeTradeId: trade.id,
    clientOrderId,
    price,
    quantity,
    ...(fee === undefined ? {} : { fee }),
    ...(trade.fee?.currency === undefined ? {} : { feeAsset: trade.fee.currency }),
    exchangeTimeMs: timestamp.toString(),
  };
}

function accountRef(account: PrivateAccountConfig) {
  return {
    accountId: account.accountId,
    exchange: Exchange.EXCHANGE_OKX,
    environment: account.environment === "demo"
      ? AccountEnvironment.ACCOUNT_ENVIRONMENT_DEMO
      : AccountEnvironment.ACCOUNT_ENVIRONMENT_PRODUCTION,
  };
}

function hasOpenContracts(position: CcxtPosition): boolean {
  return typeof position.contracts === "string" && position.contracts !== "0";
}

function decimalText(value: unknown, field: string): string {
  if (typeof value !== "string" || value.trim() === "") throw new Error(`${field}_invalid`);
  return value;
}

function errorCode(error: unknown): string {
  return error instanceof Error && error.name ? error.name : "private_stream_error";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
