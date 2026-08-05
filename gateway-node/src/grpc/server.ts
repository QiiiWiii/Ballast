import * as grpc from "@grpc/grpc-js";

import type { GatewayConfig } from "../config.js";
import { exchangeGatewayService } from "./proto.js";

interface HealthResponse {
  readonly status: string;
  readonly serviceVersion: string;
  readonly gatewayTimeMs: string;
}

function unaryUnimplemented(
  _call: grpc.ServerUnaryCall<unknown, unknown>,
  callback: grpc.sendUnaryData<unknown>,
): void {
  callback({
    code: grpc.status.UNIMPLEMENTED,
    message: "gateway method is not implemented",
  });
}

function streamUnimplemented(call: grpc.ServerWritableStream<unknown, unknown>): void {
  const error = Object.assign(new Error("gateway stream is not implemented"), {
    code: grpc.status.UNIMPLEMENTED,
  });
  call.destroy(error);
}

export async function startGrpcServer(config: GatewayConfig): Promise<grpc.Server> {
  const server = new grpc.Server();
  const implementation = {
    health(
      _call: grpc.ServerUnaryCall<unknown, HealthResponse>,
      callback: grpc.sendUnaryData<HealthResponse>,
    ): void {
      callback(null, {
        status: "ok",
        serviceVersion: process.env.npm_package_version ?? "0.1.0",
        gatewayTimeMs: Date.now().toString(),
      });
    },
    listInstruments: unaryUnimplemented,
    getCapabilities: unaryUnimplemented,
    getOrderBook: unaryUnimplemented,
    streamTrades: streamUnimplemented,
    getBalances: unaryUnimplemented,
    getPositions: unaryUnimplemented,
    getOpenOrders: unaryUnimplemented,
    getOrder: unaryUnimplemented,
    createOrder: unaryUnimplemented,
    cancelOrder: unaryUnimplemented,
    streamOrderUpdates: streamUnimplemented,
    streamFills: streamUnimplemented,
  };

  server.addService(
    exchangeGatewayService,
    implementation as grpc.UntypedServiceImplementation,
  );

  await new Promise<void>((resolve, reject) => {
    server.bindAsync(config.bind, grpc.ServerCredentials.createInsecure(), (error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });

  return server;
}
