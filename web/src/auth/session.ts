export type AuthenticationListener = () => void;

let accessToken: string | undefined;
const authenticationRequiredListeners = new Set<AuthenticationListener>();

export function setAccessToken(token: string | undefined): void {
  accessToken = token;
}
export function getAccessToken(): string | undefined {
  return accessToken;
}

export function markAuthenticationRequired(): void {
  accessToken = undefined;
  authenticationRequiredListeners.forEach((listener) => listener());
}

export function onAuthenticationRequired(listener: AuthenticationListener): () => void {
  authenticationRequiredListeners.add(listener);
  return () => authenticationRequiredListeners.delete(listener);
}
