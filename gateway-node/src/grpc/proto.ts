import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import { fileURLToPath } from "node:url";

import type { ProtoGrpcType as MarketProtoGrpcType } from "../generated/exchange_gateway.js";
import type { ProtoGrpcType as PrivateProtoGrpcType } from "../generated/private_gateway.js";

const marketProtoPath = fileURLToPath(
  new URL("../../../proto/exchange_gateway.proto", import.meta.url),
);
const privateProtoPath = fileURLToPath(
  new URL("../../../proto/private_gateway.proto", import.meta.url),
);

const definition = protoLoader.loadSync([marketProtoPath, privateProtoPath], {
  defaults: true,
  enums: Number,
  keepCase: false,
  longs: String,
  oneofs: true,
});

const loaded = grpc.loadPackageDefinition(definition) as unknown as MarketProtoGrpcType & PrivateProtoGrpcType;

export const marketDataService = loaded.ballast.gateway.v1.MarketDataService;
export const accountService = loaded.ballast.gateway.v1.AccountService;
export const tradingService = loaded.ballast.gateway.v1.TradingService;
export const algorithmicTradingService = loaded.ballast.gateway.v1.AlgorithmicTradingService;
