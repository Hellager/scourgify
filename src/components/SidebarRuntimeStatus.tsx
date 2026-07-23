import { useCallback, useEffect, useMemo, useState } from "react";
import { Activity } from "lucide-react";

import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { AUTO_CLEAN_UPDATED_EVENT } from "@/lib/app-events";
import { invokeCommand } from "@/lib/commands";
import { useI18n, type I18nKey } from "@/lib/i18n";
import { cn } from "@/lib/utils";

type RuntimeState =
  | "healthy"
  | "degraded"
  | "error"
  | "inactive"
  | "running"
  | "paused"
  | "unknown";

interface RuntimeStatusSnapshot {
  overall: RuntimeState;
  database: { state: RuntimeState };
  qa_monitoring: {
    state: RuntimeState;
    active_watchers: number;
    expected_watchers: number;
    failing_watchers: number;
  };
  auto_clean: {
    state: RuntimeState;
    policy: "disabled" | "monitor" | "every_hours" | "daily_at";
  };
  rules: {
    state: RuntimeState;
    total: number;
    enabled: number;
  };
}

interface StatusEntry {
  key: keyof Pick<
    RuntimeStatusSnapshot,
    "database" | "qa_monitoring" | "auto_clean" | "rules"
  >;
  label: I18nKey;
  state: RuntimeState;
}

const REFRESH_INTERVAL_MS = 10_000;

const stateLabelKeys: Record<RuntimeState, I18nKey> = {
  healthy: "runtimeHealthy",
  degraded: "runtimeDegraded",
  error: "runtimeError",
  inactive: "runtimeInactive",
  running: "runtimeRunning",
  paused: "runtimePaused",
  unknown: "runtimeUnknown",
};

const stateDotClasses: Record<RuntimeState, string> = {
  healthy: "bg-emerald-500",
  degraded: "bg-amber-500",
  error: "bg-red-500",
  inactive: "bg-muted-foreground/45",
  running: "animate-pulse bg-sky-500",
  paused: "bg-muted-foreground/45",
  unknown: "bg-muted-foreground/30",
};

export function SidebarRuntimeStatus() {
  const { t } = useI18n();
  const [status, setStatus] = useState<RuntimeStatusSnapshot | null>(null);

  const refresh = useCallback(() => {
    void invokeCommand<RuntimeStatusSnapshot>("get_runtime_status")
      .then(setStatus)
      .catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    refresh();
    const interval = window.setInterval(refresh, REFRESH_INTERVAL_MS);
    const refreshOnFocus = () => refresh();
    window.addEventListener("focus", refreshOnFocus);
    window.addEventListener(AUTO_CLEAN_UPDATED_EVENT, refreshOnFocus);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", refreshOnFocus);
      window.removeEventListener(AUTO_CLEAN_UPDATED_EVENT, refreshOnFocus);
    };
  }, [refresh]);

  const entries = useMemo<StatusEntry[]>(
    () => [
      {
        key: "qa_monitoring",
        label: "runtimeQuickAccess",
        state: status?.qa_monitoring.state ?? "unknown",
      },
      {
        key: "rules",
        label: "runtimeRules",
        state: status?.rules.state ?? "unknown",
      },
      {
        key: "auto_clean",
        label: "autoClean",
        state: status?.auto_clean.state ?? "unknown",
      },
      {
        key: "database",
        label: "runtimeDatabase",
        state: status?.database.state ?? "unknown",
      },
    ],
    [status],
  );
  const overall = status?.overall ?? "unknown";
  const overallLabel = t(stateLabelKeys[overall]);

  return (
    <>
      <SidebarGroup className="group-data-[collapsible=icon]:hidden">
        <SidebarGroupLabel>{t("runtimeStatus")}</SidebarGroupLabel>
        <SidebarGroupContent>
          <div className="grid gap-1 px-2">
            {entries.map((entry) => (
              <StatusBadge
                key={entry.key}
                label={t(entry.label)}
                state={entry.state}
                stateLabel={t(stateLabelKeys[entry.state])}
                withTooltip
              />
            ))}
          </div>
        </SidebarGroupContent>
      </SidebarGroup>

      <SidebarGroup className="hidden group-data-[collapsible=icon]:flex">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              aria-label={t("runtimeStatusSummary", { status: overallLabel })}
              render={<div role="group" tabIndex={0} />}
              tooltip={{
                className: "grid w-72 items-stretch gap-2",
                children: (
                  <div className="grid gap-1.5">
                    <div className="font-medium">{t("runtimeStatus")}</div>
                    {entries.map((entry) => (
                      <StatusBadge
                        inverted
                        key={entry.key}
                        label={t(entry.label)}
                        showStateText
                        state={entry.state}
                        stateLabel={t(stateLabelKeys[entry.state])}
                      />
                    ))}
                  </div>
                ),
              }}
            >
              <Activity />
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarGroup>
    </>
  );
}

function StatusBadge({
  inverted = false,
  label,
  showStateText = false,
  state,
  stateLabel,
  withTooltip = false,
}: {
  inverted?: boolean;
  label: string;
  showStateText?: boolean;
  state: RuntimeState;
  stateLabel: string;
  withTooltip?: boolean;
}) {
  const badge = (
    <div
      className={cn(
        "flex min-w-0 items-center gap-2 rounded-sm py-1 text-xs outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring",
        inverted ? "text-background" : "text-sidebar-foreground",
      )}
      tabIndex={withTooltip ? 0 : undefined}
    >
      <StatusDot state={state} />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {showStateText ? (
        <span
          className={cn(
            "shrink-0",
            inverted ? "text-background/70" : "text-sidebar-foreground/60",
          )}
        >
          {stateLabel}
        </span>
      ) : null}
    </div>
  );

  if (!withTooltip) {
    return badge;
  }

  return (
    <Tooltip>
      <TooltipTrigger render={badge} />
      <TooltipContent align="start" side="right">
        {label}: {stateLabel}
      </TooltipContent>
    </Tooltip>
  );
}

function StatusDot({
  state,
}: {
  state: RuntimeState;
}) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "size-2 shrink-0 rounded-full ring-2 ring-sidebar/80",
        stateDotClasses[state],
      )}
    />
  );
}
