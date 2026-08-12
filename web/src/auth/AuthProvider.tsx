import type { User } from "oidc-client-ts";
import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";

import type { AuthConfig } from "./config";
import { createOidcSessionManager, type OidcSessionManager } from "./manager";
import { onAuthenticationRequired, setAccessToken } from "./session";

type AuthState =
  | { status: "open" }
  | { status: "loading" }
  | { status: "anonymous"; error?: string }
  | { status: "authenticated"; user: User };

type AuthContextValue = AuthState & {
  login(): Promise<void>;
  logout(): Promise<void>;
};

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function AuthProvider({ config, children }: { config: AuthConfig; children: ReactNode }) {
  const [manager, setManager] = useState<OidcSessionManager>();
  const [state, setState] = useState<AuthState>(config.mode === "open" ? { status: "open" } : { status: "loading" });
  const startup = useRef<Promise<OidcSessionManager> | undefined>(undefined);
  const restore = useRef<Promise<User | null> | undefined>(undefined);

  useEffect(() => {
    if (config.mode === "open") return;
    let active = true;
    startup.current ??= createOidcSessionManager(config);
    void startup.current.then(async (nextManager) => {
      if (!active) return;
      setManager(nextManager);
      restore.current ??= nextManager.restore();
      const user = await restore.current;
      if (!active) return;
      setAccessToken(user?.access_token);
      setState(user ? { status: "authenticated", user } : { status: "anonymous" });
    }).catch((error: unknown) => {
      if (!active) return;
      setAccessToken(undefined);
      setState({ status: "anonymous", error: error instanceof Error ? error.message : "oidc_restore_failed" });
    });
    return () => { active = false; setAccessToken(undefined); };
  }, [config]);

  useEffect(() => {
    if (config.mode === "open") return;
    return onAuthenticationRequired(() => setState({ status: "anonymous", error: "authentication_required" }));
  }, [config.mode]);

  useEffect(() => {
    if (!manager) return;
    return manager.onSessionExpired(() => {
      setAccessToken(undefined);
      setState({ status: "anonymous", error: "authentication_required" });
    });
  }, [manager]);

  const login = useCallback(async () => {
    if (!manager) {
      setState({ status: "anonymous", error: "oidc_manager_unavailable" });
      return;
    }
    setState({ status: "loading" });
    try {
      await manager.login();
    } catch (error) {
      setState({ status: "anonymous", error: error instanceof Error ? error.message : "oidc_login_failed" });
    }
  }, [manager]);
  const logout = useCallback(async () => {
    if (!manager) return;
    setAccessToken(undefined);
    setState({ status: "anonymous" });
    await manager.logout();
  }, [manager]);

  return <AuthContext.Provider value={{ ...state, login, logout }}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const value = useContext(AuthContext);
  if (!value) throw new Error("AuthProvider is missing");
  return value;
}

export function AuthGate({ children }: { children: ReactNode }) {
  const auth = useAuth();
  if (auth.status === "open" || auth.status === "authenticated") return children;
  if (auth.status === "loading") return <main className="auth-screen" aria-live="polite"><section><span>OIDC / SESSION</span><h1>Restoring secure session</h1><p>Validating the browser session with the configured identity provider.</p></section></main>;
  return <main className="auth-screen"><section><span>OIDC / ACCESS CONTROL</span><h1>Authentication required</h1><p>{auth.error === "authentication_required" ? "Your API session is no longer valid. Sign in again to continue." : "Sign in through the configured identity provider to open Ballast."}</p><button className="primary-button" onClick={() => void auth.login()}>Sign in</button>{auth.error && auth.error !== "authentication_required" ? <code>{auth.error}</code> : null}</section></main>;
}
