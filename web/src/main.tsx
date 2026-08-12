import "@fontsource/barlow-condensed/latin-500.css";
import "@fontsource/barlow-condensed/latin-600.css";
import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/ibm-plex-mono/latin-400.css";
import "./styles.css";
import "./i18n";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRootRoute, createRoute, createRouter, RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { AppShell } from "./components";
import {
  AccountsPage, AnalyticsPage, ApprovalsPage, ControlRoomPage, CreateExecutionPage,
  ExecutionDetailPage, ExecutionsPage, HedgingPage, RiskPage, RoadmapPage,
  StrategyDetailPage, SystemPage, VenueDetailPage, VenuesPage,
} from "./pages";
import { StrategiesPage } from "./researchLab";

const rootRoute = createRootRoute({ component: AppShell });
const controlRoomRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: ControlRoomPage });
const executionsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/executions", component: ExecutionsPage });
const createExecutionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/executions/new",
  validateSearch: (search: Record<string, unknown>) => ({ template: typeof search.template === "string" ? search.template : undefined }),
  component: CreateExecutionPage,
});
const executionDetailRoute = createRoute({ getParentRoute: () => rootRoute, path: "/executions/$executionId", component: ExecutionDetailPage });
const strategiesRoute = createRoute({ getParentRoute: () => rootRoute, path: "/strategies", component: StrategiesPage });
const strategyDetailRoute = createRoute({ getParentRoute: () => rootRoute, path: "/strategies/$strategyId", component: StrategyDetailPage });
const analyticsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/analytics", component: AnalyticsPage });
const venuesRoute = createRoute({ getParentRoute: () => rootRoute, path: "/venues", component: VenuesPage });
const venueDetailRoute = createRoute({ getParentRoute: () => rootRoute, path: "/venues/$exchange", component: VenueDetailPage });
const systemRoute = createRoute({ getParentRoute: () => rootRoute, path: "/system", component: SystemPage });
const roadmapRoute = createRoute({ getParentRoute: () => rootRoute, path: "/roadmap", component: RoadmapPage });
const approvalsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/approvals", component: ApprovalsPage });
const accountsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/accounts", component: AccountsPage });
const riskRoute = createRoute({ getParentRoute: () => rootRoute, path: "/risk", component: RiskPage });
const hedgingRoute = createRoute({ getParentRoute: () => rootRoute, path: "/hedging", component: HedgingPage });
const routeTree = rootRoute.addChildren([
  controlRoomRoute, executionsRoute, createExecutionRoute, executionDetailRoute, strategiesRoute,
  strategyDetailRoute, analyticsRoute, venuesRoute, venueDetailRoute, approvalsRoute, accountsRoute,
  riskRoute, hedgingRoute, systemRoute, roadmapRoute,
]);
const router = createRouter({ routeTree });
declare module "@tanstack/react-router" { interface Register { router: typeof router } }

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 3_000, retry: 1 } } });
const root = document.getElementById("root");
if (!root) throw new Error("root element is missing");
createRoot(root).render(<StrictMode><QueryClientProvider client={queryClient}><RouterProvider router={router} /></QueryClientProvider></StrictMode>);
