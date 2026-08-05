export interface GatewayConfig {
  readonly bind: string;
  readonly logLevel: string;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): GatewayConfig {
  const bind = env.BALLAST_GATEWAY_BIND ?? "0.0.0.0:50051";
  const portText = bind.slice(bind.lastIndexOf(":") + 1);
  const port = Number(portText);

  if (!bind.includes(":") || !Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error(`invalid BALLAST_GATEWAY_BIND: ${bind}`);
  }

  return {
    bind,
    logLevel: env.LOG_LEVEL ?? "info",
  };
}
