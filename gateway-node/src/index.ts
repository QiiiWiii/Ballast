import pino from "pino";

import { loadConfig } from "./config.js";
import { startGrpcServer } from "./grpc/server.js";

const config = loadConfig();
const logger = pino({ level: config.logLevel, name: "ballast-exchange-gateway" });
const runtime = await startGrpcServer(config, logger);

logger.info({ bind: config.bind }, "exchange gateway listening");

let shuttingDown = false;
function shutdown(signal: NodeJS.Signals): void {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;
  logger.info({ signal }, "exchange gateway shutting down");
  runtime.server.tryShutdown((error?: Error) => {
    if (error) {
      logger.error({ error }, "graceful shutdown failed");
      runtime.server.forceShutdown();
      process.exitCode = 1;
      return;
    }
    void runtime.closeAdapters()
      .then(() => { process.exitCode = 0; })
      .catch((closeError: unknown) => {
        logger.error({ code: closeError instanceof Error ? closeError.name : "UnknownError" }, "gateway adapter shutdown failed");
        process.exitCode = 1;
      });
  });
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
