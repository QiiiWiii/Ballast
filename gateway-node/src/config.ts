export interface GatewayConfig {
  readonly bind: string;
  readonly logLevel: string;
  readonly exchangeTimeoutMs: number;
  readonly streamStaleAfterMs: number;
  readonly okxAccountsFile?: string;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): GatewayConfig {
  const bind = env.BALLAST_GATEWAY_BIND ?? "0.0.0.0:50051";
  const portText = bind.slice(bind.lastIndexOf(":") + 1);
  const port = Number(portText);

  if (!bind.includes(":") || !Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error(`invalid BALLAST_GATEWAY_BIND: ${bind}`);
  }

  const okxAccountsFile = optionalText(env.BALLAST_OKX_ACCOUNTS_FILE);
  return {
    bind,
    logLevel: env.LOG_LEVEL ?? "info",
    exchangeTimeoutMs: positiveInteger(env.EXCHANGE_TIMEOUT_MS, 15_000),
    streamStaleAfterMs: positiveInteger(env.STREAM_STALE_AFTER_MS, 30_000),
    ...(okxAccountsFile === undefined ? {} : { okxAccountsFile }),
  };
}

function optionalText(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized ? normalized : undefined;
}

function positiveInteger(value: string | undefined, fallback: number): number {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`expected a positive integer, received: ${value}`);
  }
  return parsed;
}
