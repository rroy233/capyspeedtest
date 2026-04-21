import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { AppRoutes } from "./App";

vi.mock("./pages/HomePage", () => ({
  default: () => <h1>测速中心</h1>,
}));

vi.mock("./pages/SettingsPage", () => ({
  default: () => <h1>设置中心</h1>,
}));

vi.mock("./pages/ResultsPage", () => ({
  default: () => <h1>历史记录</h1>,
}));

vi.mock("./pages/AboutPage", () => ({
  default: () => <h1>关于</h1>,
}));

vi.mock("./pages/NotFoundPage", () => ({
  default: () => <h2>404</h2>,
}));

describe("AppRoutes", () => {
  it("访问 / 时展示首页", () => {
    render(
      <MemoryRouter initialEntries={["/"]}>
        <AppRoutes />
      </MemoryRouter>
    );

    expect(screen.getByRole("heading", { name: "测速中心" })).toBeInTheDocument();
  });

  it("访问未知路由时展示 404", () => {
    render(
      <MemoryRouter initialEntries={["/not-exists"]}>
        <AppRoutes />
      </MemoryRouter>
    );

    expect(screen.getByRole("heading", { name: "404" })).toBeInTheDocument();
  });

  it("访问 /settings 时展示设置中心", () => {
    render(
      <MemoryRouter initialEntries={["/settings"]}>
        <AppRoutes />
      </MemoryRouter>
    );

    expect(screen.getByRole("heading", { name: "设置中心" })).toBeInTheDocument();
  });

  it("访问 /results 时展示历史记录页", () => {
    render(
      <MemoryRouter initialEntries={["/results"]}>
        <AppRoutes />
      </MemoryRouter>
    );

    expect(screen.getByRole("heading", { name: "历史记录" })).toBeInTheDocument();
  });
});
