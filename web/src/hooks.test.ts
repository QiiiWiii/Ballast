import { afterEach, describe, expect, it, vi } from "vitest";

import { api } from "./api";
import { createEventSocket } from "./hooks";

describe("createEventSocket", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("keeps open-mode WebSockets ticket-free", async () => {
    const socket = vi.fn();
    vi.stubGlobal("WebSocket", socket);
    await createEventSocket({ mode: "open" }, "ws://localhost/api/v1/ws");
    expect(socket).toHaveBeenCalledWith("ws://localhost/api/v1/ws");
  });

  it("gets a new single-use ticket for every connection attempt", async () => {
    vi.spyOn(api, "wsTicket")
      .mockResolvedValueOnce({ ticket: "first", expires_at: "2026-08-12T00:00:00Z" })
      .mockResolvedValueOnce({ ticket: "second", expires_at: "2026-08-12T00:01:00Z" });
    const socket = vi.fn();
    vi.stubGlobal("WebSocket", socket);
    const config = { mode: "oidc", issuer: "https://id.example", clientId: "ballast" } as const;

    await createEventSocket(config, "wss://ballast.example/api/v1/ws?after_sequence=7");
    await createEventSocket(config, "wss://ballast.example/api/v1/ws?after_sequence=7");

    expect(api.wsTicket).toHaveBeenCalledTimes(2);
    expect(socket).toHaveBeenNthCalledWith(1, "wss://ballast.example/api/v1/ws?after_sequence=7", ["ballast-ticket", "ballast-ticket-value.first"]);
    expect(socket).toHaveBeenNthCalledWith(2, "wss://ballast.example/api/v1/ws?after_sequence=7", ["ballast-ticket", "ballast-ticket-value.second"]);
  });
});
