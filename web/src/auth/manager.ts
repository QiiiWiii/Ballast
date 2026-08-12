import type { User, UserManager } from "oidc-client-ts";

import type { AuthConfig } from "./config";

export interface OidcSessionManager {
  restore(): Promise<User | null>;
  login(): Promise<void>;
  logout(): Promise<void>;
  onSessionExpired(listener: () => void): () => void;
}

function isAuthorizationResponse(search: string): boolean {
  const params = new URLSearchParams(search);
  return params.has("state") && (params.has("code") || params.has("error"));
}

export function safeReturnUrl(value: unknown, origin: string): string {
  const fallback = `${origin}/`;
  if (typeof value !== "string") return fallback;
  try {
    const url = new URL(value, origin);
    return url.origin === origin ? url.href : fallback;
  } catch {
    return fallback;
  }
}

export async function createOidcSessionManager(config: Extract<AuthConfig, { mode: "oidc" }>): Promise<OidcSessionManager> {
  const { UserManager, WebStorageStateStore } = await import("oidc-client-ts");
  const redirectUri = `${window.location.origin}/auth/callback`;
  const store = new WebStorageStateStore({ store: window.sessionStorage });
  const manager: UserManager = new UserManager({
    authority: config.issuer,
    client_id: config.clientId,
    redirect_uri: redirectUri,
    post_logout_redirect_uri: `${window.location.origin}/`,
    response_type: "code",
    scope: "openid profile",
    automaticSilentRenew: false,
    stateStore: store,
    userStore: store,
    ...(config.audience === undefined ? {} : { extraQueryParams: { audience: config.audience } }),
  });

  return {
    async restore() {
      if (isAuthorizationResponse(window.location.search)) {
        const user = await manager.signinRedirectCallback();
        const state = user.state as { returnUrl?: unknown } | undefined;
        window.location.replace(safeReturnUrl(state?.returnUrl, window.location.origin));
        return null;
      }
      const user = await manager.getUser();
      return user?.expired ? null : user;
    },
    async login() {
      await manager.signinRedirect({ state: { returnUrl: window.location.href } });
    },
    async logout() {
      const user = await manager.getUser();
      await manager.removeUser();
      await manager.clearStaleState();
      await manager.signoutRedirect({ id_token_hint: user?.id_token });
    },
    onSessionExpired(listener) {
      manager.events.addAccessTokenExpired(listener);
      return () => manager.events.removeAccessTokenExpired(listener);
    },
  };
}
