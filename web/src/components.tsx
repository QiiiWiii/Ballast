import { Link, Outlet } from "@tanstack/react-router";
import { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { ExecutionTask } from "./api";
import { useEventStream } from "./hooks";
import i18n from "./i18n";

type NavItem = readonly [string, string, string];

const executionNav: readonly NavItem[] = [
  ["/", "controlRoom", "CO"],
  ["/executions", "executions", "EX"],
  ["/strategies", "strategyCenter", "ST"],
  ["/approvals", "approvals", "AP"],
];

const intelligenceNav: readonly NavItem[] = [
  ["/analytics", "analytics", "AN"],
  ["/venues", "venues", "VN"],
  ["/accounts", "accountsReconciliation", "AC"],
];

const safeguardsNav: readonly NavItem[] = [
  ["/risk", "riskControls", "RK"],
  ["/hedging", "hedging", "HG"],
  ["/system", "systemStatus", "SY"],
  ["/roadmap", "roadmap", "RM"],
];

function NavGroup({ label, items }: { label: string; items: readonly NavItem[] }) {
  const { t } = useTranslation();
  return <div className="nav-group">
    <span className="nav-group-label">{label}</span>
    <nav aria-label={label}>
      {items.map(([to, translation, code]) => <Link key={to} to={to} activeOptions={{ exact: to === "/" }}>
        <span className="nav-code">{code}</span><span>{t(translation)}</span><i aria-hidden="true" />
      </Link>)}
    </nav>
  </div>;
}

export function AppShell() {
  const { t } = useTranslation();
  const connected = useEventStream();
  const language = i18n.language.startsWith("zh") ? "zh" : "en";
  const switchLanguage = () => {
    const next = language === "zh" ? "en" : "zh";
    localStorage.setItem("ballast-language", next);
    void i18n.changeLanguage(next);
  };

  return <div className="app-shell">
    <aside className="keel-nav">
      <div className="brand-lockup">
        <span className="brand-mark" aria-hidden="true"><i/><i/><i/></span>
        <div><strong>{t("brand")}</strong><small>EXECUTION CONTROL</small></div>
      </div>
      <div className="environment-card">
        <span>{t("environment")}</span><b>PAPER</b><small>{t("paperBoundaryShort")}</small>
      </div>
      <NavGroup label={t("navExecution")} items={executionNav} />
      <NavGroup label={t("navIntelligence")} items={intelligenceNav} />
      <NavGroup label={t("navSafeguards")} items={safeguardsNav} />
      <div className="nav-foot">
        <span className={`link-state ${connected ? "is-live" : ""}`}><i/>{connected ? t("eventStreamOnline") : t("eventStreamOffline")}</span>
        <button className="language-switch" onClick={switchLanguage}>{language === "zh" ? "ENGLISH" : "中文界面"}</button>
      </div>
    </aside>

    <section className="workbench">
      <header className="system-bar">
        <div className="system-breadcrumb"><b>BALLAST</b><span>/</span><span>{t("operationsWorkspace")}</span></div>
        <div className="system-indicators">
          <span><i className={connected ? "signal-ok" : "signal-alarm"}/>{t("eventBus")}</span>
          <span><i className="signal-paper"/>{t("executionMode")}: PAPER</span>
          <span className="session-id">LOCAL / OPS</span>
        </div>
      </header>
      <main className="workspace"><Outlet /></main>
    </section>

    <nav className="mobile-nav" aria-label={t("primaryNavigation")}>
      {executionNav.slice(0, 3).concat(intelligenceNav.slice(0, 2)).map(([to, label, code]) =>
        <Link key={to} to={to} activeOptions={{ exact: to === "/" }}><b>{code}</b><span>{t(label)}</span></Link>)}
    </nav>
  </div>;
}

export function PageHeader({ eyebrow, title, action, description }: { eyebrow: string; title: string; action?: ReactNode; description?: string }) {
  return <header className="page-header">
    <div className="page-heading"><span className="eyebrow">{eyebrow}</span><h1>{title}</h1>{description && <p>{description}</p>}</div>
    {action && <div className="page-actions">{action}</div>}
  </header>;
}

export function PanelTitle({ title, tag }: { title: string; tag?: string }) {
  return <header className="panel-title"><div><i aria-hidden="true"/><h2>{title}</h2></div>{tag && <span>{tag}</span>}</header>;
}

export function StabilityLine({ residual, label }: { residual: number; label: string }) {
  const normalized = Math.max(-1, Math.min(1, residual));
  return <div className="stability-wrap" aria-label={`${label}: ${Math.abs(normalized * 100).toFixed(1)}%`}>
    <div className="stability-heading"><div><span>KEEL / RESIDUAL AXIS</span><b>{label}</b></div><strong>{Math.abs(normalized * 100).toFixed(1)}<small>%</small></strong></div>
    <div className="stability-line"><i className="zero-pin"/><span className="residual-arm" style={{ width: `${Math.abs(normalized) * 46}%`, transform: normalized < 0 ? "translateX(-100%) rotate(-1.5deg)" : "rotate(1.5deg)" }}/><b className="residual-marker" style={{ left: `${50 + normalized * 46}%` }}/></div>
    <div className="axis-notes"><span>OVERSELL</span><span>0 / BALANCED</span><span>REMAINDER</span></div>
  </div>;
}

export function Metric({ label, value, note, tone = "default" }: { label: string; value: string; note?: string; tone?: "default" | "alarm" }) {
  return <article className={`metric metric-${tone}`}><span>{label}</span><strong>{value}</strong>{note && <small>{note}</small>}</article>;
}

export function StatusPill({ status }: { status: string }) {
  return <span className={`status-pill status-${status}`}><i/>{status.replaceAll("_", " ")}</span>;
}

export function ErrorPanel({ error }: { error: Error }) {
  const { t } = useTranslation();
  return <div className="error-panel" role="alert"><span>!</span><div><strong>{t("requestFailed")}</strong><code>{error.message}</code><p>{t("retryHint")}</p></div></div>;
}

export function EmptyState({ text }: { text: string }) { return <div className="empty-state"><i/><p>{text}</p></div>; }

export function TaskProgress({ task }: { task: ExecutionTask }) {
  const target = Number(task.requested_amount);
  const executed = Number(task.executed_amount);
  const percent = target > 0 ? Math.min(100, executed / target * 100) : 0;
  return <div className="task-progress" aria-label={`${percent.toFixed(1)}%`}><span style={{ width: `${percent}%` }}/><b>{percent.toFixed(1)}%</b></div>;
}

export function RelativeTime({ value }: { value: string }) {
  const { t } = useTranslation();
  const delta = new Date(value).getTime() - Date.now();
  if (delta <= 0) return <span className="alarm-text">{t("dueNow")}</span>;
  const seconds = Math.ceil(delta / 1000);
  return <span>{seconds < 60 ? `${seconds}s` : `${Math.ceil(seconds / 60)}m`}</span>;
}
