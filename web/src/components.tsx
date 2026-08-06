import { Link, Outlet } from "@tanstack/react-router";
import { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { ExecutionTask } from "./api";
import { useEventStream } from "./hooks";
import i18n from "./i18n";

const primaryNav = [
  ["/", "controlRoom"], ["/executions", "executions"], ["/strategies", "strategyCenter"],
  ["/analytics", "analytics"], ["/venues", "venues"],
] as const;
const secondaryNav = [
  ["/approvals", "approvals"], ["/accounts", "accountsReconciliation"],
  ["/risk", "riskControls"], ["/hedging", "hedging"],
  ["/system", "systemStatus"], ["/roadmap", "roadmap"],
] as const;

export function AppShell() {
  const { t } = useTranslation();
  const connected = useEventStream();
  const language = i18n.language.startsWith("zh") ? "zh" : "en";
  const nav = (items: typeof primaryNav | typeof secondaryNav) => items.map(([to, label]) => (
    <Link key={to} to={to} activeOptions={{ exact: to === "/" }}>{t(label)}</Link>
  ));
  return <div className="app-shell">
    <aside className="keel-nav">
      <div className="brand-lockup"><span className="brand-mark" aria-hidden="true"><i/><i/><i/></span><div><strong>{t("brand")}</strong><small>BALLAST / OPS</small></div></div>
      <p className="nav-subtitle">{t("subtitle")}</p>
      <nav aria-label={t("primaryNavigation")}>{nav(primaryNav)}</nav>
      <div className="nav-divider" />
      <nav aria-label={t("secondaryNavigation")}>{nav(secondaryNav)}</nav>
      <div className="nav-foot">
        <span className={`link-state ${connected ? "is-live" : ""}`}><i/>{connected ? t("live") : t("disconnected")}</span>
        <button className="language-switch" onClick={() => { const next = language === "zh" ? "en" : "zh"; localStorage.setItem("ballast-language", next); void i18n.changeLanguage(next); }}>{language === "zh" ? "EN" : "中文"}</button>
        <span className="mode-stamp">PAPER / {t("paperMode")}</span>
      </div>
    </aside>
    <main className="workspace"><Outlet /></main>
    <nav className="mobile-nav" aria-label={t("primaryNavigation")}>
      {primaryNav.slice(0, 5).map(([to, label]) => <Link key={to} to={to} activeOptions={{ exact: to === "/" }}>{t(label)}</Link>)}
    </nav>
  </div>;
}

export function PageHeader({ eyebrow, title, action, description }: { eyebrow: string; title: string; action?: ReactNode; description?: string }) {
  return <header className="page-header"><div><span className="eyebrow">{eyebrow}</span><h1>{title}</h1>{description && <p>{description}</p>}</div>{action}</header>;
}

export function PanelTitle({ title, tag }: { title: string; tag?: string }) {
  return <header className="panel-title"><h2>{title}</h2>{tag && <span>{tag}</span>}</header>;
}

export function StabilityLine({ residual, label }: { residual: number; label: string }) {
  const normalized = Math.max(-1, Math.min(1, residual));
  return <div className="stability-wrap" aria-label={`${label}: ${Math.abs(normalized * 100).toFixed(1)}%`}>
    <div className="stability-label"><span>{label}</span><b>{Math.abs(normalized * 100).toFixed(1)}%</b></div>
    <div className="stability-line"><i className="zero-pin"/><span className="residual-arm" style={{ width: `${Math.abs(normalized) * 46}%`, transform: normalized < 0 ? "translateX(-100%) rotate(-1.5deg)" : "rotate(1.5deg)" }}/><b className="residual-marker" style={{ left: `${50 + normalized * 46}%` }}/></div>
    <div className="axis-notes"><span>−</span><span>0 / KEEL</span><span>+</span></div>
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
  return <div className="error-panel" role="alert"><strong>{t("requestFailed")}</strong><code>{error.message}</code><p>{t("retryHint")}</p></div>;
}

export function EmptyState({ text }: { text: string }) { return <div className="empty-state"><i/><p>{text}</p></div>; }

export function TaskProgress({ task }: { task: ExecutionTask }) {
  const target = Number(task.requested_amount); const executed = Number(task.executed_amount);
  const percent = target > 0 ? Math.min(100, executed / target * 100) : 0;
  return <div className="task-progress"><span style={{ width: `${percent}%` }}/><b>{percent.toFixed(1)}%</b></div>;
}

export function RelativeTime({ value }: { value: string }) {
  const { t } = useTranslation(); const delta = new Date(value).getTime() - Date.now();
  if (delta <= 0) return <span className="alarm-text">{t("dueNow")}</span>;
  const seconds = Math.ceil(delta / 1000); return <span>{seconds < 60 ? `${seconds}s` : `${Math.ceil(seconds / 60)}m`}</span>;
}
