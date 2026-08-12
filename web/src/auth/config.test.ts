import { describe, expect, it } from "vitest";

import { resolveAuthConfig } from "./config";

describe("resolveAuthConfig", () => {
  it("keeps open mode when OIDC is not configured", () => {
    expect(resolveAuthConfig({}, {})).toEqual({ mode: "open" });
  });

  it("rejects partial runtime config instead of mixing it with Vite", () => {
    expect(() => resolveAuthConfig(
      { oidcIssuer: "https://runtime.example" },
      { VITE_BALLAST_OIDC_ISSUER: "https://build.example", VITE_BALLAST_OIDC_CLIENT_ID: "local-client" },
    )).toThrow("must be configured together");
  });

  it("maps the deployed runtime field names and gives them priority", () => {
    expect(resolveAuthConfig(
      { oidcIssuer: "https://runtime.example", oidcClientId: "runtime-client", oidcAudience: "runtime-api" },
      { VITE_BALLAST_OIDC_ISSUER: "https://vite.example", VITE_BALLAST_OIDC_CLIENT_ID: "vite-client", VITE_BALLAST_OIDC_AUDIENCE: "vite-api" },
    )).toEqual({ mode: "oidc", issuer: "https://runtime.example", clientId: "runtime-client", audience: "runtime-api" });
  });

  it("rejects a partial OIDC configuration", () => {
    expect(() => resolveAuthConfig({ oidcClientId: "ballast" }, {}))
      .toThrow("must be configured together");
  });

  it("rejects an audience without an OIDC client", () => {
    expect(() => resolveAuthConfig({ oidcAudience: "ballast-api" }, {}))
      .toThrow("requires OIDC issuer and client ID");
  });

  it.each(["http://id.example", "id.example", "/oidc", "not a url"])("rejects a non-HTTPS absolute issuer: %s", (issuer) => {
    expect(() => resolveAuthConfig({ oidcIssuer: issuer, oidcClientId: "ballast" }, {}))
      .toThrow("absolute HTTPS URL");
  });

  it("normalizes trailing issuer slashes like the server", () => {
    expect(resolveAuthConfig({ oidcIssuer: "https://id.example///", oidcClientId: "ballast" }, {}))
      .toEqual({ mode: "oidc", issuer: "https://id.example", clientId: "ballast" });
  });
});
