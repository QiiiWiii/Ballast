import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import { fileURLToPath } from "node:url";

const protoPath = fileURLToPath(
  new URL("../../../proto/exchange_gateway.proto", import.meta.url),
);

const definition = protoLoader.loadSync(protoPath, {
  defaults: true,
  enums: String,
  keepCase: false,
  longs: String,
  oneofs: true,
});

interface GatewayPackage {
  readonly ballast: {
    readonly gateway: {
      readonly v1: {
        readonly ExchangeGateway: grpc.ServiceClientConstructor;
      };
    };
  };
}

const loaded = grpc.loadPackageDefinition(definition) as unknown as GatewayPackage;

export const exchangeGatewayService = loaded.ballast.gateway.v1.ExchangeGateway.service;
