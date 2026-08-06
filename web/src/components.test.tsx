import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import "./i18n";
import { StabilityLine, StatusPill } from "./components";
import { summarizeGateway } from "./pages";

describe("StabilityLine", () => {
  it("labels paper execution residual explicitly", () => {
    render(<StabilityLine residual={0.25} label="执行残余" />);
    expect(screen.getByLabelText("执行残余: 25.0%")).toBeInTheDocument();
  });
});

describe("StatusPill", () => {
  it("translates machine states in the active locale", () => {
    render(<StatusPill status="completed" />);
    expect(screen.getByText("已完成")).toBeInTheDocument();
  });
});

describe("summarizeGateway", () => {
  it("reports degraded when any registered adapter is degraded", () => {
    const exchanges = ["binance", "okx", "bybit", "gate_io", "bitget"].map((exchange, index) => ({
      exchange,
      status: index === 2 ? "degraded" : "ready",
    }));
    expect(summarizeGateway(exchanges as Parameters<typeof summarizeGateway>[0], false, false)).toEqual({
      readyCount: 4,
      status: "degraded",
    });
  });
});
