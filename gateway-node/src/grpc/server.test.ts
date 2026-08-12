import assert from "node:assert/strict";
import test from "node:test";
import type * as grpc from "@grpc/grpc-js";
import pino from "pino";

import type { GetOrderByClientIdRequest__Output } from "../generated/ballast/gateway/v1/GetOrderByClientIdRequest.js";
import type { OrderSnapshot } from "../generated/ballast/gateway/v1/OrderSnapshot.js";
import { createTradingHandlers } from "./server.js";

test("trading server routes GetOrderByClientId while keeping mutations disabled", async () => {
  const expected = { clientOrderId: "clientOrder1" } as OrderSnapshot;
  let received: GetOrderByClientIdRequest__Output | undefined;
  const handlers = createTradingHandlers({
    getOrderByClientId: async (request) => {
      received = request;
      return expected;
    },
  }, pino({ enabled: false }));
  const request = { requestId: "request-1", clientOrderId: "clientOrder1" };

  const response = await new Promise<OrderSnapshot>((resolve, reject) => {
    handlers.GetOrderByClientId({ request } as never, (error, value) => {
      if (error) reject(error);
      else resolve(value as OrderSnapshot);
    });
  });
  assert.equal(received, request);
  assert.equal(response, expected);

  await new Promise<void>((resolve, reject) => {
    handlers.PlaceIocOrder({} as never, (error) => {
      try {
        assert.equal((error as grpc.ServiceError).details, "private_services_disabled");
        resolve();
      } catch (assertionError) {
        reject(assertionError);
      }
    });
  });
});
