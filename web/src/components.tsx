import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import type { TFunction } from "i18next";
import { ReactNode, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import type { ExecutionTask, StrategyTemplate } from "./api";
import { useAuth } from "./auth";
import type { AuthConfig } from "./auth/config";
import { EventStreamStatusContext, useEventStream } from "./hooks";
import i18n from "./i18n";

export function currentVersion(template: StrategyTemplate) {
  return template.versions.find((version) => version.version === template.current_version) ?? template.versions[0];
}

type NavItem = readonly [string, string];

const primaryNav: readonly NavItem[] = [
  ["/", "controlRoom"],
  ["/executions", "executions"],
  ["/strategies", "strategyCenter"],
  ["/analytics", "analytics"],
  ["/venues", "venues"],
];

const utilityNav: readonly NavItem[] = [
  ["/approvals", "approvals"],
  ["/accounts", "accountsReconciliation"],
  ["/risk", "riskControls"],
  ["/hedging", "hedging"],
  ["/system", "systemStatus"],
  ["/roadmap", "roadmap"],
];

function NavLinks({ items, className }: { items: readonly NavItem[]; className?: string }) {
  const { t } = useTranslation();
  return <div className={className}>
    {items.map(([to, translation]) => <Link key={to} to={to} activeOptions={{ exact: to === "/" }}>
      <span>{t(translation)}</span>
    </Link>)}
  </div>;
}

export function BallastLogo({ compact = false }: { compact?: boolean }) {
  return <span className={`ballast-logo ${compact ? "is-compact" : ""}`}>
    <svg className="ballast-symbol" viewBox="0 0 40 40" aria-hidden="true">
      <path className="symbol-waterline" d="M3 17.5H37" />
      <path className="symbol-load" d="M5 10H15V15H5ZM25 10H35V15H25Z" />
      <path className="symbol-keel" d="M18.5 5H21.5V29H18.5Z" />
      <path className="symbol-ballast" d="M13 27H27L23.5 35H16.5Z" />
    </svg>
    {!compact && <span className="ballast-wordmark"><strong>BALLAST</strong><small>压舱石 · EXECUTION CONTROL</small></span>}
  </span>;
}

export function AppShell({ authConfig }: { authConfig: AuthConfig }) {
  const { t } = useTranslation();
  const auth = useAuth();
  const connected = useEventStream(authConfig);
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const navigation = useRef<HTMLElement>(null);
  const language = i18n.language.startsWith("zh") ? "zh" : "en";
  const switchLanguage = () => {
    const next = language === "zh" ? "en" : "zh";
    localStorage.setItem("ballast-language", next);
    void i18n.changeLanguage(next);
  };

  useEffect(() => {
    const active = navigation.current?.querySelector<HTMLElement>('a[data-status="active"]');
    active?.scrollIntoView({ block: "nearest", inline: "center" });
  }, [pathname]);

  return <EventStreamStatusContext.Provider value={connected}><div className="app-shell">
    <a className="skip-link" href="#main-content">{t("skipToContent")}</a>
    <header className="command-header">
      <div className="command-primary">
        <Link to="/" className="brand-lockup" aria-label="Ballast 压舱石"><BallastLogo /></Link>
        <div className="environment-chip"><i aria-hidden="true"/><b>PAPER</b><span>{t("paperBoundaryShort")}</span></div>
        <div className="command-status">
          <span className={`link-state ${connected ? "is-live" : ""}`}><i/>{connected ? t("eventStreamOnline") : t("eventStreamOffline")}</span>
          {auth.status === "authenticated" ? <button className="session-button" onClick={() => void auth.logout()} title={auth.user.profile.sub}>Sign out</button> : null}
          <button className="language-switch" onClick={switchLanguage}>{language === "zh" ? "EN" : "中"}</button>
        </div>
      </div>
      <nav ref={navigation} className="top-navigation" aria-label={t("primaryNavigation")}>
        <NavLinks items={primaryNav} className="top-nav-primary" />
        <NavLinks items={utilityNav} className="top-nav-utility" />
      </nav>
    </header>
    <section className="workbench">
      <main className="workspace" id="main-content" tabIndex={-1}><Outlet /></main>
    </section>
  </div></EventStreamStatusContext.Provider>;
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

export function statusText(status: string, t: TFunction) {
  return t(`status_${status}`, { defaultValue: status.replaceAll("_", " ") });
}

export function StatusPill({ status }: { status: string }) {
  const { t } = useTranslation();
  return <span className={`status-pill status-${status}`}><i/>{statusText(status, t)}</span>;
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
