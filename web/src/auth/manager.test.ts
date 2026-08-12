import { beforeEach, describe, expect, it, vi } from "vitest";

const removeUser = vi.fn(async () => undefined);
const clearStaleState = vi.fn(async () => undefined);
const signoutRedirect = vi.fn(async () => undefined);
const getUser = vi.fn(async () => ({ id_token: "identity-token" }));
const signinRedirect = vi.fn(async () => undefined);
const signinRedirectCallback = vi.fn(async () => ({ state: { returnUrl: "https://ballast.example/executions/1?tab=events" } }));
const userManagerSettings: unknown[] = [];
const addAccessTokenExpired = vi.fn();
const removeAccessTokenExpired = vi.fn();

vi.mock("oidc-client-ts", () => ({
  UserManager: class {
    constructor(settings: unknown) { userManagerSettings.push(settings); }
    getUser = getUser;
    signinRedirect = signinRedirect;
    signinRedirectCallback = signinRedirectCallback;
    removeUser = removeUser;
    clearStaleState = clearStaleState;
    signoutRedirect = signoutRedirect;
    events = { addAccessTokenExpired, removeAccessTokenExpired };
  },
  WebStorageStateStore: class {},
}));

import { createOidcSessionManager, safeReturnUrl } from "./manager";

describe("OidcSessionManager", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    userManagerSettings.length = 0;
  });

  it("uses one fixed callback URI and keeps the business URL in login state", async () => {
    const manager = await createOidcSessionManager({ mode: "oidc", issuer: "https://id.example", clientId: "ballast" });
    expect(userManagerSettings[0]).toMatchObject({ redirect_uri: `${window.location.origin}/auth/callback` });
    expect(userManagerSettings[0]).toMatchObject({ automaticSilentRenew: false });
    await manager.login();
    expect(signinRedirect).toHaveBeenCalledWith({ state: { returnUrl: window.location.href } });
  });

  it("exposes token expiry without hidden silent renewal", async () => {
    const manager = await createOidcSessionManager({ mode: "oidc", issuer: "https://id.example", clientId: "ballast" });
    const listener = vi.fn();
    const unsubscribe = manager.onSessionExpired(listener);
    expect(addAccessTokenExpired).toHaveBeenCalledWith(listener);
    unsubscribe();
    expect(removeAccessTokenExpired).toHaveBeenCalledWith(listener);
  });

  it("allows only same-origin callback return URLs", () => {
    expect(safeReturnUrl("https://ballast.example/executions/1?tab=events", "https://ballast.example"))
      .toBe("https://ballast.example/executions/1?tab=events");
    expect(safeReturnUrl("https://attacker.example/steal", "https://ballast.example"))
      .toBe("https://ballast.example/");
    expect(safeReturnUrl(undefined, "https://ballast.example"))
      .toBe("https://ballast.example/");
  });

  it("clears the OIDC browser session before redirecting logout", async () => {
    const manager = await createOidcSessionManager({ mode: "oidc", issuer: "https://id.example", clientId: "ballast" });
    await manager.logout();
    expect(removeUser).toHaveBeenCalledOnce();
    expect(clearStaleState).toHaveBeenCalledOnce();
    expect(signoutRedirect).toHaveBeenCalledWith({ id_token_hint: "identity-token" });
    expect(removeUser.mock.invocationCallOrder[0]).toBeLessThan(signoutRedirect.mock.invocationCallOrder[0]);
  });
});
