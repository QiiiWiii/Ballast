import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  login: vi.fn(async () => undefined),
  logout: vi.fn(async () => undefined),
  restore: vi.fn<() => Promise<never>>(),
  createOidcSessionManager: vi.fn(),
  onSessionExpired: vi.fn(() => vi.fn()),
}));

vi.mock("./manager", () => ({ createOidcSessionManager: mocks.createOidcSessionManager }));

import { AuthGate, AuthProvider } from "./AuthProvider";

describe("AuthProvider", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.restore.mockRejectedValue(new Error("callback_failed"));
    mocks.createOidcSessionManager.mockResolvedValue({ login: mocks.login, logout: mocks.logout, restore: mocks.restore, onSessionExpired: mocks.onSessionExpired });
  });

  it("keeps login available when session restore fails", async () => {
    render(<AuthProvider config={{ mode: "oidc", issuer: "https://id.example", clientId: "ballast" }}><AuthGate><div>secured</div></AuthGate></AuthProvider>);
    expect(await screen.findByText("callback_failed")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await waitFor(() => expect(mocks.login).toHaveBeenCalledOnce());
  });

  it("leaves loading and reports a login redirect failure", async () => {
    mocks.login.mockRejectedValueOnce(new Error("redirect_blocked"));
    render(<AuthProvider config={{ mode: "oidc", issuer: "https://id.example", clientId: "ballast" }}><AuthGate><div>secured</div></AuthGate></AuthProvider>);
    await screen.findByText("callback_failed");
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(await screen.findByText("redirect_blocked")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
  });
});
