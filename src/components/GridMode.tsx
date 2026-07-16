import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Skeleton } from "@/components/ui/skeleton";
import { TitleBar } from "@/components/TitleBar";
import { invokeCommand } from "@/lib/commands";
import { useI18n } from "@/lib/i18n";

const QUICK_ACCESS_CHANGED_EVENT = "quick-access-changed";
const AUTO_CLEAN_FINISHED_EVENT = "auto-clean-finished";

interface GridSummary {
  recent_files: number;
  quick_access: number;
  frequent_folders: number;
  blacklist_rules: number | null;
  to_clean: number | null;
  to_clean_recent: number | null;
  to_clean_frequent: number | null;
  whitelist_rules: number | null;
  protected_items: number | null;
  cleaned_files: number | null;
  cleaned_total: number | null;
  cleaned_folders: number | null;
}

export function GridMode() {
  const { t } = useI18n();
  const [summary, setSummary] = useState<GridSummary | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const loading = summary === null && !unavailable;

  const loadSummary = useCallback(async () => {
    try {
      setSummary(await invokeCommand<GridSummary>("get_grid_summary"));
      setUnavailable(false);
    } catch {
      setUnavailable(true);
    }
  }, []);

  useEffect(() => {
    void loadSummary();
    const listeners = [QUICK_ACCESS_CHANGED_EVENT, AUTO_CLEAN_FINISHED_EVENT].map(
      (event) => listen(event, () => void loadSummary()),
    );
    return () => {
      listeners.forEach((listener) => void listener.then((unlisten) => unlisten()));
    };
  }, [loadSummary]);

  const tiles = useMemo(
    () => [
      {
        key: "recentFiles",
        label: t("recentFiles"),
        value: summary?.recent_files,
      },
      {
        key: "quickAccess",
        label: t("quickAccess"),
        value: summary?.quick_access,
      },
      {
        key: "frequentFolders",
        label: t("frequentFolders"),
        value: summary?.frequent_folders,
      },
      {
        key: "blacklistRules",
        label: t("blacklistRules"),
        value: summary?.blacklist_rules,
      },
      {
        key: "toClean",
        label: t("toClean"),
        value: summary?.to_clean,
      },
      {
        key: "whitelistRules",
        label: t("whitelistRules"),
        value: summary?.whitelist_rules,
      },
      {
        key: "cleanedFiles",
        label: t("cleanedFiles"),
        value: summary?.cleaned_files,
      },
      {
        key: "cleanedTotal",
        label: t("totalCleaned"),
        value: summary?.cleaned_total,
      },
      {
        key: "cleanedFolders",
        label: t("cleanedFolders"),
        value: summary?.cleaned_folders,
      },
    ],
    [summary, t],
  );

  return (
    <main className="flex h-screen flex-col bg-border text-foreground">
      <TitleBar
        closeLabel={t("closeWindow")}
        dashboardLabel={t("dashboard")}
        maximizeLabel={t("maximizeWindow")}
        minimizeLabel={t("minimizeWindow")}
      />
      <div
        aria-busy={loading}
        className="grid min-h-0 flex-1 grid-cols-3 grid-rows-3 gap-px"
      >
        {tiles.map(({ key, label, value }) => (
          <section
            className="group relative flex min-w-0 flex-col items-center justify-center bg-card px-3 py-4 text-center text-card-foreground transition-[background-color,box-shadow] duration-200 hover:z-10 hover:bg-accent hover:shadow-lg"
            key={key}
          >
            <h2
              className="mb-2 line-clamp-2 text-xs font-medium text-muted-foreground transition-colors group-hover:text-foreground"
              title={label}
            >
              {label}
            </h2>
            {loading ? (
              <Skeleton className="h-9 w-14" />
            ) : (
              <strong className="min-h-9 text-3xl font-semibold tabular-nums">
                {value ?? "--"}
              </strong>
            )}
          </section>
        ))}
      </div>
      <p aria-live="polite" className="sr-only">
        {unavailable ? t("gridSummaryUnavailable") : ""}
      </p>
    </main>
  );
}
