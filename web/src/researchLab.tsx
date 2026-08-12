import { FormEvent, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  api,
  StrategyTemplate,
  TemplateInput,
  ValidationCase,
  ValidationDecision,
  ValidationReport,
  ValidationRun,
} from "./api";
import {
  EmptyState,
  ErrorPanel,
  PageHeader,
  PanelTitle,
  StatusPill,
  currentVersion,
} from "./components";
import { useTranslation } from "react-i18next";

export function StrategiesPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [section, setSection] = useState<"library" | "cases" | "data" | "experiments">("experiments");
  const [showForm, setShowForm] = useState(false);
  const templates = useQuery({ queryKey: ["templates"], queryFn: api.templates });
  const cases = useQuery({ queryKey: ["validation-cases"], queryFn: api.validationCases });
  const jobs = useQuery({
    queryKey: ["research-download-jobs"],
    queryFn: api.downloadJobs,
    refetchInterval: (query) =>
      query.state.data?.some((job) => ["queued", "downloading", "verifying"].includes(job.status))
        ? 2_000
        : false,
  });
  const runs = useQuery({
    queryKey: ["validation-runs"],
    queryFn: api.validationRuns,
    refetchInterval: (query) =>
      query.state.data?.some((run) => ["queued", "preparing", "running"].includes(run.status))
        ? 2_000
        : false,
  });
  const create = useMutation({
    mutationFn: api.createTemplate,
    onSuccess: async () => {
      setShowForm(false);
      await queryClient.invalidateQueries({ queryKey: ["templates"] });
    },
  });
  if (templates.error) return <ErrorPanel error={templates.error} />;
  const researchTemplates = templates.data?.filter((template) => template.scope === "research") ?? [];
  const executionTemplates = templates.data?.filter((template) => template.scope === "paper_execution") ?? [];
  const currentCase = cases.data?.[0];
  const strategyVersionIds = researchTemplates
    .map((template) => currentVersion(template)?.id)
    .filter((value): value is string => Boolean(value));
  const action =
    section === "library" ? (
      <button className="primary-button" onClick={() => setShowForm(!showForm)}>
        {showForm ? t("close") : t("newTemplate")}
      </button>
    ) : undefined;
  return (
    <>
      <PageHeader
        eyebrow="RESEARCH / VALIDATION LAB V2"
        title={t("strategyCenter")}
        description="配置验证案例，运行固定四策略比较，导出报告并留下人工决策。"
        action={action}
      />
      <nav className="research-tabs" aria-label="策略中心分区">
        {(
          [
            ["library", "策略库", researchTemplates.length],
            ["cases", "验证案例", cases.data?.length ?? 0],
            ["data", "数据", jobs.data?.[0]?.verified_sessions ?? 0],
            ["experiments", "实验", runs.data?.length ?? 0],
          ] as const
        ).map(([item, label, count]) => (
          <button
            key={item}
            className={section === item ? "is-active" : ""}
            onClick={() => setSection(item)}
          >
            <span>{label}</span>
            <small>{count}</small>
          </button>
        ))}
      </nav>
      {section === "library" && (
        <StrategyLibrary
          research={researchTemplates}
          execution={executionTemplates}
          showForm={showForm}
          create={create}
        />
      )}
      {section === "cases" && <ValidationCases cases={cases.data ?? []} error={cases.error} />}
      {section === "data" && <ResearchDataPanel jobs={jobs.data ?? []} error={jobs.error} currentCase={currentCase} />}
      {section === "experiments" && (
        <ExperimentPanel
          currentCase={currentCase}
          strategyVersionIds={strategyVersionIds}
          runs={runs.data ?? []}
          error={runs.error}
        />
      )}
    </>
  );
}

function StrategyLibrary({
  research,
  execution,
  showForm,
  create,
}: {
  research: StrategyTemplate[];
  execution: StrategyTemplate[];
  showForm: boolean;
  create: { mutate: (input: TemplateInput) => void; error: Error | null };
}) {
  const { t } = useTranslation();
  return (
    <>
      {showForm && (
        <section className="panel inline-form">
          <PanelTitle title={t("newTemplate")} tag="PAPER EXECUTION / VERSION 01" />
          <TemplateForm
            submitLabel={t("createTemplate")}
            onSubmit={(input) => create.mutate(input)}
            error={create.error}
          />
        </section>
      )}
      <section className="research-section">
        <header>
          <div>
            <span>RESEARCH CANDIDATES</span>
            <h2>不可变比较版本</h2>
          </div>
          <p>算法参数留在版本中；金额、验证日和参与率上限留在案例中。</p>
        </header>
        <section className="strategy-grid">
          {research.map((template) => (
            <StrategyCard key={template.id} template={template} research />
          ))}
        </section>
      </section>
      <section className="research-section">
        <header>
          <div>
            <span>PAPER EXECUTION</span>
            <h2>现有加密执行模板</h2>
          </div>
          <p>继续服务纸面执行任务，不与股票研究案例混用。</p>
        </header>
        <section className="strategy-grid">
          {execution.map((template) => (
            <StrategyCard key={template.id} template={template} />
          ))}
        </section>
      </section>
    </>
  );
}

function StrategyCard({ template, research = false }: { template: StrategyTemplate; research?: boolean }) {
  const { t } = useTranslation();
  const version = currentVersion(template);
  const native = version?.execution_backend === "venue_native_algo";
  return (
    <article className={`strategy-card ${template.status} ${research ? "research-card" : ""}`}>
      <header>
        <span>
          {version?.strategy.toUpperCase()} ·{" "}
          {research ? "RESEARCH" : native ? t("nativeBackend") : t("managedBackend")}
        </span>
        <StatusPill status={template.status} />
      </header>
      <h2>{template.name}</h2>
      <p>{template.description || t("noDescription")}</p>
      <dl>
        <div>
          <dt>{t("currentVersion")}</dt>
          <dd>v{template.current_version}</dd>
        </div>
        <div>
          <dt>配置</dt>
          <dd>
            {research
              ? Object.entries(version?.algorithm_config ?? {})
                  .filter(([key]) => key !== "algorithm")
                  .map(([key, value]) => `${key.replaceAll("_", " ")} ${String(value)}`)
                  .join(" · ") || "fixed"
              : `${version?.duration_seconds}s`}
          </dd>
        </div>
      </dl>
      <div className="card-actions">
        <Link className="quiet-button" to="/strategies/$strategyId" params={{ strategyId: template.id }}>
          {t("manage")}
        </Link>
        {!research && template.status === "active" && !native && (
          <Link className="primary-button" to="/executions/new" search={{ template: template.id }}>
            {t("useTemplate")}
          </Link>
        )}
      </div>
    </article>
  );
}

function ValidationCases({ cases, error }: { cases: ValidationCase[]; error: Error | null }) {
  const client = useQueryClient();
  const [editing, setEditing] = useState<string>();
  const update = useMutation({
    mutationFn: ({
      id,
      input,
    }: {
      id: string;
      input: Parameters<typeof api.updateValidationCase>[1];
    }) => api.updateValidationCase(id, input),
    onSuccess: async () => {
      setEditing(undefined);
      await client.invalidateQueries({ queryKey: ["validation-cases"] });
    },
  });
  if (error) return <ErrorPanel error={error} />;
  return (
    <section className="case-stack">
      {cases.map((item) => {
        const requiredSessions = item.warmup_sessions + item.evaluation_sessions;
        return (
          <article className="case-sheet" key={item.id}>
            <header>
              <div>
                <span>CASE / V{item.version}</span>
                <h2>{item.name}</h2>
              </div>
              <StatusPill status={item.status} />
            </header>
            <div className="case-thesis">
              <strong>{item.side.toUpperCase()}</strong>
              <b>
                ${Number(item.target_notional_usd).toLocaleString()} {item.symbol}
              </b>
              <p>
                固定 {item.start_time.slice(0, 5)}–{item.end_time.slice(0, 5)} ET 窗口；编辑案例只影响后续新 run，历史 run 保留快照。
              </p>
            </div>
            <dl>
              <div>
                <dt>标的</dt>
                <dd>
                  {item.symbol} · {item.asset_class}/{item.instrument_kind}
                </dd>
              </div>
              <div>
                <dt>窗口</dt>
                <dd>
                  {item.start_time.slice(0, 5)}–{item.end_time.slice(0, 5)} ET
                </dd>
              </div>
              <div>
                <dt>最大参与率</dt>
                <dd>{(Number(item.max_participation_rate) * 100).toFixed(1)}%</dd>
              </div>
              <div>
                <dt>数据需求</dt>
                <dd>
                  {item.warmup_sessions} warm-up + {item.evaluation_sessions} evaluation = {requiredSessions} 日
                </dd>
              </div>
              <div>
                <dt>数据源</dt>
                <dd>
                  {item.market_data_provider}/{item.market_data_dataset}
                </dd>
              </div>
              <div>
                <dt>执行场所</dt>
                <dd>{item.execution_venue ?? "未指定（研究）"}</dd>
              </div>
            </dl>
            <button
              className="quiet-button"
              onClick={() => setEditing(editing === item.id ? undefined : item.id)}
            >
              {editing === item.id ? "关闭编辑" : "编辑案例参数"}
            </button>
            {editing === item.id && (
              <form
                className="case-editor"
                onSubmit={(event) => {
                  event.preventDefault();
                  const data = new FormData(event.currentTarget);
                  update.mutate({
                    id: item.id,
                    input: {
                      name: String(data.get("name")),
                      target_notional_usd: String(data.get("notional")),
                      max_participation_rate: String(data.get("participation")),
                      evaluation_sessions: Number(data.get("sessions")),
                    },
                  });
                }}
              >
                <label>
                  名称
                  <input name="name" defaultValue={item.name} required maxLength={120} />
                </label>
                <label>
                  名义金额 USD
                  <input name="notional" defaultValue={item.target_notional_usd} required />
                </label>
                <label>
                  最大参与率 (0-1)
                  <input name="participation" defaultValue={item.max_participation_rate} required />
                </label>
                <label>
                  验证交易日 (1-200)
                  <input
                    name="sessions"
                    type="number"
                    min={1}
                    max={200}
                    defaultValue={item.evaluation_sessions}
                    required
                  />
                </label>
                <p className="case-editor-hint">
                  Warm-up 固定 20 日以匹配 VWAP 模板；保存后版本号递增。需要至少 {item.warmup_sessions}+N 个已验证交易日数据。
                </p>
                {update.error && <ErrorPanel error={update.error} />}
                <button className="primary-button">保存案例</button>
              </form>
            )}
          </article>
        );
      })}
    </section>
  );
}

function ResearchDataPanel({
  jobs,
  error,
  currentCase,
}: {
  jobs: Awaited<ReturnType<typeof api.downloadJobs>>;
  error: Error | null;
  currentCase?: ValidationCase;
}) {
  const client = useQueryClient();
  const [coverage, setCoverage] = useState<Awaited<ReturnType<typeof api.researchCoverage>>>();
  const check = useMutation({ mutationFn: api.researchCoverage, onSuccess: setCoverage });
  const download = useMutation({
    mutationFn: api.createDownloadJob,
    onSuccess: async () => client.invalidateQueries({ queryKey: ["research-download-jobs"] }),
  });
  const cancel = useMutation({
    mutationFn: api.cancelDownloadJob,
    onSuccess: async () => client.invalidateQueries({ queryKey: ["research-download-jobs"] }),
  });
  if (error) return <ErrorPanel error={error} />;
  const latest = jobs[0];
  const required = currentCase
    ? currentCase.warmup_sessions + currentCase.evaluation_sessions
    : 120;
  return (
    <>
      <section className="data-source-banner">
        <div>
          <span>FREE RESEARCH FEED</span>
          <h2>Alpaca Basic · IEX</h2>
          <p>
            免费 IEX 分钟数据。当前案例需要约 {required} 个交易日（含 20 日 warm-up）。下载任务固定 120
            个交易日，足够覆盖默认与缩短案例。
          </p>
        </div>
        <div className="data-source-actions">
          <button className="quiet-button" onClick={() => check.mutate()} disabled={check.isPending}>
            {check.isPending ? "检查中" : "检查覆盖"}
          </button>
          <button
            className="primary-button"
            onClick={() => download.mutate()}
            disabled={!coverage?.available || download.isPending}
          >
            {download.isPending ? "创建中" : "下载 120 个交易日"}
          </button>
        </div>
      </section>
      {check.error && <ErrorPanel error={check.error} />}
      {download.error && <ErrorPanel error={download.error} />}
      {coverage && (
        <div className="coverage-line">
          <StatusPill status={coverage.available ? "ready" : "failed"} />
          <b>{coverage.symbol}</b>
          <span>
            {new Date(coverage.start).toLocaleDateString()} → {new Date(coverage.end).toLocaleDateString()}
          </span>
          <code>{coverage.limitation}</code>
        </div>
      )}
      <section className="panel research-job-panel">
        <PanelTitle title="历史数据任务" tag={`${jobs.length} JOBS`} />
        {jobs.length ? (
          <div className="research-job-list">
            {jobs.map((job) => (
              <div key={job.id}>
                <StatusPill status={job.status} />
                <b>
                  {job.symbol} · {job.dataset}
                </b>
                <span>
                  {job.verified_sessions}/{job.requested_sessions} sessions
                </span>
                <code>{job.downloaded_records.toLocaleString()} bars</code>
                {["queued", "downloading", "verifying"].includes(job.status) ? (
                  <button className="quiet-button" onClick={() => cancel.mutate(job.id)}>
                    取消
                  </button>
                ) : (
                  <time>{new Date(job.updated_at).toLocaleString()}</time>
                )}
              </div>
            ))}
          </div>
        ) : (
          <EmptyState text="先检查覆盖，再创建本地历史数据任务。" />
        )}
      </section>
      {latest?.status === "failed" && <div className="residual-warning">{latest.error_code}</div>}
    </>
  );
}

function ExperimentPanel({
  currentCase,
  strategyVersionIds,
  runs,
  error,
}: {
  currentCase?: ValidationCase;
  strategyVersionIds: string[];
  runs: ValidationRun[];
  error: Error | null;
}) {
  const client = useQueryClient();
  const [selectedRun, setSelectedRun] = useState<string>();
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const create = useMutation({
    mutationFn: () => api.createValidationRun(currentCase!.id, strategyVersionIds),
    onSuccess: async (run) => {
      setSelectedRun(run.id);
      await client.invalidateQueries({ queryKey: ["validation-runs"] });
    },
  });
  const compare = useMutation({ mutationFn: (ids: string[]) => api.compareValidationRuns(ids) });
  const run = runs.find((item) => item.id === selectedRun) ?? runs[0];
  const required = currentCase
    ? currentCase.warmup_sessions + currentCase.evaluation_sessions
    : 120;

  const toggleCompare = (id: string) => {
    setCompareIds((current) => {
      if (current.includes(id)) return current.filter((item) => item !== id);
      if (current.length >= 8) return current;
      return [...current, id];
    });
  };

  return (
    <>
      <section className="experiment-launch">
        <div>
          <span>FIXED COMPARISON SET</span>
          <h2>Immediate · TWAP · POV · VWAP</h2>
          <p>
            {currentCase
              ? `${currentCase.name} · $${Number(currentCase.target_notional_usd).toLocaleString()} · max ${(Number(currentCase.max_participation_rate) * 100).toFixed(1)}% · ${currentCase.evaluation_sessions} 验证日（需 ${required} 日数据）`
              : "先准备验证案例与数据。"}
          </p>
        </div>
        <button
          className="primary-button"
          disabled={!currentCase || strategyVersionIds.length !== 4 || create.isPending}
          onClick={() => create.mutate()}
        >
          {create.isPending ? "正在创建" : "运行验证"}
        </button>
      </section>
      {create.error && <ErrorPanel error={create.error} />}
      {error && <ErrorPanel error={error} />}
      <div className="experiment-layout">
        <aside className="run-rail">
          <div className="run-rail-tools">
            <button
              className="quiet-button"
              disabled={compareIds.length < 2 || compare.isPending}
              onClick={() => compare.mutate(compareIds)}
            >
              比较已选 ({compareIds.length})
            </button>
          </div>
          {runs.map((item) => (
            <div className={`run-rail-item ${item.id === run?.id ? "is-active" : ""}`} key={item.id}>
              <label className="run-compare-check">
                <input
                  type="checkbox"
                  checked={compareIds.includes(item.id)}
                  disabled={item.status !== "succeeded" && !compareIds.includes(item.id)}
                  onChange={() => toggleCompare(item.id)}
                />
              </label>
              <button onClick={() => setSelectedRun(item.id)}>
                <StatusPill status={item.status} />
                <b>{item.id.slice(0, 8)}</b>
                <time>{new Date(item.created_at).toLocaleDateString()}</time>
                <small>
                  {String(item.case_snapshot?.name ?? "case")} · v
                  {String(item.case_snapshot?.version ?? "?")}
                </small>
              </button>
            </div>
          ))}
        </aside>
        <main>
          {compare.data && (
            <ComparePanel
              items={compare.data.items}
              onClear={() => compare.reset()}
            />
          )}
          {compare.error && <ErrorPanel error={compare.error} />}
          {!run ? (
            <EmptyState text="数据准备完成后，在这里运行策略比较。" />
          ) : run.status === "succeeded" && run.report ? (
            <ValidationReportPanel report={run.report} run={run} />
          ) : (
            <section className="panel run-state">
              <PanelTitle title="验证运行" tag={run.id.slice(0, 8)} />
              <div>
                <StatusPill status={run.status} />
                <p>
                  {run.status === "failed"
                    ? run.error_code
                    : "正在准备确定性回放和逐日指标。"}
                </p>
              </div>
            </section>
          )}
        </main>
      </div>
    </>
  );
}

function ComparePanel({
  items,
  onClear,
}: {
  items: Awaited<ReturnType<typeof api.compareValidationRuns>>["items"];
  onClear: () => void;
}) {
  const strategies = ["immediate", "twap", "pov", "vwap"] as const;
  return (
    <section className="panel compare-panel">
      <PanelTitle title="多 run 对比" tag={`${items.length} RUNS`} />
      <div className="compare-actions">
        <button className="quiet-button" onClick={onClear}>
          关闭对比
        </button>
      </div>
      <div className="table-scroll">
        <table className="data-table report-table">
          <thead>
            <tr>
              <th>策略 / 指标</th>
              {items.map((item) => (
                <th key={item.run.id}>
                  {item.run.id.slice(0, 8)}
                  <small>
                    {String(item.case_summary.name ?? "")} · $
                    {String(item.case_summary.target_notional_usd ?? "")}
                  </small>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {strategies.map((strategy) => (
              <tr key={strategy}>
                <td>
                  <b>{strategy.toUpperCase()}</b>
                  <small>median IS / p95 / complete</small>
                </td>
                {items.map((item) => {
                  const row = item.aggregates.find((aggregate) => aggregate.strategy === strategy);
                  return (
                    <td key={`${item.run.id}-${strategy}`} className="mono">
                      {row
                        ? `${Number(row.median_shortfall_bps).toFixed(2)} / ${Number(row.p95_shortfall_bps).toFixed(2)} / ${row.completed_sessions}/${row.evaluated_sessions}`
                        : "—"}
                    </td>
                  );
                })}
              </tr>
            ))}
            <tr>
              <td>
                <b>LATEST DECISION</b>
              </td>
              {items.map((item) => (
                <td key={`${item.run.id}-decision`}>
                  {item.latest_decision ? (
                    <>
                      <StatusPill status={item.latest_decision.decision} />
                      <small>{item.latest_decision.note || "无备注"}</small>
                    </>
                  ) : (
                    "—"
                  )}
                </td>
              ))}
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ValidationReportPanel({ report, run }: { report: ValidationReport; run: ValidationRun }) {
  const client = useQueryClient();
  const [strategy, setStrategy] = useState<"immediate" | "twap" | "pov" | "vwap">("twap");
  const [note, setNote] = useState("");
  const decisions = useQuery({
    queryKey: ["validation-decisions", run.id],
    queryFn: () => api.validationDecisions(run.id),
  });
  const decision = useMutation({
    mutationFn: (value: "validated" | "rejected") =>
      api.createValidationDecision(run.id, value, note),
    onSuccess: async () => {
      setNote("");
      await client.invalidateQueries({ queryKey: ["validation-decisions", run.id] });
    },
  });
  const sample = report.sessions[0];
  const candidate = sample?.candidates.find((item) => item.strategy === strategy);
  const caseName = String(
    (report.case as { symbol?: string }).symbol
      ? `${(report.case as { side?: string }).side?.toUpperCase() ?? "SELL"} $${String((report.case as { target_notional_usd?: string }).target_notional_usd ?? "")} ${(report.case as { symbol?: string }).symbol}`
      : "Validation report",
  );
  const caseMeta = report.case as {
    evaluation_sessions?: number;
    max_participation_rate?: string;
    start_time?: string;
    end_time?: string;
  };

  return (
    <div className="report-stack">
      <header className="report-header">
        <div>
          <span>VALIDATION REPORT / IEX PROXY</span>
          <h2>{caseName}</h2>
          <p>
            {report.sessions.length} 个有效交易日 · {Object.keys(report.invalid_sessions).length} 个无效日期
            {caseMeta.evaluation_sessions != null && ` · 配置验证日 ${caseMeta.evaluation_sessions}`}
            {caseMeta.max_participation_rate != null &&
              ` · 参与率 cap ${(Number(caseMeta.max_participation_rate) * 100).toFixed(1)}%`}
          </p>
        </div>
        <div className="report-export-actions">
          <a className="quiet-button" href={api.exportValidationRunUrl(run.id, "json")}>
            导出 JSON
          </a>
          <a
            className="quiet-button"
            href={api.exportValidationRunUrl(run.id, "csv", "aggregates")}
          >
            导出汇总 CSV
          </a>
          <a className="quiet-button" href={api.exportValidationRunUrl(run.id, "csv", "sessions")}>
            导出逐日 CSV
          </a>
        </div>
      </header>

      <section className="panel">
        <PanelTitle title="策略比较 Ledger" tag="BASE IMPACT" />
        <div className="table-scroll">
          <table className="data-table report-table">
            <thead>
              <tr>
                <th>策略</th>
                <th>完成</th>
                <th>Median IS</th>
                <th>P75</th>
                <th>P95</th>
                <th>约束违规</th>
                <th>自动门槛</th>
              </tr>
            </thead>
            <tbody>
              {report.aggregates.map((item) => (
                <tr key={item.strategy}>
                  <td>
                    <b>{item.strategy.toUpperCase()}</b>
                  </td>
                  <td>
                    {item.completed_sessions}/{item.evaluated_sessions}
                  </td>
                  <td>{Number(item.median_shortfall_bps).toFixed(2)} bps</td>
                  <td>{Number(item.p75_shortfall_bps).toFixed(2)} bps</td>
                  <td>{Number(item.p95_shortfall_bps).toFixed(2)} bps</td>
                  <td>{item.violation_count}</td>
                  <td>
                    <StatusPill
                      status={
                        item.strategy === "immediate"
                          ? "benchmark"
                          : item.eligible_for_validation
                            ? "ready"
                            : "failed"
                      }
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="panel decision-panel">
        <PanelTitle title="人工验证决策" tag="APPEND-ONLY" />
        <label className="decision-note">
          备注（可选，写入审计记录）
          <textarea
            value={note}
            maxLength={1000}
            rows={3}
            placeholder="例如：Base 情景下 TWAP 优于 Immediate，POV 在低波动日完成度不足。"
            onChange={(event) => setNote(event.target.value)}
          />
        </label>
        <div className="decision-actions">
          <button className="alarm-button" onClick={() => decision.mutate("rejected")} disabled={decision.isPending}>
            Reject
          </button>
          <button className="primary-button" onClick={() => decision.mutate("validated")} disabled={decision.isPending}>
            Mark validated
          </button>
        </div>
        {decision.error && <ErrorPanel error={decision.error} />}
        <DecisionHistory decisions={decisions.data ?? []} error={decisions.error} />
      </section>

      <section className="panel replay-panel">
        <PanelTitle title="市场回放带" tag={sample?.date ?? "NO SESSION"} />
        <div className="replay-controls">
          {(["immediate", "twap", "pov", "vwap"] as const).map((name) => (
            <button
              className={strategy === name ? "is-active" : ""}
              onClick={() => setStrategy(name)}
              key={name}
            >
              {name.toUpperCase()}
            </button>
          ))}
        </div>
        {candidate && <MarketReplayBand candidate={candidate} />}
      </section>

      <section className="panel">
        <PanelTitle title="逐日结果" tag="FIRST 12 SESSIONS" />
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>日期</th>
                <th>Arrival</th>
                <th>Interval VWAP</th>
                <th>策略</th>
                <th>完成</th>
                <th>Base IS</th>
              </tr>
            </thead>
            <tbody>
              {report.sessions.slice(0, 12).flatMap((session) =>
                session.candidates
                  .filter((item) => item.strategy === strategy)
                  .map((item) => (
                    <tr key={`${session.date}-${item.strategy}`}>
                      <td>{session.date}</td>
                      <td>{session.arrival_price}</td>
                      <td>{session.interval_vwap}</td>
                      <td>{item.strategy.toUpperCase()}</td>
                      <td>
                        {item.completed
                          ? "YES"
                          : `${item.filled_quantity}/${item.target_quantity}`}
                      </td>
                      <td>
                        {Number(
                          item.scenarios.find((scenario) => scenario.scenario === "base")
                            ?.implementation_shortfall_bps ?? 0,
                        ).toFixed(2)}{" "}
                        bps
                      </td>
                    </tr>
                  )),
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function DecisionHistory({
  decisions,
  error,
}: {
  decisions: ValidationDecision[];
  error: Error | null;
}) {
  if (error) return <ErrorPanel error={error} />;
  if (!decisions.length) return <EmptyState text="尚无人工决策。通过后不可删除，只能追加新记录。" />;
  return (
    <div className="decision-history">
      {decisions.map((item) => (
        <div key={item.sequence}>
          <StatusPill status={item.decision} />
          <b>#{item.sequence}</b>
          <p>{item.note || "（无备注）"}</p>
          <time>{new Date(item.created_at).toLocaleString()}</time>
        </div>
      ))}
    </div>
  );
}

function MarketReplayBand({
  candidate,
}: {
  candidate: ValidationReport["sessions"][number]["candidates"][number];
}) {
  const maxVolume = Math.max(...candidate.replay.map((point) => Number(point.market_volume)), 1);
  const target = Number(candidate.target_quantity) || 1;
  return (
    <div className="market-replay">
      <div className="replay-axis">
        <span>10:00</span>
        <span>10:30</span>
        <span>11:00</span>
        <span>11:30 ET</span>
      </div>
      <div className="replay-buckets">
        {candidate.replay.map((point) => {
          const market = Number(point.market_volume);
          const filled = Number(point.filled_quantity);
          const cumulative = Number(point.cumulative_quantity);
          return (
            <div className="replay-bucket" key={point.bucket}>
              <i style={{ height: `${(market / maxVolume) * 100}%` }} />
              <b style={{ height: `${(filled / Math.max(market, 1)) * 100}%` }} />
              <span style={{ bottom: `${Math.min(100, (cumulative / target) * 100)}%` }} />
              <small>{point.bucket}</small>
            </div>
          );
        })}
      </div>
      <footer>
        <span>
          <i className="legend-volume" />
          IEX volume
        </span>
        <span>
          <i className="legend-fill" />
          simulated fill / participation cap
        </span>
        <span>
          <i className="legend-progress" />
          cumulative completion
        </span>
      </footer>
    </div>
  );
}

// Minimal paper template form kept for library tab.
function TemplateForm({
  initial,
  submitLabel,
  onSubmit,
  error,
}: {
  initial?: ReturnType<typeof currentVersion>;
  submitLabel: string;
  onSubmit: (input: TemplateInput) => void;
  error: Error | null;
}) {
  const { t } = useTranslation();
  const [strategy, setStrategy] = useState<"twap" | "pov">(
    (initial?.strategy as "twap" | "pov" | undefined) ?? "twap",
  );
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    onSubmit({
      name: String(data.get("name") || ""),
      description: String(data.get("description") || ""),
      strategy,
      quantity_unit: String(data.get("quantity_unit") || "base_quantity") as TemplateInput["quantity_unit"],
      duration_seconds: Number(data.get("duration_seconds") || 3600),
      slice_interval_ms: Number(data.get("slice_interval_ms") || 60000),
      max_slippage_bps: Number(data.get("max_slippage_bps") || 20),
      participation_rate: strategy === "pov" ? String(data.get("participation_rate") || "0.1") : undefined,
      max_slice_amount: String(data.get("max_slice_amount") || "") || undefined,
      change_note: String(data.get("change_note") || ""),
      execution_backend: "managed_ioc",
    });
  };
  return (
    <form className="template-form" onSubmit={submit}>
      <label>
        {t("templateName")}
        <input name="name" defaultValue="" required maxLength={120} />
      </label>
      <label>
        {t("description")}
        <input name="description" defaultValue="" />
      </label>
      <div className="form-row">
        <label>
          {t("strategy")}
          <select value={strategy} onChange={(event) => setStrategy(event.target.value as "twap" | "pov")}>
            <option value="twap">TWAP</option>
            <option value="pov">POV</option>
          </select>
        </label>
        <label>
          {t("unit")}
          <select name="quantity_unit" defaultValue={initial?.quantity_unit ?? "base_quantity"}>
            <option value="base_quantity">base_quantity</option>
            <option value="quote_notional">quote_notional</option>
            <option value="contracts">contracts</option>
          </select>
        </label>
      </div>
      <div className="form-row">
        <label>
          {t("duration")}
          <input name="duration_seconds" type="number" defaultValue={initial?.duration_seconds ?? 3600} />
        </label>
        <label>
          {t("interval")}
          <input name="slice_interval_ms" type="number" defaultValue={initial?.slice_interval_ms ?? 60000} />
        </label>
      </div>
      <div className="form-row">
        <label>
          {t("slippage")}
          <input name="max_slippage_bps" type="number" defaultValue={initial?.max_slippage_bps ?? 20} />
        </label>
        {strategy === "pov" && (
          <label>
            {t("participation")}
            <input name="participation_rate" defaultValue={initial?.participation_rate ?? "0.1"} />
          </label>
        )}
      </div>
      <label>
        {t("maxSlice")}
        <input name="max_slice_amount" defaultValue={initial?.max_slice_amount ?? ""} />
      </label>
      <label>
        {t("changeNote")}
        <input name="change_note" defaultValue="" />
      </label>
      {error && <ErrorPanel error={error} />}
      <button className="primary-button">{submitLabel}</button>
    </form>
  );
}

