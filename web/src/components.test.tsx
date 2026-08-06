import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import "./i18n";
import { StabilityLine } from "./components";

describe("StabilityLine", () => {
  it("labels paper execution residual explicitly", () => {
    render(<StabilityLine residual={0.25} label="执行残余" />);
    expect(screen.getByLabelText("执行残余: 25.0%")).toBeInTheDocument();
  });
});
