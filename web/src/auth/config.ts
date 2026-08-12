/// <reference types="vite/client" />

export interface RuntimeAuthConfig {
  oidcIssuer?: string;
  oidcClientId?: string;
  oidcAudience?: string;
}

export interface ViteAuthConfig {
  VITE_BALLAST_OIDC_ISSUER?: string;
  VITE_BALLAST_OIDC_CLIENT_ID?: string;
  VITE_BALLAST_OIDC_AUDIENCE?: string;
}

declare global {
  interface Window {
    __BALLAST_CONFIG__?: RuntimeAuthConfig;
  }
}

export type AuthConfig =
  | { mode: "open" }
  | { mode: "oidc"; issuer: string; clientId: string; audience?: string };

const normalized = (value: unknown): string | undefined =>
  typeof value === "string" && value.trim() !== "" ? value.trim() : undefined;

export function resolveAuthConfig(
  runtime: RuntimeAuthConfig | undefined = window.__BALLAST_CONFIG__,
  vite: ViteAuthConfig = {
    VITE_BALLAST_OIDC_ISSUER: import.meta.env.VITE_BALLAST_OIDC_ISSUER,
    VITE_BALLAST_OIDC_CLIENT_ID: import.meta.env.VITE_BALLAST_OIDC_CLIENT_ID,
    VITE_BALLAST_OIDC_AUDIENCE: import.meta.env.VITE_BALLAST_OIDC_AUDIENCE,
  },
): AuthConfig {
  const runtimeValues = {
    issuer: normalized(runtime?.oidcIssuer),
    clientId: normalized(runtime?.oidcClientId),
    audience: normalized(runtime?.oidcAudience),
  };
  const hasRuntimeConfig = Object.values(runtimeValues).some((value) => value !== undefined);
  const issuer = hasRuntimeConfig ? runtimeValues.issuer : normalized(vite.VITE_BALLAST_OIDC_ISSUER);
  const clientId = hasRuntimeConfig ? runtimeValues.clientId : normalized(vite.VITE_BALLAST_OIDC_CLIENT_ID);
  const audience = hasRuntimeConfig ? runtimeValues.audience : normalized(vite.VITE_BALLAST_OIDC_AUDIENCE);

  if (issuer === undefined && clientId === undefined) {
    if (audience !== undefined) throw new Error("VITE_BALLAST_OIDC_AUDIENCE requires OIDC issuer and client ID");
    return { mode: "open" };
  }
  if (issuer === undefined || clientId === undefined) {
    throw new Error("VITE_BALLAST_OIDC_ISSUER and VITE_BALLAST_OIDC_CLIENT_ID must be configured together");
  }
  const normalizedIssuer = issuer.replace(/\/+$/, "");
  let issuerUrl: URL;
  try {
    issuerUrl = new URL(normalizedIssuer);
  } catch {
    throw new Error("OIDC issuer must be an absolute HTTPS URL");
  }
  if (issuerUrl.protocol !== "https:" || issuerUrl.host === "") {
    throw new Error("OIDC issuer must be an absolute HTTPS URL");
  }
  return { mode: "oidc", issuer: normalizedIssuer, clientId, ...(audience === undefined ? {} : { audience }) };
}
