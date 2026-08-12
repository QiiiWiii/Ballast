import { afterEach, describe, expect, it, vi } from "vitest";

import { request } from "./api";
import { setAccessToken } from "./auth/session";

describe("request", () => {
  afterEach(() => {
    setAccessToken(undefined);
    vi.restoreAllMocks();
  });

  it("injects the current Bearer token", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(JSON.stringify({ ok: true }), {
      status: 200,
      headers: { "content-type": "application/json" },
    }));
    setAccessToken("access-token");

    await request("/api/v1/tasks");

    const headers = new Headers(fetchMock.mock.calls[0][1]?.headers);
    expect(headers.get("authorization")).toBe("Bearer access-token");
  });
});
