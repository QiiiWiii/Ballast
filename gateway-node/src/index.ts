import pino from "pino";

import { loadConfig } from "./config.js";
import { startGrpcServer } from "./grpc/server.js";

const config = loadConfig();
const logger = pino({ level: config.logLevel, name: "ballast-exchange-gateway" });
const server = await startGrpcServer(config);

logger.info({ bind: config.bind }, "exchange gateway listening");

let shuttingDown = false;
function shutdown(signal: NodeJS.Signals): void {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;
  logger.info({ signal }, "exchange gateway shutting down");
  server.tryShutdown((error?: Error) => {
    if (error) {
      logger.error({ error }, "graceful shutdown failed");
      server.forceShutdown();
      process.exitCode = 1;
      return;
    }
    process.exitCode = 0;
  });
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
