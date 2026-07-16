import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Ban,
  FileCheck2,
  FileClock,
  FolderCheck,
  FolderClock,
  History,
  Layers3,
  ShieldCheck,
  Trash2,
} from "lucide-react";
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
        icon: FileClock,
        label: t("recentFiles"),
        value: summary?.recent_files,
        detail: unavailable ? t("dataUnavailable") : t("currentItems"),
      },
      {
        key: "quickAccess",
        icon: Layers3,
        label: t("quickAccess"),
        value: summary?.quick_access,
        detail: unavailable ? t("dataUnavailable") : t("currentTotal"),
      },
      {
        key: "frequentFolders",
        icon: FolderClock,
        label: t("frequentFolders"),
        value: summary?.frequent_folders,
        detail: unavailable ? t("dataUnavailable") : t("currentItems"),
      },
      {
        key: "blacklistRules",
        icon: Ban,
        label: t("blacklistRules"),
        value: summary?.blacklist_rules,
        detail:
          unavailable || summary?.blacklist_rules == null
            ? t("dataUnavailable")
            : t("enabledRules"),
      },
      {
        key: "toClean",
        icon: Trash2,
        label: t("toClean"),
        value: summary?.to_clean,
        detail:
          unavailable ||
          summary?.to_clean_recent == null ||
          summary.to_clean_frequent == null
            ? t("dataUnavailable")
            : t("itemTypeSplit", {
                files: summary.to_clean_recent,
                folders: summary.to_clean_frequent,
              }),
      },
      {
        key: "whitelistRules",
        icon: ShieldCheck,
        label: t("whitelistRules"),
        value: summary?.whitelist_rules,
        detail:
          unavailable || summary?.protected_items == null
            ? t("dataUnavailable")
            : t("protectedItems", { count: summary.protected_items }),
      },
      {
        key: "cleanedFiles",
        icon: FileCheck2,
        label: t("cleanedFiles"),
        value: summary?.cleaned_files,
        detail:
          unavailable || summary?.cleaned_files == null
            ? t("dataUnavailable")
            : t("retainedHistory"),
      },
      {
        key: "cleanedTotal",
        icon: History,
        label: t("totalCleaned"),
        value: summary?.cleaned_total,
        detail:
          unavailable || summary?.cleaned_total == null
            ? t("dataUnavailable")
            : t("retainedHistory"),
      },
      {
        key: "cleanedFolders",
        icon: FolderCheck,
        label: t("cleanedFolders"),
        value: summary?.cleaned_folders,
        detail:
          unavailable || summary?.cleaned_folders == null
            ? t("dataUnavailable")
            : t("retainedHistory"),
      },
    ],
    [summary, t, unavailable],
  );

  return (
    <main className="grid min-h-screen place-items-center bg-background p-5 text-foreground">
      <div
        aria-busy={summary === null && !unavailable}
        className="grid w-full max-w-lg grid-cols-3 gap-3"
      >
        {tiles.map(({ detail, icon: Icon, key, label, value }) => (
          <section
            className="flex h-36 min-w-0 flex-col items-center justify-center gap-2 rounded-md border bg-card px-3 text-center text-card-foreground"
            key={key}
          >
            <Icon className="size-5 shrink-0 text-muted-foreground" />
            <strong className="text-3xl font-semibold tabular-nums">
              {value ?? "--"}
            </strong>
            <div className="min-w-0">
              <h2 className="line-clamp-2 text-sm font-medium" title={label}>
                {label}
              </h2>
              <p
                className="mt-0.5 line-clamp-2 break-words text-xs text-muted-foreground"
                title={detail}
              >
                {detail}
              </p>
            </div>
          </section>
        ))}
      </div>
      <p aria-live="polite" className="sr-only">
        {unavailable ? t("gridSummaryUnavailable") : ""}
      </p>
    </main>
  );
}
