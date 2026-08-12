import assert from "node:assert/strict";
import test from "node:test";

import { Exchange } from "./generated/ballast/gateway/v1/Exchange.js";
import { algoCapabilities, tradingCapabilities } from "./grpc/server.js";

test("only OKX advertises query-by-client-order-id capability", () => {
  for (const exchange of [
    Exchange.EXCHANGE_BINANCE,
    Exchange.EXCHANGE_OKX,
    Exchange.EXCHANGE_BYBIT,
    Exchange.EXCHANGE_GATE_IO,
    Exchange.EXCHANGE_BITGET,
  ]) {
    const capabilities = tradingCapabilities({ exchange });
    assert.equal(capabilities.placeIoc, false);
    assert.equal(capabilities.queryByClientOrderId, exchange === Exchange.EXCHANGE_OKX);
    assert.equal(capabilities.cancelOrder, false);
    assert.equal(capabilities.privateOrderStream, false);
    assert.equal(capabilities.privateFillStream, false);
  }
});

test("native algo research never reports submit readiness", () => {
  const binance = algoCapabilities({ exchange: Exchange.EXCHANGE_BINANCE });
  assert.deepEqual(
    binance.algorithms?.map((capability) => capability.validationStatus),
    ["documented_not_validated", "documented_not_validated", "documented_not_validated"],
  );
  assert.deepEqual(
    binance.algorithms?.map((capability) => capability.algorithm),
    ["binance_spot_twap", "binance_usdm_twap", "binance_usdm_vp"],
  );
  assert.ok(binance.algorithms?.every((capability) => !capability.submit && !capability.protectedPrice));

  const bitget = algoCapabilities({ exchange: Exchange.EXCHANGE_BITGET });
  assert.equal(bitget.algorithms?.[0]?.validationStatus, "research_required");
});
