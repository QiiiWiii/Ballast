import assert from "node:assert/strict";
import test from "node:test";
import pino from "pino";

import type { GetOrderByClientIdRequest__Output } from "../generated/ballast/gateway/v1/GetOrderByClientIdRequest.js";
import type { OrderSnapshot } from "../generated/ballast/gateway/v1/OrderSnapshot.js";
import { AdapterRegistry } from "../exchanges/registry.js";
import { createTradingHandlers, healthResponse } from "./server.js";

test("health reports the packaged gateway version without npm environment variables", () => {
  const previous = process.env.npm_package_version;
  delete process.env.npm_package_version;
  try {
    const response = healthResponse(new AdapterRegistry(1_000, pino({ enabled: false })));
    assert.equal(response.serviceVersion, "0.1.1");
  } finally {
    if (previous === undefined) delete process.env.npm_package_version;
    else process.env.npm_package_version = previous;
  }
});

test("trading server routes private order commands", async () => {
  const expected = { clientOrderId: "clientOrder1" } as OrderSnapshot;
  let received: GetOrderByClientIdRequest__Output | undefined;
  let placed = false;
  const handlers = createTradingHandlers({
    placeIocOrder: async () => { placed = true; return expected; },
    getOrderByClientId: async (request) => {
      received = request;
      return expected;
    },
    cancelOrder: async () => expected,
    watchOrderEvents: async () => undefined,
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
    handlers.PlaceIocOrder({} as never, (error, value) => {
      try {
        if (error) reject(error);
        else {
          assert.equal(value, expected);
          assert.equal(placed, true);
          resolve();
        }
      } catch (assertionError) {
        reject(assertionError);
      }
    });
  });
});
