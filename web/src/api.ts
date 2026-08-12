import { z } from "zod";

const decimal = z.string().regex(/^-?\d+(\.\d+)?$/);
const timestamp = z.string().datetime({ offset: true });
export const exchangeSchema = z.enum(["binance", "okx", "bybit", "gate_io", "bitget"]);
export const marketKindSchema = z.enum(["spot", "perpetual"]);
export const executionBackendSchema = z.enum(["managed_ioc", "venue_native_algo"]);
const nativeConfigurationSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("binance_spot_twap"), limit_price: decimal }),
  z.object({
    kind: z.literal("binance_usdm_twap"), limit_price: decimal,
    position_side: z.string(), reduce_only: z.boolean(),
  }),
  z.object({
    kind: z.literal("binance_usdm_vp"), limit_price: decimal,
    urgency: z.enum(["LOW", "MEDIUM", "HIGH"]), position_side: z.string(), reduce_only: z.boolean(),
  }),
  z.object({
    kind: z.literal("okx_twap"), trade_mode: z.string(), position_side: z.string(),
    size_limit: decimal, price_limit: decimal, time_interval_seconds: z.number(),
    price_variance_ratio: decimal.nullable(), price_spread: decimal.nullable(), reduce_only: z.boolean(),
  }),
]);

const capabilitiesSchema = z.object({
  spot: z.boolean(), perpetual_linear: z.boolean(), perpetual_inverse: z.boolean(),
  fetch_order_book: z.boolean(), watch_order_book: z.boolean(), watch_trades: z.boolean(),
});

export const exchangeStatusSchema = z.object({
  exchange: exchangeSchema,
  status: z.string(),
  last_error_code: z.string().nullable(),
  last_success_at_ms: z.number().nullable(),
  health_query_latency_ms: z.number().nullable(),
  instrument_count: z.number(),
  active_subscriptions: z.number(),
  stale_subscriptions: z.number(),
  capabilities: capabilitiesSchema.nullable(),
});

export const instrumentSchema = z.object({
  id: z.string().uuid(), exchange: exchangeSchema, market_kind: marketKindSchema,
  symbol: z.string(), exchange_symbol: z.string(), base_asset: z.string(), quote_asset: z.string(),
  settle_asset: z.string().nullable(), contract_kind: z.enum(["linear", "inverse"]).nullable(),
  contract_size: decimal.nullable(), price_tick: decimal, quantity_step: decimal,
  minimum_quantity: decimal.nullable(), minimum_notional: decimal.nullable(),
  maker_fee_rate: decimal.nullable(), taker_fee_rate: decimal.nullable(), active: z.boolean(), observed_at: timestamp,
});

export const instrumentPageSchema = z.object({
  items: z.array(instrumentSchema), total: z.number(), limit: z.number(), offset: z.number(),
});

export const taskSchema = z.object({
  id: z.string().uuid(), instrument_id: z.string().uuid(), template_version_id: z.string().uuid(),
  side: z.enum(["buy", "sell"]), strategy: z.enum(["twap", "pov"]),
  strategy_params: z.record(z.string(), z.unknown()), requested_amount: decimal,
  quantity_unit: z.enum(["base_quantity", "quote_notional", "contracts"]),
  executed_amount: decimal, residual_amount: decimal, max_slippage_bps: z.number(), slice_interval_ms: z.number(),
  status: z.enum(["pending_approval", "scheduled", "running", "paused", "cancelling", "completed", "cancelled", "expired", "failed", "rejected"]),
  execution_mode: z.enum(["paper", "live"]), execution_backend: executionBackendSchema,
  account_id: z.string(), requested_by: z.string().nullable(), approved_by: z.string().nullable(),
  paused_reason: z.string().nullable(), failure_code: z.string().nullable(), started_at: timestamp.nullable(),
  deadline_at: timestamp, next_tick_at: timestamp, last_tick_at: timestamp.nullable(), version: z.number(),
  created_at: timestamp, updated_at: timestamp,
});

export const sliceSchema = z.object({
  id: z.string().uuid(), task_id: z.string().uuid(), sequence: z.number(), requested_amount: decimal,
  native_quantity: decimal, filled_native_quantity: decimal, filled_base_quantity: decimal,
  filled_quote_quantity: decimal, average_price: decimal.nullable(), worst_price: decimal.nullable(),
  slippage_bps: decimal.nullable(), fee_amount: decimal.nullable(), fee_asset: z.string().nullable(),
  fee_status: z.enum(["calculated", "unavailable"]), status: z.enum(["filled", "partial", "unfilled", "rejected"]),
  market_snapshot: z.record(z.string(), z.unknown()), decision_input: z.record(z.string(), z.unknown()), created_at: timestamp,
});

export const templateVersionSchema = z.object({
  id: z.string().uuid(), template_id: z.string().uuid(), version: z.number(), strategy: z.enum(["twap", "pov"]),
  quantity_unit: z.enum(["base_quantity", "quote_notional", "contracts"]), duration_seconds: z.number(),
  slice_interval_ms: z.number(), max_slippage_bps: z.number(), participation_rate: decimal.nullable(),
  max_slice_amount: decimal.nullable(), execution_backend: executionBackendSchema,
  venue_exchange: exchangeSchema.nullable(), venue_market_kind: marketKindSchema.nullable(),
  native_algorithm: z.enum(["binance_spot_twap", "binance_usdm_twap", "binance_usdm_vp", "okx_twap"]).nullable(),
  native_configuration: nativeConfigurationSchema.nullable(),
  change_note: z.string(), created_at: timestamp,
});

export const templateSchema = z.object({
  id: z.string().uuid(), name: z.string(), description: z.string(), status: z.enum(["active", "archived"]),
  current_version: z.number(), usage_count: z.number(), versions: z.array(templateVersionSchema),
  created_at: timestamp, updated_at: timestamp,
});

export const analyticsSchema = z.object({
  window: z.string(), task_count: z.number(), active_count: z.number(), completed_count: z.number(), exception_count: z.number(),
  average_completion_ratio: decimal, median_slippage_bps: decimal.nullable(), p95_slippage_bps: decimal.nullable(),
  fee_unavailable_ratio: decimal, median_runtime_ms: z.number().nullable(), status_counts: z.record(z.string(), z.number()),
  pause_reason_counts: z.record(z.string(), z.number()), strategy_counts: z.record(z.string(), z.number()),
  exchange_counts: z.record(z.string(), z.number()),
});

export const dashboardSchema = z.object({
  mode: z.literal("paper"), generated_at: timestamp, analytics: analyticsSchema,
  urgent_tasks: z.array(taskSchema), exchanges: z.array(exchangeStatusSchema),
});

export const healthEventSchema = z.object({
  sequence: z.number(), status: z.string(), error_code: z.string().nullable(), health_query_latency_ms: z.number().nullable(), observed_at: timestamp,
});

export const subscriptionSchema = z.object({
  instrument_id: z.string().uuid(), stream_kind: z.enum(["order_book", "trades"]), status: z.string(),
  reconnect_attempt: z.number(), error_code: z.string().nullable(), last_event_at: timestamp.nullable(), observed_at: timestamp,
});
export const eventSchema = z.object({
  sequence: z.number(), event_id: z.string().uuid(), task_id: z.string().uuid().nullable(), event_type: z.string(),
  payload: z.record(z.string(), z.unknown()), created_at: timestamp,
});

export type ExchangeStatus = z.infer<typeof exchangeStatusSchema>;
export type Instrument = z.infer<typeof instrumentSchema>;
export type InstrumentPage = z.infer<typeof instrumentPageSchema>;
export type ExecutionTask = z.infer<typeof taskSchema>;
export type ExecutionSlice = z.infer<typeof sliceSchema>;
export type StrategyTemplate = z.infer<typeof templateSchema>;
export type StrategyTemplateVersion = z.infer<typeof templateVersionSchema>;
export type Analytics = z.infer<typeof analyticsSchema>;








export interface TemplateInput {
  name?: string; description?: string; strategy: "twap" | "pov";
  quantity_unit: "base_quantity" | "quote_notional" | "contracts";
  duration_seconds: number; slice_interval_ms: number; max_slippage_bps: number;
  participation_rate?: string; max_slice_amount?: string; change_note?: string;
  execution_backend: "managed_ioc" | "venue_native_algo";
}

export interface CreateTaskInput {
  instrument_id: string; side: "buy" | "sell"; target_amount: string; template_version_id: string;
}

export interface InstrumentQuery {
  exchange?: z.infer<typeof exchangeSchema>;
  market_kind?: z.infer<typeof marketKindSchema>;
  active_only?: boolean;
  search?: string;
  ids?: string[];
  limit?: number;
  offset?: number;
}

async function request(path: string, init?: RequestInit): Promise<unknown> {
  const response = await fetch(path, init);
  const body = await response.json().catch(() => null) as unknown;
  if (!response.ok) {
    const parsed = z.object({ error: z.object({ code: z.string(), params: z.record(z.string(), z.unknown()).optional() }) }).safeParse(body);
    throw new Error(parsed.success ? parsed.data.error.code : `http_${response.status}`);
  }
  return body;
}

const jsonRequest = (method: "POST" | "PATCH", body?: unknown): RequestInit => ({
  method, headers: { "content-type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body),
});

export const api = {
  exchanges: async () => z.array(exchangeStatusSchema).parse(await request("/api/v1/exchanges")),
  exchangeSnapshots: async () => z.array(exchangeStatusSchema).parse(await request("/api/v1/exchanges/snapshots")),
  exchange: async (exchange: string) => exchangeStatusSchema.parse(await request(`/api/v1/exchanges/${exchange}`)),
  healthEvents: async (exchange: string) => z.array(healthEventSchema).parse(await request(`/api/v1/exchanges/${exchange}/health-events`)),
  subscriptions: async (exchange: string) => z.array(subscriptionSchema).parse(await request(`/api/v1/exchanges/${exchange}/subscriptions`)),
  instruments: async (query: InstrumentQuery = {}) => {
    const params = new URLSearchParams();
    if (query.exchange) params.set("exchange", query.exchange);
    if (query.market_kind) params.set("market_kind", query.market_kind);
    if (query.active_only ?? true) params.set("active_only", "true");
    if (query.search) params.set("search", query.search);
    if (query.ids?.length) params.set("ids", query.ids.join(","));
    params.set("limit", String(query.limit ?? 100));
    params.set("offset", String(query.offset ?? 0));
    return instrumentPageSchema.parse(await request(`/api/v1/instruments?${params}`));
  },
  syncInstruments: async () => { await request("/api/v1/instruments/sync", jsonRequest("POST", { reload: true })); },
  tasks: async () => z.array(taskSchema).parse(await request("/api/v1/tasks?limit=500")),
  task: async (id: string) => taskSchema.parse(await request(`/api/v1/tasks/${id}`)),
  slices: async (id: string) => z.array(sliceSchema).parse(await request(`/api/v1/tasks/${id}/slices`)),
  events: async (taskId: string) => z.array(eventSchema).parse(await request(`/api/v1/events?task_id=${encodeURIComponent(taskId)}&limit=1000`)),
  createTask: async (input: CreateTaskInput) => taskSchema.parse(await request("/api/v1/tasks", {
    ...jsonRequest("POST", input), headers: { "content-type": "application/json", "idempotency-key": crypto.randomUUID() },
  })),
  cancelTask: async (id: string) => taskSchema.parse(await request(`/api/v1/tasks/${id}/cancel`, jsonRequest("POST"))),
  templates: async () => z.array(templateSchema).parse(await request("/api/v1/strategy-templates")),
  template: async (id: string) => templateSchema.parse(await request(`/api/v1/strategy-templates/${id}`)),
  createTemplate: async (input: TemplateInput) => templateSchema.parse(await request("/api/v1/strategy-templates", jsonRequest("POST", input))),
  createTemplateVersion: async (id: string, input: TemplateInput) => templateVersionSchema.parse(await request(`/api/v1/strategy-templates/${id}/versions`, jsonRequest("POST", input))),
  archiveTemplate: async (id: string) => templateSchema.parse(await request(`/api/v1/strategy-templates/${id}/archive`, jsonRequest("POST"))),
  analytics: async (query = "window=24h") => analyticsSchema.parse(await request(`/api/v1/analytics/executions?${query}`)),
  dashboard: async () => dashboardSchema.parse(await request("/api/v1/dashboard/operations?window=24h")),
  health: async () => z.object({ status: z.string(), service: z.string(), version: z.string() }).parse(await request("/health")),
};
