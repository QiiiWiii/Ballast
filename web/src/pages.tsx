import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { FormEvent, KeyboardEvent, useDeferredValue, useEffect, useId, useMemo, useRef, useState } from "react";
import { Area, AreaChart, Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useTranslation } from "react-i18next";

import { api, type ExchangeStatus, type ExecutionTask, type Instrument, type StrategyTemplate, type TemplateInput } from "./api";
import { EmptyState, ErrorPanel, Metric, PageHeader, PanelTitle, RelativeTime, StabilityLine, statusText, StatusPill, TaskProgress } from "./components";
import { useEventStreamStatus } from "./hooks";

const activeStatuses = new Set(["scheduled", "running", "paused", "cancelling"]);
const currentVersion = (template: StrategyTemplate) => template.versions.find((version) => version.version === template.current_version) ?? template.versions[0];
const displayExchange = (exchange: string) => exchange.replace("_", ".").toUpperCase();
const displayLatency = (latency: number|null) => latency === null ? "—" : `${latency}ms`;

const venueOptions: Instrument["exchange"][] = ["binance", "okx", "bybit", "gate_io", "bitget"];

function InstrumentPicker({ value, onChange }: { value?: Instrument; onChange: (instrument?: Instrument) => void }) {
  const { t } = useTranslation();
  const listId = useId();
  const root = useRef<HTMLDivElement>(null);
  const [exchange, setExchange] = useState<Instrument["exchange"]>(value?.exchange ?? "binance");
  const [market, setMarket] = useState<"spot" | "perpetual">(value?.market_kind ?? "spot");
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const [open, setOpen] = useState(false);
  const [highlighted, setHighlighted] = useState(0);
  useEffect(() => {
    if (!open) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [open]);
  const matches = useQuery({
    queryKey: ["instrument-search", exchange, market, deferredQuery],
    queryFn: () => api.instruments({ exchange, market_kind: market, search: deferredQuery, limit: 60 }),
  });
  const visibleMatches = matches.data?.items ?? [];
  const choose = (instrument: Instrument) => {
    onChange(instrument);
    setQuery(instrument.symbol);
    setOpen(false);
  };
  const changeScope = (nextExchange: Instrument["exchange"], nextMarket: "spot" | "perpetual") => {
    setExchange(nextExchange);
    setMarket(nextMarket);
    setQuery("");
    setHighlighted(0);
    setOpen(true);
    onChange(undefined);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") { setOpen(false); return; }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setOpen(true);
      const direction = event.key === "ArrowDown" ? 1 : -1;
      setHighlighted((current) => Math.max(0, Math.min(visibleMatches.length - 1, current + direction)));
    }
    if (event.key === "Enter" && open && visibleMatches[highlighted]) {
      event.preventDefault();
      choose(visibleMatches[highlighted]);
    }
  };
  return <div className="instrument-picker" ref={root} onBlur={(event) => {
    if (!root.current?.contains(event.relatedTarget as Node | null)) setOpen(false);
  }}>
    <div className="form-row instrument-scope">
      <label>{t("venue")}<select value={exchange} onChange={(event) => changeScope(event.target.value as Instrument["exchange"], market)}>{venueOptions.map((item) => <option key={item} value={item}>{displayExchange(item)}</option>)}</select></label>
      <label>{t("marketKind")}<select value={market} onChange={(event) => changeScope(exchange, event.target.value as "spot" | "perpetual")}><option value="spot">{t("spot")}</option><option value="perpetual">{t("perpetual")}</option></select></label>
    </div>
    <label>{t("instrument")}<div className="instrument-combobox">
      <input role="combobox" aria-required="true" aria-expanded={open} aria-controls={listId} aria-autocomplete="list" aria-activedescendant={open && visibleMatches[highlighted] ? `${listId}-${visibleMatches[highlighted].id}` : undefined} placeholder={t("searchInstrument")} value={query} onFocus={() => setOpen(true)} onClick={() => setOpen(true)} onChange={(event) => { setQuery(event.target.value); setHighlighted(0); setOpen(true); onChange(undefined); }} onKeyDown={onKeyDown}/>
      {open && <div className="instrument-results" id={listId} role="listbox">
        <header><span>{t("matchingInstruments", { count: matches.data?.total ?? 0 })}</span>{(matches.data?.total ?? 0) > visibleMatches.length && <b>{t("refineInstrumentSearch")}</b>}</header>
        {visibleMatches.map((item, index) => <button type="button" tabIndex={-1} id={`${listId}-${item.id}`} role="option" aria-selected={item.id === value?.id} className={index === highlighted ? "is-highlighted" : ""} key={item.id} onMouseDown={(event) => event.preventDefault()} onClick={() => choose(item)}><span><b>{item.symbol}</b><small>{item.exchange_symbol}</small></span><em>{item.contract_kind ?? item.market_kind}</em></button>)}
        {visibleMatches.length === 0 && <p>{t("noInstrumentMatches")}</p>}
      </div>}
    </div></label>
    {value && <div className="instrument-selection"><span>{t("selectedInstrument")}</span><b>{displayExchange(value.exchange)} · {value.symbol}</b><code>{value.quantity_step} step</code></div>}
  </div>;
}

export function ControlRoomPage() {
  const { t } = useTranslation();
  const dashboard = useQuery({ queryKey: ["dashboard"], queryFn: api.dashboard, refetchInterval: 10_000 });
  const data = dashboard.data;
  const urgentInstrumentIds = data?.urgent_tasks.map((task) => task.instrument_id) ?? [];
  const instruments = useQuery({
    queryKey: ["instruments-by-id", urgentInstrumentIds.join(",")],
    queryFn: () => api.instruments({ ids: urgentInstrumentIds, limit: Math.max(urgentInstrumentIds.length, 1) }),
    enabled: urgentInstrumentIds.length > 0,
  });
  const analytics = data?.analytics;
  const residual = analytics ? Math.max(0, 1 - Number(analytics.average_completion_ratio)) : 0;
  return <>
    <PageHeader eyebrow="OPERATIONS / COMMAND DECK" title={t("controlRoom")} description={t("controlRoomDescription")} action={<Link className="primary-button" to="/executions/new" search={{template:undefined}}>＋ {t("createExecution")}</Link>} />
    <div className="mode-banner"><b>PAPER</b><span>{t("paperBoundary")}</span><time>{data ? `SNAPSHOT ${new Date(data.generated_at).toLocaleTimeString()}` : "AWAITING SNAPSHOT"}</time></div>
    {dashboard.error && <ErrorPanel error={dashboard.error} />}
    <StabilityLine residual={residual} label={t("executionResidual")} />
    <section className="metric-strip">
      <Metric label={t("activeTasks")} value={String(analytics?.active_count ?? 0).padStart(2, "0")} note="OPEN EXECUTION OBJECTS" />
      <Metric label={t("exceptions")} value={String(analytics?.exception_count ?? 0).padStart(2, "0")} note="REQUIRES OPERATOR REVIEW" tone={(analytics?.exception_count ?? 0) > 0 ? "alarm" : "default"} />
      <Metric label={t("completion")} value={`${((Number(analytics?.average_completion_ratio ?? "0")) * 100).toFixed(1)}%`} note={t("taskAverage")} />
      <Metric label={t("medianSlippage")} value={analytics?.median_slippage_bps ? `${analytics.median_slippage_bps} bps` : "—"} note="P95 / execution analysis" />
    </section>
    <section className="dashboard-grid operations-dashboard">
      <article className="panel task-panel"><PanelTitle title={t("needsAttention")} tag="PRIORITY / ACTION QUEUE" />
        {data?.urgent_tasks.length ? <TaskTable tasks={data.urgent_tasks} instruments={new Map(instruments.data?.items.map((item) => [item.id, item]))} compact /> : <EmptyState text={t("noUrgentTasks")} />}
      </article>
      <article className="panel"><PanelTitle title={t("venueHealth")} tag="5 VENUES / PUBLIC DATA"/><div className="health-list">
        {data?.exchanges.map((exchange) => <Link to="/venues/$exchange" params={{ exchange: exchange.exchange }} key={exchange.exchange}>
          <b>{displayExchange(exchange.exchange)}</b><span>{exchange.instrument_count} {t("instruments")} · {t("healthQueryLatency")} {displayLatency(exchange.health_query_latency_ms)}</span><StatusPill status={exchange.status}/>
        </Link>)}
      </div></article>
      <article className="panel"><PanelTitle title={t("pauseReasons")} tag="24H"/><Breakdown data={analytics?.pause_reason_counts ?? {}} empty={t("noPauseReasons")} /></article>
      <article className="panel"><PanelTitle title={t("safetyBoundary")} tag="PAPER ONLY"/><ul className="boundary-list"><li>{t("noPrivateAccounts")}</li><li>{t("noLiveOrders")}</li><li>{t("residualIsNotExposure")}</li></ul></article>
    </section>
  </>;
}

export function ExecutionsPage() {
  const { t } = useTranslation(); const [status, setStatus] = useState("active"); const [exchange, setExchange] = useState("all"); const [strategy, setStrategy] = useState("all");
  const tasks = useQuery({ queryKey: ["tasks"], queryFn: api.tasks, refetchInterval: 5_000 });
  const taskInstrumentIds = [...new Set((tasks.data ?? []).map((task) => task.instrument_id))];
  const instruments = useQuery({
    queryKey: ["instruments-by-id", taskInstrumentIds.join(",")],
    queryFn: async () => {
      const chunks = Array.from({ length: Math.ceil(taskInstrumentIds.length / 100) }, (_, index) => taskInstrumentIds.slice(index * 100, index * 100 + 100));
      const pages = await Promise.all(chunks.map((ids) => api.instruments({ ids, limit: ids.length })));
      return pages.flatMap((page) => page.items);
    },
    enabled: taskInstrumentIds.length > 0,
  });
  const instrumentMap = new Map(instruments.data?.map((item) => [item.id, item]));
  const filtered = (tasks.data ?? []).filter((task) => {
    const instrument = instrumentMap.get(task.instrument_id);
    return (status === "all" || (status === "active" ? activeStatuses.has(task.status) : task.status === status))
      && (exchange === "all" || instrument?.exchange === exchange) && (strategy === "all" || task.strategy === strategy);
  });
  const allTasks = tasks.data ?? [];
  const counts = {
    active: allTasks.filter((task) => activeStatuses.has(task.status)).length,
    paused: allTasks.filter((task) => task.status === "paused").length,
    completed: allTasks.filter((task) => task.status === "completed").length,
    failed: allTasks.filter((task) => task.status === "failed" || task.status === "expired").length,
  };
  return <><PageHeader eyebrow="EXECUTION / LIVE BLOTTER" title={t("executions")} description={t("executionsDescription")} action={<Link className="primary-button" to="/executions/new" search={{template:undefined}}>＋ {t("createExecution")}</Link>} />
    {tasks.error && <ErrorPanel error={tasks.error} />}
    <section className="execution-ledger" aria-label="Execution status summary">
      <div><span>ACTIVE</span><b>{String(counts.active).padStart(2,"0")}</b></div>
      <div><span>PAUSED</span><b className={counts.paused ? "copper" : ""}>{String(counts.paused).padStart(2,"0")}</b></div>
      <div><span>COMPLETED</span><b>{String(counts.completed).padStart(2,"0")}</b></div>
      <div><span>FAILED / EXPIRED</span><b className={counts.failed ? "alarm-text" : ""}>{String(counts.failed).padStart(2,"0")}</b></div>
    </section>
    <section className="filter-bar" aria-label={t("filters")}><select value={status} onChange={(event) => setStatus(event.target.value)}><option value="active">{t("activeOnly")}</option><option value="all">{t("allStatuses")}</option><option value="paused">{statusText("paused", t)}</option><option value="completed">{statusText("completed", t)}</option><option value="failed">{statusText("failed", t)}</option></select><select value={exchange} onChange={(event) => setExchange(event.target.value)}><option value="all">{t("allVenues")}</option>{["binance","okx","bybit","gate_io","bitget"].map((item) => <option key={item} value={item}>{displayExchange(item)}</option>)}</select><select value={strategy} onChange={(event) => setStrategy(event.target.value)}><option value="all">{t("allStrategies")}</option><option value="twap">TWAP</option><option value="pov">POV</option></select><span>{filtered.length} {t("tasks")}</span></section>
    <section className="panel operations-panel"><TaskTable tasks={filtered} instruments={instrumentMap} /></section></>;
}

function TaskTable({ tasks, instruments = new Map(), compact = false }: { tasks: ExecutionTask[]; instruments?: Map<string, Instrument>; compact?: boolean }) {
  const { t } = useTranslation(); if (!tasks.length) return <EmptyState text={t("noTasks")} />;
  return <div className="table-scroll"><table className="data-table execution-table"><thead><tr><th>ID / STATE</th><th>{t("venueInstrument")}</th><th>SIDE / ALGO</th>{!compact && <th>TARGET / EXECUTED</th>}<th>{t("progress")}</th>{!compact && <><th>{t("residual")}</th><th>{t("nextSlice")}</th></>}<th/></tr></thead><tbody>{tasks.map((task) => {
    const instrument = instruments.get(task.instrument_id);
    return <tr key={task.id}>
      <td><StatusPill status={task.status}/><small>{task.id.slice(0,8).toUpperCase()}</small>{task.paused_reason && <small className="alarm-text">{task.paused_reason}</small>}</td>
      <td><b>{instrument?.symbol ?? task.instrument_id.slice(0,8)}</b><small>{instrument ? `${displayExchange(instrument.exchange)} / ${instrument.market_kind}` : "VENUE PENDING"}</small></td>
      <td><b className={task.side === "sell" ? "copper" : ""}>{task.side.toUpperCase()}</b><small>{task.strategy.toUpperCase()} / {task.execution_backend === "managed_ioc" ? "MANAGED" : "NATIVE"}</small></td>
      {!compact && <td className="mono"><b>{task.requested_amount}</b><small>{task.executed_amount} / {task.quantity_unit}</small></td>}
      <td><TaskProgress task={task}/></td>
      {!compact && <><td className="mono copper">{task.residual_amount}</td><td className="mono"><RelativeTime value={task.next_tick_at}/></td></>}
      <td><Link className="evidence-link" to="/executions/$executionId" params={{ executionId: task.id }}>{t("inspect")} →</Link></td>
    </tr>;
  })}</tbody></table></div>;
}

export function CreateExecutionPage() {
  const { t } = useTranslation(); const navigate = useNavigate(); const queryClient = useQueryClient();
  const search = useSearch({ from: "/executions/new" });
  const templates = useQuery({ queryKey: ["templates"], queryFn: api.templates });
  const activeTemplates = templates.data?.filter((template) => template.status === "active" && currentVersion(template)?.execution_backend !== "venue_native_algo") ?? [];
  const [templateId, setTemplateId] = useState(search.template ?? ""); const [instrument, setInstrument] = useState<Instrument>(); const [side, setSide] = useState<"buy"|"sell">("sell"); const [amount, setAmount] = useState("1");
  const template = activeTemplates.find((item) => item.id === templateId) ?? activeTemplates[0]; const version = template ? currentVersion(template) : undefined;
  const mutation = useMutation({ mutationFn: api.createTask, onSuccess: async (task) => { await queryClient.invalidateQueries({queryKey:["tasks"]}); await navigate({to:"/executions/$executionId",params:{executionId:task.id}}); } });
  const submit = (event: FormEvent) => { event.preventDefault(); if (!version || !instrument) return; mutation.mutate({ template_version_id: version.id, instrument_id: instrument.id, side, target_amount: amount }); };
  return <><PageHeader eyebrow="EXECUTION / NEW" title={t("createExecution")} description={t("createExecutionDescription")} action={<Link className="quiet-button" to="/executions">← {t("back")}</Link>} />
    <form className="execution-create-grid" onSubmit={submit}><section className="panel form-panel"><PanelTitle title={t("executionObject")} tag="01"/><label>{t("strategyTemplate")}<select required value={template?.id ?? ""} onChange={(event) => setTemplateId(event.target.value)}><option value="" disabled>{t("selectTemplate")}</option>{activeTemplates.map((item) => <option key={item.id} value={item.id}>{item.name} · v{item.current_version}</option>)}</select></label><InstrumentPicker value={instrument} onChange={setInstrument}/><div className="form-row"><label>{t("side")}<select value={side} onChange={(event) => setSide(event.target.value as "buy"|"sell")}><option value="sell">{t("sell")}</option><option value="buy">{t("buy")}</option></select></label><label>{t("amount")}<input inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} required/></label></div></section>
      <section className="panel execution-summary"><PanelTitle title={t("submissionSummary")} tag="02 / REVIEW"/>{template && version && instrument ? <><h2>{side === "buy" ? t("buy") : t("sell")} {instrument.symbol}</h2><p>{displayExchange(instrument.exchange)} · {template.name} · v{version.version}</p><dl className="definition-grid"><div><dt>{t("target")}</dt><dd>{amount} <small>{version.quantity_unit}</small></dd></div><div><dt>{t("duration")}</dt><dd>{version.duration_seconds}s</dd></div><div><dt>{t("interval")}</dt><dd>{version.slice_interval_ms}ms</dd></div><div><dt>{t("slippage")}</dt><dd>{version.max_slippage_bps} bps</dd></div></dl><div className="residual-warning">{t("residualWarning")}</div>{mutation.error && <ErrorPanel error={mutation.error}/>}<button className="primary-button full-button" disabled={mutation.isPending}>{t("startPaperExecution")}</button></> : <EmptyState text={t("templateRequired")}/>}</section></form></>;
}

export function ExecutionDetailPage() {
  const { t } = useTranslation(); const { executionId } = useParams({ from: "/executions/$executionId" }); const queryClient = useQueryClient();
  const task = useQuery({queryKey:["task",executionId],queryFn:()=>api.task(executionId),refetchInterval:(query)=>activeStatuses.has(query.state.data?.status??"")?3000:false}); const slices = useQuery({queryKey:["slices",executionId],queryFn:()=>api.slices(executionId)}); const events = useQuery({queryKey:["events",executionId],queryFn:()=>api.events(executionId)});
  const cancel = useMutation({mutationFn:()=>api.cancelTask(executionId),onSuccess:async()=>queryClient.invalidateQueries({queryKey:["task",executionId]})});
  if (task.error) return <ErrorPanel error={task.error}/>; if (!task.data) return <div className="loading-line"/>;
  const current = task.data; const residual = Number(current.residual_amount)/Number(current.requested_amount); const latest = slices.data?.at(-1); const taskEvents = events.data ?? [];
  const chart = slices.data?.map((slice)=>({sequence:slice.sequence,requested:Number(slice.requested_amount),filled:Number(slice.filled_native_quantity)})) ?? [];
  return <><PageHeader eyebrow={`EXECUTION / ${current.id.slice(0,8).toUpperCase()}`} title={`${current.strategy.toUpperCase()} · ${current.side.toUpperCase()}`} action={<div className="header-actions"><Link className="quiet-button" to="/executions">← {t("back")}</Link>{activeStatuses.has(current.status)&&<button className="alarm-button" onClick={()=>cancel.mutate()}>{t("cancel")}</button>}</div>}/><StabilityLine residual={residual} label={t("executionResidual")}/>
    <section className="detail-grid"><article className="panel"><PanelTitle title={t("executionOverview")} tag={statusText(current.status, t)}/><dl className="definition-grid"><div><dt>{t("target")}</dt><dd>{current.requested_amount} <small>{current.quantity_unit}</small></dd></div><div><dt>{t("executed")}</dt><dd>{current.executed_amount}</dd></div><div><dt>{t("residual")}</dt><dd className="copper">{current.residual_amount}</dd></div><div><dt>{t("nextSlice")}</dt><dd><RelativeTime value={current.next_tick_at}/></dd></div><div><dt>{t("slippage")}</dt><dd>{current.max_slippage_bps} bps</dd></div><div><dt>{t("templateVersion")}</dt><dd>{current.template_version_id.slice(0,8)}</dd></div></dl>{current.paused_reason&&<div className="residual-warning">{current.paused_reason}</div>}</article>
      <article className="panel chart-panel"><PanelTitle title={t("plannedVsActual")} tag="SLICE QUANTITY"/><div className="chart-space"><ResponsiveContainer width="100%" height="100%"><AreaChart data={chart} margin={{left:8,right:8}}><CartesianGrid vertical={false} stroke="var(--chart-grid)"/><XAxis dataKey="sequence"/><YAxis width={76}/><Tooltip/><Area dataKey="requested" stroke="var(--chart-secondary)" fill="transparent"/><Area dataKey="filled" stroke="var(--chart-primary)" fill="color-mix(in srgb, var(--chart-primary) 15%, transparent)"/></AreaChart></ResponsiveContainer></div></article></section>
    <section className="detail-grid"><article className="panel"><PanelTitle title={t("orderBookEvidence")} tag={latest?`SLICE ${latest.sequence}`:"WAITING"}/>{latest?<div className="evidence-block"><div><span>AVG PRICE</span><b>{latest.average_price??"—"}</b></div><div><span>WORST PRICE</span><b>{latest.worst_price??"—"}</b></div><div><span>SLIPPAGE</span><b>{latest.slippage_bps??"—"} bps</b></div><div><span>FEE</span><b>{latest.fee_amount??t("feeUnavailable")}</b></div><pre>{JSON.stringify({market:latest.market_snapshot,decision:latest.decision_input},null,2)}</pre></div>:<EmptyState text={t("noSlices")}/>}</article>
      <article className="panel"><PanelTitle title={t("stateEvents")} tag={`${taskEvents.length} EVENTS`}/><div className="event-list">{taskEvents.map((event)=><div key={event.event_id}><time>{new Date(event.created_at).toLocaleTimeString()}</time><b>{event.event_type}</b><code>#{event.sequence}</code></div>)}</div></article></section>
    <section className="panel timeline-panel"><PanelTitle title={t("sliceTimeline")} tag={`${slices.data?.length??0} SLICES`}/><div className="slice-timeline">{slices.data?.map((slice)=><div key={slice.id}><i/><time>{new Date(slice.created_at).toLocaleTimeString()}</time><b>#{slice.sequence} · {statusText(slice.status, t)}</b><span>{slice.filled_native_quantity} / {slice.native_quantity}</span><em>{slice.slippage_bps??"—"} bps</em></div>)}</div></section></>;
}

export function StrategiesPage() {
  const { t } = useTranslation(); const queryClient = useQueryClient(); const [showForm,setShowForm]=useState(false); const templates=useQuery({queryKey:["templates"],queryFn:api.templates});
  const create=useMutation({mutationFn:api.createTemplate,onSuccess:async()=>{setShowForm(false);await queryClient.invalidateQueries({queryKey:["templates"]});}});
  if(templates.error)return <ErrorPanel error={templates.error}/>;
  return <><PageHeader eyebrow="STRATEGY / GOVERNANCE" title={t("strategyCenter")} description={t("strategyCenterDescription")} action={<button className="primary-button" onClick={()=>setShowForm(!showForm)}>{showForm?t("close"):t("newTemplate")}</button>}/>{showForm&&<section className="panel inline-form"><PanelTitle title={t("newTemplate")} tag="VERSION 01"/><TemplateForm submitLabel={t("createTemplate")} onSubmit={(input)=>create.mutate(input)} error={create.error}/></section>}
    <section className="strategy-grid">{templates.data?.map((template)=>{const version=currentVersion(template);const native=version?.execution_backend==="venue_native_algo";return <article className={`strategy-card ${template.status}`} key={template.id}><header><span>{version?.strategy.toUpperCase()} · {native?t("nativeBackend"):t("managedBackend")}</span><StatusPill status={template.status}/></header><h2>{template.name}</h2><p>{template.description||t("noDescription")}</p><dl><div><dt>{t("currentVersion")}</dt><dd>v{template.current_version}</dd></div><div><dt>{t("usage")}</dt><dd>{template.usage_count}</dd></div><div><dt>{t("duration")}</dt><dd>{version?.duration_seconds}s</dd></div><div><dt>{t("slippage")}</dt><dd>{version?.max_slippage_bps} bps</dd></div></dl><div className="card-actions"><Link className="quiet-button" to="/strategies/$strategyId" params={{strategyId:template.id}}>{t("manage")}</Link>{template.status==="active"&&!native&&<Link className="primary-button" to="/executions/new" search={{template:template.id}}>{t("useTemplate")}</Link>}</div></article>})}</section></>;
}

export function StrategyDetailPage(){const {t}=useTranslation();const {strategyId}=useParams({from:"/strategies/$strategyId"});const client=useQueryClient();const template=useQuery({queryKey:["template",strategyId],queryFn:()=>api.template(strategyId)});const versionMutation=useMutation({mutationFn:(input:TemplateInput)=>api.createTemplateVersion(strategyId,input),onSuccess:async()=>client.invalidateQueries({queryKey:["template",strategyId]})});const archive=useMutation({mutationFn:()=>api.archiveTemplate(strategyId),onSuccess:async()=>client.invalidateQueries({queryKey:["template",strategyId]})});if(template.error)return <ErrorPanel error={template.error}/>;if(!template.data)return <div className="loading-line"/>;const data=template.data;const current=currentVersion(data);return <><PageHeader eyebrow="STRATEGY / VERSION CONTROL" title={data.name} description={data.description} action={<div className="header-actions"><Link className="quiet-button" to="/strategies">← {t("back")}</Link>{data.status==="active"&&<button className="alarm-button" onClick={()=>archive.mutate()}>{t("archive")}</button>}</div>}/><section className="detail-grid"><article className="panel"><PanelTitle title={t("versionHistory")} tag={`${data.versions.length} VERSIONS`}/><div className="version-list">{data.versions.map((version)=><div key={version.id}><b>v{version.version} · {version.strategy.toUpperCase()}</b><span>{version.quantity_unit} · {version.duration_seconds}s · {version.max_slippage_bps}bps</span><small>{version.change_note||t("noChangeNote")}</small><time>{new Date(version.created_at).toLocaleString()}</time></div>)}</div></article>{data.status==="active"&&<article className="panel inline-form"><PanelTitle title={t("newVersion")} tag={`FROM V${current?.version}`}/><TemplateForm initial={current} submitLabel={t("createVersion")} onSubmit={(input)=>versionMutation.mutate(input)} error={versionMutation.error}/></article>}</section></>}

function TemplateForm({initial,submitLabel,onSubmit,error}:{initial?:ReturnType<typeof currentVersion>;submitLabel:string;onSubmit:(input:TemplateInput)=>void;error:Error|null}){const {t}=useTranslation();const [strategy,setStrategy]=useState<"twap"|"pov">(initial?.strategy??"twap");const submit=(event:FormEvent<HTMLFormElement>)=>{event.preventDefault();const data=new FormData(event.currentTarget);const input:TemplateInput={name:String(data.get("name")||"")||undefined,description:String(data.get("description")||"")||undefined,strategy,quantity_unit:String(data.get("quantity_unit")) as TemplateInput["quantity_unit"],duration_seconds:Number(data.get("duration_seconds")),slice_interval_ms:Number(data.get("slice_interval_ms")),max_slippage_bps:Number(data.get("max_slippage_bps")),change_note:String(data.get("change_note")||""),execution_backend:"managed_ioc"};const participation=String(data.get("participation_rate")||"");const maximum=String(data.get("max_slice_amount")||"");if(participation)input.participation_rate=participation;if(maximum)input.max_slice_amount=maximum;onSubmit(input)};return <form className="task-form" onSubmit={submit}>{!initial&&<><label>{t("templateName")}<input name="name" required maxLength={120}/></label><label>{t("description")}<textarea name="description" rows={2}/></label></>}<div className="form-row"><label>{t("strategy")}<select value={strategy} name="strategy" onChange={(event)=>setStrategy(event.target.value as "twap"|"pov")}><option value="twap">TWAP</option><option value="pov">POV</option></select></label><label>{t("unit")}<select name="quantity_unit" defaultValue={initial?.quantity_unit??"base_quantity"}><option value="base_quantity">base_quantity</option><option value="quote_notional">quote_notional</option><option value="contracts">contracts</option></select></label></div><div className="form-row"><label>{t("duration")}<input name="duration_seconds" type="number" min="1" max="86400" defaultValue={initial?.duration_seconds??300}/></label><label>{t("interval")}<input name="slice_interval_ms" type="number" min="1" defaultValue={initial?.slice_interval_ms??5000}/></label></div><div className="form-row"><label>{t("slippage")}<input name="max_slippage_bps" type="number" min="0" max="10000" defaultValue={initial?.max_slippage_bps??20}/></label>{strategy==="pov"?<label>{t("participation")}<input name="participation_rate" inputMode="decimal" defaultValue={initial?.participation_rate??"0.1"}/></label>:<label>{t("maxSlice")}<input name="max_slice_amount" inputMode="decimal" defaultValue={initial?.max_slice_amount??""}/></label>}</div><label>{t("changeNote")}<input name="change_note" defaultValue=""/></label>{error&&<ErrorPanel error={error}/>}<button className="primary-button">{submitLabel}</button></form>}

export function AnalyticsPage(){const {t}=useTranslation();const [window,setWindow]=useState("24h");const analytics=useQuery({queryKey:["analytics",window],queryFn:()=>api.analytics(`window=${window}`)});if(analytics.error)return <ErrorPanel error={analytics.error}/>;const data=analytics.data;const statusData=Object.entries(data?.status_counts??{}).map(([name,value])=>({name,value}));return <><PageHeader eyebrow="EXECUTION / ANALYTICS" title={t("analytics")} description={t("analyticsDescription")} action={<select value={window} onChange={(event)=>setWindow(event.target.value)}><option value="24h">24H</option><option value="7d">7D</option><option value="30d">30D</option></select>}/><section className="metric-strip"><Metric label={t("tasks")} value={String(data?.task_count??0)}/><Metric label={t("completion")} value={`${(Number(data?.average_completion_ratio??0)*100).toFixed(1)}%`} note={t("taskAverage")}/><Metric label={t("medianSlippage")} value={data?.median_slippage_bps?`${data.median_slippage_bps} bps`:"—"}/><Metric label="P95 SLIPPAGE" value={data?.p95_slippage_bps?`${data.p95_slippage_bps} bps`:"—"}/></section><section className="dashboard-grid"><article className="panel chart-panel"><PanelTitle title={t("statusDistribution")} tag={window.toUpperCase()}/><div className="chart-space"><ResponsiveContainer width="100%" height="100%"><BarChart data={statusData}><CartesianGrid vertical={false} stroke="var(--chart-grid)"/><XAxis dataKey="name"/><YAxis/><Tooltip/><Bar dataKey="value" fill="var(--chart-primary)"/></BarChart></ResponsiveContainer></div></article><article className="panel"><PanelTitle title={t("operationalEfficiency")} tag="DURATION / EXCEPTIONS"/><dl className="definition-grid"><div><dt>{t("medianRuntime")}</dt><dd>{data?.median_runtime_ms?`${Math.round(data.median_runtime_ms/1000)}s`:"—"}</dd></div><div><dt>{t("feeUnavailable")}</dt><dd>{(Number(data?.fee_unavailable_ratio??0)*100).toFixed(1)}%</dd></div><div><dt>{t("exceptions")}</dt><dd>{data?.exception_count??0}</dd></div><div><dt>{t("completed")}</dt><dd>{data?.completed_count??0}</dd></div></dl></article><article className="panel"><PanelTitle title={t("pauseReasons")} tag="CAUSE"/><Breakdown data={data?.pause_reason_counts??{}} empty={t("noPauseReasons")}/></article><article className="panel"><PanelTitle title={t("venueDistribution")} tag="TASK COUNT"/><Breakdown data={data?.exchange_counts??{}} empty={t("noTasks")}/></article></section></>}

function Breakdown({data,empty}:{data:Record<string,number>;empty:string}){const entries=Object.entries(data);return entries.length?<div className="breakdown-list">{entries.map(([label,value])=><div key={label}><span>{label.replaceAll("_"," ")}</span><b>{value}</b></div>)}</div>:<EmptyState text={empty}/>}

function InstrumentDirectory({fixedExchange}:{fixedExchange?:Instrument["exchange"]}){const {t}=useTranslation();const [exchange,setExchange]=useState<"all"|Instrument["exchange"]>(fixedExchange??"all");const [market,setMarket]=useState<"all"|Instrument["market_kind"]>("all");const [search,setSearch]=useState("");const deferredSearch=useDeferredValue(search);const [page,setPage]=useState(0);const limit=100;const instruments=useQuery({queryKey:["instrument-page",exchange,market,deferredSearch,page],queryFn:()=>api.instruments({exchange:exchange==="all"?undefined:exchange,market_kind:market==="all"?undefined:market,search:deferredSearch,limit,offset:page*limit})});const data=instruments.data;const start=data&&data.total>0?data.offset+1:0;const end=data?Math.min(data.offset+data.items.length,data.total):0;const changeScope=(apply:()=>void)=>{apply();setPage(0)};return <><section className="filter-bar market-filters" aria-label={t("filters")}><input value={search} onChange={(event)=>changeScope(()=>setSearch(event.target.value))} placeholder={t("searchInstrument")}/>{!fixedExchange&&<select value={exchange} onChange={(event)=>changeScope(()=>setExchange(event.target.value as typeof exchange))}><option value="all">{t("allVenues")}</option>{venueOptions.map((item)=><option key={item} value={item}>{displayExchange(item)}</option>)}</select>}<select value={market} onChange={(event)=>changeScope(()=>setMarket(event.target.value as typeof market))}><option value="all">{t("allMarketKinds")}</option><option value="spot">{t("spot")}</option><option value="perpetual">{t("perpetual")}</option></select><span>{t("instrumentRange",{start,end,total:data?.total??0})}</span></section><section className="panel market-panel"><PanelTitle title={t("instrumentMatrix")} tag={`${data?.total??0} INSTRUMENTS`}/>{instruments.error?<ErrorPanel error={instruments.error}/>:<InstrumentTable instruments={data?.items??[]}/>}<footer className="table-pagination"><button className="quiet-button" disabled={page===0} onClick={()=>setPage((value)=>Math.max(0,value-1))}>{t("previousPage")}</button><span>{page+1} / {Math.max(1,Math.ceil((data?.total??0)/limit))}</span><button className="quiet-button" disabled={!data||data.offset+data.items.length>=data.total} onClick={()=>setPage((value)=>value+1)}>{t("nextPage")}</button></footer></section></>}

export function VenuesPage(){const {t}=useTranslation();const client=useQueryClient();const exchanges=useQuery({queryKey:["exchanges"],queryFn:api.exchanges});const sync=useMutation({mutationFn:api.syncInstruments,onSuccess:async()=>{await client.invalidateQueries({queryKey:["instrument-page"]});await client.invalidateQueries({queryKey:["exchanges"]})}});if(exchanges.error)return <ErrorPanel error={exchanges.error}/>;return <><PageHeader eyebrow="MARKET / VENUE MATRIX" title={t("venues")} description={t("venuesDescription")} action={<button className="primary-button" onClick={()=>sync.mutate()}>{sync.isPending?t("syncing"):t("syncMarkets")}</button>}/><section className="venue-grid">{exchanges.data?.map((exchange)=><Link className="venue-card" to="/venues/$exchange" params={{exchange:exchange.exchange}} key={exchange.exchange}><header><b>{displayExchange(exchange.exchange)}</b><StatusPill status={exchange.status}/></header><strong>{displayLatency(exchange.health_query_latency_ms)}<small>{t("healthQueryLatency")}</small></strong><dl><div><dt>{t("instruments")}</dt><dd>{exchange.instrument_count}</dd></div><div><dt>{t("activeSubscriptions")}</dt><dd>{exchange.active_subscriptions}</dd></div><div><dt>{t("stale")}</dt><dd>{exchange.stale_subscriptions}</dd></div></dl></Link>)}</section><InstrumentDirectory/></>}

function InstrumentTable({instruments}:{instruments:Instrument[]}){const {t}=useTranslation();return instruments.length?<div className="table-scroll"><table className="data-table market-table"><thead><tr><th>{t("instrument")}</th><th>{t("venue")}</th><th>{t("contract")}</th><th>{t("precision")}</th><th>{t("minimum")}</th><th>{t("fees")}</th></tr></thead><tbody>{instruments.map((item)=><tr key={item.id}><td><b>{item.symbol}</b><small>{item.exchange_symbol}</small></td><td>{displayExchange(item.exchange)}</td><td><b>{item.market_kind}</b><small>{item.contract_kind??"spot"}{item.contract_size?` · ${item.contract_size}`:""}</small></td><td className="mono">P {item.price_tick}<small>Q {item.quantity_step}</small></td><td className="mono">{item.minimum_quantity??"—"}<small>{item.minimum_notional??"—"}</small></td><td className="mono">{item.taker_fee_rate??t("feeUnavailable")}</td></tr>)}</tbody></table></div>:<EmptyState text={t("noMarkets")}/>}

export function VenueDetailPage(){const {t}=useTranslation();const {exchange}=useParams({from:"/venues/$exchange"});const venue=useQuery({queryKey:["exchange",exchange],queryFn:()=>api.exchange(exchange)});const events=useQuery({queryKey:["health-events",exchange],queryFn:()=>api.healthEvents(exchange)});const subscriptions=useQuery({queryKey:["subscriptions",exchange],queryFn:()=>api.subscriptions(exchange)});const header=<PageHeader eyebrow="VENUE / CONNECTIVITY" title={displayExchange(exchange)} description={t("venueDetailDescription")} action={<Link className="quiet-button" to="/venues">← {t("back")}</Link>}/>;if(venue.error)return <>{header}<ErrorPanel error={venue.error}/></>;if(!venue.data)return <>{header}<section className="panel venue-loading" role="status" aria-live="polite"><i/><b>{t("loadingVenue")}</b></section></>;const data=venue.data;return <>{header}<section className="metric-strip"><Metric label={t("healthQueryLatency")} value={displayLatency(data.health_query_latency_ms)} note={data.status}/><Metric label={t("instruments")} value={String(data.instrument_count)}/><Metric label={t("activeSubscriptions")} value={String(data.active_subscriptions)}/><Metric label={t("stale")} value={String(data.stale_subscriptions)} tone={data.stale_subscriptions>0?"alarm":"default"}/></section><section className="detail-grid"><article className="panel"><PanelTitle title={t("capabilities")} tag="ADAPTER"/><div className="capability-list">{data.capabilities&&Object.entries(data.capabilities).map(([name,supported])=><div key={name}><span>{name.replaceAll("_"," ")}</span><b>{supported?t("supported"):t("unsupported")}</b></div>)}</div></article><article className="panel"><PanelTitle title={t("subscriptions")} tag="REAL STATE"/>{subscriptions.data?.length?<div className="event-list">{subscriptions.data.map((item)=><div key={`${item.instrument_id}-${item.stream_kind}`}><StatusPill status={item.status}/><b>{item.stream_kind}</b><span>{item.reconnect_attempt} reconnect</span></div>)}</div>:<EmptyState text={t("notSubscribed")}/>}</article></section><section className="panel health-history-panel"><PanelTitle title={t("healthHistory")} tag="TRANSITIONS"/><div className="event-list">{events.data?.map((event)=><div key={event.sequence}><time>{new Date(event.observed_at).toLocaleString()}</time><StatusPill status={event.status}/><span>{displayLatency(event.health_query_latency_ms)}</span><code>{event.error_code??"—"}</code></div>)}</div></section><InstrumentDirectory fixedExchange={exchange as Instrument["exchange"]}/></>}

export function summarizeGateway(exchanges: ExchangeStatus[]|undefined,isPending:boolean,isError:boolean){const readyCount=exchanges?.filter((exchange)=>exchange.status==="ready").length??0;return{readyCount,status:isPending?"checking":isError||readyCount!==venueOptions.length?"degraded":"ready"}}

export function SystemPage(){const {t}=useTranslation();const eventStreamConnected=useEventStreamStatus();const health=useQuery({queryKey:["server-health"],queryFn:api.health,refetchInterval:10000});const exchanges=useQuery({queryKey:["exchange-snapshots"],queryFn:api.exchangeSnapshots});const gateway=summarizeGateway(exchanges.data,exchanges.isPending,exchanges.isError);return <><PageHeader eyebrow="SYSTEM / STATUS" title={t("systemStatus")} description={t("systemDescription")}/><section className="system-stack"><article className="system-row"><div><span>HTTP API</span><b>ballast-server</b></div><StatusPill status={health.data?.status??"checking"}/><code>{health.data?.version??"—"}</code></article><article className="system-row"><div><span>DATABASE</span><b>PostgreSQL</b></div><StatusPill status={health.data?.status==="ok"?"ready":"degraded"}/><code>{t("checkedByHealth")}</code></article><article className="system-row"><div><span>GATEWAY</span><b>Node / ccxt</b></div><StatusPill status={gateway.status}/><code>{gateway.readyCount}/5 {t("gatewayAdaptersReady")}</code></article><article className="system-row"><div><span>WEBSOCKET</span><b>Task event stream</b></div><StatusPill status={eventStreamConnected?"ready":"degraded"}/><code>{t("clientObserved")}</code></article></section></>}

export function RoadmapPage(){const {t}=useTranslation();const modules=[{phase:"05",title:t("accountsPositions"),body:t("accountsRoadmap"),requirements:["OIDC","viewer / operator / admin","read-only API keys"]},{phase:"06",title:t("riskControls"),body:t("riskRoadmap"),requirements:["kill switch","zero-default limits","testnet reconciliation"]},{phase:"07",title:t("hedging"),body:t("hedgingRoadmap"),requirements:["fill-driven hedge","maximum naked time","manual intervention"]}];return <><PageHeader eyebrow="PRODUCT / SAFETY ROADMAP" title={t("roadmap")} description={t("roadmapDescription")}/><section className="roadmap-grid">{modules.map((module)=><article className="roadmap-card" key={module.phase}><span>PHASE {module.phase}</span><h2>{module.title}</h2><p>{module.body}</p><ul>{module.requirements.map((item)=><li key={item}>{item}</li>)}</ul><footer>{t("notEnabled")}</footer></article>)}</section></>}

export function ApprovalsPage(){const {t}=useTranslation();return <LockedCapabilityPage eyebrow="LIVE / APPROVAL" title={t("approvals")} rules={[t("approvalRule"),t("selfApprovalDenied"),t("submitRiskRecheck")]} empty={t("noPendingApprovals")}/>}
export function AccountsPage(){const {t}=useTranslation();return <LockedCapabilityPage eyebrow="PRIVATE / RECONCILIATION" title={t("accountsReconciliation")} rules={[t("accountSecretsBoundary"),t("withdrawalDenied"),t("reconciliationRequired")]} empty={t("noAccountsConfigured")}/>}
export function RiskPage(){const {t}=useTranslation();return <LockedCapabilityPage eyebrow="RISK / CONTROL PLANE" title={t("riskControls")} rules={[t("zeroDefaultRisk"),t("killSwitchHierarchy"),t("submitRiskRecheck")]} empty={t("noRiskConfigured")}/>}
export function HedgingPage(){const {t}=useTranslation();return <LockedCapabilityPage eyebrow="HEDGE / EXPOSURE" title={t("hedging")} rules={[t("hedgeExecutionRule"),t("hedgeFailureRule"),t("submitRiskRecheck")]} empty={t("noHedgeConfigured")} stability/>}

function LockedCapabilityPage({eyebrow,title,rules,empty,stability=false}:{eyebrow:string;title:string;rules:string[];empty:string;stability?:boolean}){const {t}=useTranslation();return <><PageHeader eyebrow={eyebrow} title={title} description={t("privatePlaneDescription")}/><div className="mode-banner locked-banner"><b>{t("locked")}</b><span>{t("privatePlaneLocked")}</span><time>LIVE = OFF</time></div>{stability&&<StabilityLine residual={0} label={t("executionResidual")}/>}<section className="locked-capability-grid"><article className="panel lock-panel"><PanelTitle title={t("safetyBoundary")} tag="FAIL CLOSED"/><div className="lock-symbol" aria-hidden="true"><i/><i/></div><h2>{t("privatePlaneLocked")}</h2><p>{empty}</p></article><article className="panel"><PanelTitle title={t("prerequisites")} tag="REQUIRED"/><ol className="safety-ledger">{rules.map((rule,index)=><li key={rule}><span>{String(index+1).padStart(2,"0")}</span><p>{rule}</p><StatusPill status="locked"/></li>)}</ol></article></section></>}
