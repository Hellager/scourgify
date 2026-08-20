import {
  lazy,
  Suspense,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  type Column,
  type ColumnDef,
  type ColumnFiltersState,
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getSortedRowModel,
  type SortingState,
  useReactTable,
} from "@tanstack/react-table";
import { useLocation } from "react-router-dom";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  BrushCleaning,
  ChevronLeft,
  ChevronRight,
  FilterX,
  ListFilter,
  LoaderCircle,
  Search,
} from "lucide-react";
import { toast } from "sonner";
import { useAppShell } from "@/components/AppShell";
import { AutoCleanMonitorPulse } from "@/components/AutoCleanMonitorPulse";
import {
  DatabaseRecoveryPanel,
  type DatabaseStatus,
} from "@/components/DatabaseRecoveryPanel";
import { Dots } from "@/components/loading-ui/dots";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { StatusNotice } from "@/components/ui/status-notice";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { PageFrame } from "@/components/ui/page-frame";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const WordCloudPanel = lazy(() =>
  import("@/components/dashboard/WordCloudPanel").then((module) => ({
    default: module.WordCloudPanel,
  })),
);
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  dispatchAppEvent,
  QUICK_ACCESS_CHANGED_EVENT,
  REFRESH_DASHBOARD_EVENT,
  REFRESH_HISTORY_EVENT,
} from "@/lib/app-events";
import { type CommandOutcome, invokeCommand } from "@/lib/commands";
import {
  type AutoCleanPolicy,
  configSchema,
  type ConfigForm,
} from "@/lib/config";
import { type I18nKey, type Language, useI18n } from "@/lib/i18n";

type QaType = "recent" | "frequent";
type DashboardDialog = QaType | "cleaned" | "auto" | null;
type AutoCleanFormMode = AutoCleanPolicy["kind"];

interface QaCounts {
  recent: number;
  frequent: number;
  all: number;
}

interface QaItem {
  path: string;
  name: string;
  last_interaction_at: number | null;
  pinned: boolean | null;
  visible: boolean;
}

interface QaItemMetadata {
  path: string;
  name: string;
  last_interaction_at: number | null;
  pinned: boolean;
}

interface QaDialogState {
  items: QaItem[];
  loading: boolean;
  error: string | null;
}

interface CleanRecord {
  id: number;
  item_path: string;
  item_type: "recent_file" | "frequent_folder";
  source: "manual" | "auto";
  cleaned_at: string;
}

interface CleanRecordPage {
  records: CleanRecord[];
  total: number;
  overall_total: number;
  hidden_total: number;
  page: number;
  page_size: number;
}

interface StatsTrendPoint {
  period: string;
  count: number;
  recent_files: number;
  frequent_folders: number;
}

interface Stats {
  total: number;
  recent_files: number;
  frequent_folders: number;
  retained_total: number;
  daily_trend: StatsTrendPoint[];
  weekly_trend: StatsTrendPoint[];
  rule_hits: Array<{ keyword: string; count: number }>;
}

interface QaBatchResult {
  total: number;
  succeeded: string[];
  failed: unknown[];
  warnings: unknown[];
  skipped_protected: string[];
  history_error: string | null;
}

interface QuickAccessChanged {
  qa_type: QaType;
  revision: number;
}

interface ResourceState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
}

const initialResourceState = <T,>(): ResourceState<T> => ({
  data: null,
  error: null,
  loading: true,
});
const initialDestlistState = (): Record<QaType, ResourceState<number>> => ({
  recent: initialResourceState<number>(),
  frequent: initialResourceState<number>(),
});
const emptyCleanRecords: CleanRecord[] = [];
const HISTORY_PAGE_SIZE = 10;

export function Dashboard() {
  const { language, t } = useI18n();
  const {
    config,
    overviewHealth,
    privacyActive,
    reportOverviewFailure,
    setConfig,
  } = useAppShell();
  const location = useLocation();
  const [counts, setCounts] = useState<ResourceState<QaCounts>>(
    initialResourceState,
  );
  const [destlistCounts, setDestlistCounts] = useState<
    Record<QaType, ResourceState<number>>
  >(initialDestlistState);
  const [stats, setStats] = useState<ResourceState<Stats>>(initialResourceState);
  const [database, setDatabase] = useState<DatabaseStatus | null>(null);
  const [dashboardRevision, setDashboardRevision] = useState(0);
  const [dialog, setDialog] = useState<DashboardDialog>(null);
  const [qaDialogState, setQaDialogState] = useState<
    Record<QaType, QaDialogState>
  >({
    recent: { items: [], loading: false, error: null },
    frequent: { items: [], loading: false, error: null },
  });
  const [history, setHistory] = useState<CleanRecordPage | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [autoMode, setAutoMode] = useState<AutoCleanFormMode>("disabled");
  const [autoHours, setAutoHours] = useState("24");
  const [autoTime, setAutoTime] = useState("09:00");
  const [savingAutoClean, setSavingAutoClean] = useState(false);
  const [smartCleanConfirmOpen, setSmartCleanConfirmOpen] = useState(false);
  const [skipSmartCleanConfirm, setSkipSmartCleanConfirm] = useState(false);
  const [smartCleaning, setSmartCleaning] = useState(false);
  const smartCleanTriggerRef = useRef<HTMLButtonElement | null>(null);
  const countsRequestRef = useRef(0);
  const statsRequestRef = useRef(0);
  const initialPreviewKeyRef = useRef<string | null>(null);

  const loadCounts = useCallback(async () => {
    const requestId = ++countsRequestRef.current;
    setCounts((current) => ({ ...current, loading: true }));
    if (config.visualization_deep_parse) {
      setDestlistCounts((current) => ({
        recent: { ...current.recent, loading: true },
        frequent: { ...current.frequent, loading: true },
      }));
    } else {
      setDestlistCounts({
        recent: { data: null, error: null, loading: false },
        frequent: { data: null, error: null, loading: false },
      });
    }

    try {
      const nextCounts = await invokeCommand<QaCounts>("get_qa_counts", {
        fresh: false,
      });
      if (requestId !== countsRequestRef.current) {
        return;
      }

      if (config.visualization_deep_parse) {
        const [recent, frequent] = await Promise.allSettled([
          invokeCommand<QaItemMetadata[]>("list_qa_item_metadata", {
            qaType: "recent",
          }),
          invokeCommand<QaItemMetadata[]>("list_qa_item_metadata", {
            qaType: "frequent",
          }),
        ]);
        if (requestId !== countsRequestRef.current) {
          return;
        }
        setDestlistCounts((current) => ({
          recent: settledNumberState(current.recent, recent),
          frequent: settledNumberState(current.frequent, frequent),
        }));
      }

      setCounts({ data: nextCounts, error: null, loading: false });
    } catch (error) {
      if (requestId !== countsRequestRef.current) {
        return;
      }
      setCounts((current) => ({
        ...current,
        error: errorMessage(error),
        loading: false,
      }));
      setDestlistCounts((current) => ({
        recent: { ...current.recent, loading: false },
        frequent: { ...current.frequent, loading: false },
      }));
    }
  }, [config.visualization_deep_parse]);

  const loadStats = useCallback(async () => {
    const requestId = ++statsRequestRef.current;
    setStats((current) => ({ ...current, loading: true }));
    try {
      const nextStats = await invokeCommand<Stats>("get_stats", { range: "30d" });
      if (requestId === statsRequestRef.current) {
        setStats({ data: nextStats, error: null, loading: false });
      }
    } catch (error) {
      if (requestId === statsRequestRef.current) {
        setStats((current) => ({
          ...current,
          error: errorMessage(error),
          loading: false,
        }));
      }
    }
  }, []);

  const loadDatabaseStatus = useCallback(async () => {
    try {
      setDatabase(await invokeCommand<DatabaseStatus>("get_database_status"));
    } catch {
      setDatabase(null);
    }
  }, []);

  const loadOverview = useCallback(async () => {
    await Promise.all([loadCounts(), loadStats(), loadDatabaseStatus()]);
  }, [loadCounts, loadDatabaseStatus, loadStats]);

  useEffect(() => {
    const countsError = counts.error;
    const statsError = stats.error;
    reportOverviewFailure("counts", countsError);
    reportOverviewFailure("stats", statsError);
  }, [counts.error, reportOverviewFailure, stats.error]);

  const loadQaItems = useCallback(async (qaType: QaType) => {
    setQaDialogState((current) => ({
      ...current,
      [qaType]: { ...current[qaType], loading: true, error: null },
    }));
    try {
      const visibleItems = await invokeCommand<Omit<QaItem, "visible">[]>("list_qa_items", {
        fresh: false,
        qaType,
      });
      let items: QaItem[] = visibleItems.map((item) => ({ ...item, visible: true }));
      let metadataError: string | null = null;
      if (config.visualization_deep_parse) {
        try {
          const metadata = await invokeCommand<QaItemMetadata[]>("list_qa_item_metadata", {
            qaType,
          });
          const visibleByPath = new Map(
            visibleItems.map((item) => [item.path.toLocaleLowerCase(), item]),
          );
          const metadataPaths = new Set<string>();
          items = metadata.map((item) => {
            const pathKey = item.path.toLocaleLowerCase();
            metadataPaths.add(pathKey);
            const visible = visibleByPath.get(pathKey);
            return {
              path: item.path,
              name: item.name,
              last_interaction_at: item.last_interaction_at,
              pinned: item.pinned,
              visible: Boolean(visible),
            };
          });
          for (const visible of visibleItems) {
            if (!metadataPaths.has(visible.path.toLocaleLowerCase())) {
              items.push({ ...visible, visible: true });
            }
          }
        } catch (error) {
          metadataError = errorMessage(error);
        }
      }
      setQaDialogState((current) => ({
        ...current,
        [qaType]: { items, loading: false, error: metadataError },
      }));
    } catch (error) {
      setQaDialogState((current) => ({
        ...current,
        [qaType]: {
          ...current[qaType],
          loading: false,
          error: errorMessage(error),
        },
      }));
    }
  }, [config.visualization_deep_parse]);

  const loadHistory = useCallback(async (page: number) => {
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      setHistory(
        await invokeCommand<CleanRecordPage>("get_clean_records", {
          query: {
            page,
            page_size: HISTORY_PAGE_SIZE,
            sort_by: "cleaned_at",
            sort_order: "desc",
            search: "",
            item_type: null,
            matched_by_rule: null,
            source: null,
            run_id: null,
            date_range: null,
          },
        }),
      );
    } catch (error) {
      setHistoryError(errorMessage(error));
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  useEffect(() => {
    if (location.pathname === "/") {
      if (
        import.meta.env.DEV &&
        (location.state as { mockOverviewPreview?: string } | null)
          ?.mockOverviewPreview === "initial" &&
        initialPreviewKeyRef.current !== location.key
      ) {
        initialPreviewKeyRef.current = location.key;
        countsRequestRef.current += 1;
        statsRequestRef.current += 1;
        setCounts(initialResourceState());
        setDestlistCounts(initialDestlistState());
        setStats(initialResourceState());
      }
      void loadOverview();
    }
  }, [loadOverview, location.key, location.pathname, location.state]);

  useEffect(() => {
    const refreshDashboard = () => {
      void loadOverview();
      setDashboardRevision((revision) => revision + 1);
    };
    window.addEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
    return () => window.removeEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
  }, [loadOverview]);

  useEffect(() => {
    let timeout: number | undefined;
    const unlisten = listen<QuickAccessChanged>(QUICK_ACCESS_CHANGED_EVENT, () => {
      window.clearTimeout(timeout);
      timeout = window.setTimeout(() => {
        void loadCounts();
        setDashboardRevision((revision) => revision + 1);
      }, 100);
    });
    return () => {
      window.clearTimeout(timeout);
      void unlisten.then((cleanup) => cleanup());
    };
  }, [loadCounts]);

  const openDialog = (next: Exclude<DashboardDialog, null>) => {
    setDialog(next);
    if (next === "recent" || next === "frequent") {
      void loadQaItems(next);
    } else if (next === "cleaned") {
      void loadHistory(1);
    } else {
      const policy = config.auto_clean;
      setAutoMode(policy.kind);
      setAutoHours(
        policy.kind === "every_hours" ? String(policy.hours) : "24",
      );
      setAutoTime(
        policy.kind === "daily_at"
          ? `${String(policy.hour).padStart(2, "0")}:${String(policy.minute).padStart(2, "0")}`
          : "09:00",
      );
    }
  };

  const saveAutoClean = async () => {
    const hours = Number(autoHours);
    const [hour, minute] = autoTime.split(":").map(Number);
    if (
      autoMode === "every_hours" &&
      (!Number.isInteger(hours) || hours < 1 || hours > 168)
    ) {
      toast.error(t("autoCleanInvalidSchedule"));
      return;
    }
    if (
      autoMode === "daily_at" &&
      (!Number.isInteger(hour) ||
        !Number.isInteger(minute) ||
        hour < 0 ||
        hour > 23 ||
        minute < 0 ||
        minute > 59)
    ) {
      toast.error(t("autoCleanInvalidSchedule"));
      return;
    }

    const autoClean: AutoCleanPolicy =
      autoMode === "every_hours"
        ? { kind: "every_hours", hours }
        : autoMode === "daily_at"
          ? { kind: "daily_at", hour, minute }
        : autoMode === "monitor"
          ? { kind: "monitor" }
          : { kind: "disabled" };
    setSavingAutoClean(true);
    try {
      const nextConfig: ConfigForm = { ...config, auto_clean: autoClean };
      const result = await invokeCommand<CommandOutcome<ConfigForm>>("update_config", {
        nextConfig,
      });
      result.warnings.forEach((warning) => toast.warning(warning.message));
      setConfig(configSchema.parse(result.value));
      setDialog(null);
      toast.success(t("settingsSaved"));
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setSavingAutoClean(false);
    }
  };

  const runSmartClean = async () => {
    if (smartCleaning || privacyActive) {
      if (privacyActive) {
        toast.warning(t("privacyWriteDisabled"));
      }
      return;
    }
    setSmartCleaning(true);
    const results: QaBatchResult[] = [];
    const sectionErrors: string[] = [];

    for (const qaType of ["recent", "frequent"] as const) {
      try {
        results.push(
          await invokeCommand<QaBatchResult>("smart_clean", { qaType }),
        );
      } catch (error) {
        sectionErrors.push(errorMessage(error));
      }
    }

    dispatchAppEvent(REFRESH_DASHBOARD_EVENT, { fresh: true });
    dispatchAppEvent(REFRESH_HISTORY_EVENT);

    const summary = results.reduce(
      (current, result) => ({
        total: current.total + result.total,
        succeeded: current.succeeded + result.succeeded.length,
        failed: current.failed + result.failed.length,
        warnings: current.warnings + result.warnings.length,
        historyErrors:
          current.historyErrors + Number(result.history_error !== null),
      }),
      { total: 0, succeeded: 0, failed: 0, warnings: 0, historyErrors: 0 },
    );

    if (results.length === 0 && sectionErrors.length > 0) {
      toast.error(sectionErrors.join(" "));
    } else if (
      summary.failed > 0 ||
      summary.warnings > 0 ||
      summary.historyErrors > 0 ||
      sectionErrors.length > 0
    ) {
      toast.warning(
        t("smartCleanCompletedWithIssues", {
          succeeded: summary.succeeded,
          failed: summary.failed,
          warnings: summary.warnings,
          sectionErrors: sectionErrors.length,
          historyErrors: summary.historyErrors,
        }),
        sectionErrors.length > 0
          ? { description: sectionErrors.join(" ") }
          : undefined,
      );
    } else {
      toast.success(
        t("smartCleanCompleted", {
          succeeded: summary.succeeded,
          total: summary.total,
        }),
      );
    }

    setSmartCleaning(false);
  };

  const requestSmartClean = () => {
    if (privacyActive) {
      toast.warning(t("privacyWriteDisabled"));
      return;
    }
    if (config.smart_clean_confirm) {
      setSkipSmartCleanConfirm(false);
      setSmartCleanConfirmOpen(true);
      return;
    }
    void runSmartClean();
  };

  const confirmSmartClean = async () => {
    setSmartCleanConfirmOpen(false);
    if (skipSmartCleanConfirm) {
      try {
        const result = await invokeCommand<CommandOutcome<ConfigForm>>("update_config", {
          nextConfig: { ...config, smart_clean_confirm: false },
        });
        result.warnings.forEach((warning) => toast.warning(warning.message));
        setConfig(configSchema.parse(result.value));
      } catch (error) {
        toast.error(errorMessage(error));
      }
    }
    await runSmartClean();
  };

  const nextAutoCleanTime = useMemo(
    () => getNextAutoCleanTime(config.auto_clean, config.auto_clean_last_run),
    [config.auto_clean, config.auto_clean_last_run],
  );
  const autoCleanCardValue =
    config.auto_clean.kind === "monitor" ? (
      <AutoCleanMonitorPulse label={t("autoCleanMonitor")} />
    ) : nextAutoCleanTime ? (
      <AutoCleanCountdown
        active={location.pathname === "/"}
        target={nextAutoCleanTime}
      />
    ) : (
      formatAutoCleanPolicy(config.auto_clean, t)
    );
  const draftNextAutoCleanTime = getDraftNextAutoCleanTime(
    autoMode,
    autoHours,
    autoTime,
    config.auto_clean_last_run,
  );
  const draftNextAutoCleanDetail = draftNextAutoCleanTime
    ? `${t("autoCleanNextRun")}: ${draftNextAutoCleanTime.toLocaleString(language)}`
    : null;
  const activeQaType = dialog === "frequent" ? "frequent" : "recent";
  const activeQaState = qaDialogState[activeQaType];
  const countValues = counts.data;

  return (
    <PageFrame as="main" comfortable reserveFabSpace width="wide">
      {database && !database.available ? (
        <DatabaseRecoveryPanel
          onRecovered={loadOverview}
          onStatusChange={setDatabase}
          status={database}
        />
      ) : null}
      <section className="grid min-w-0 grid-cols-[repeat(4,minmax(0,1fr))] gap-4">
        <SummaryCard
          disabled={!overviewHealth.counts}
          onOpen={() => openDialog("recent")}
          title={t("recentFiles")}
          loading={counts.loading && countValues === null}
          value={
            countValues === null
              ? "--"
              : config.visualization_deep_parse
                ? `${countValues.recent}/${destlistCounts.recent.data ?? "--"}`
                : countValues.recent
          }
        />
        <SummaryCard
          disabled={!overviewHealth.counts}
          onOpen={() => openDialog("frequent")}
          title={t("frequentFolders")}
          loading={counts.loading && countValues === null}
          value={
            countValues === null
              ? "--"
              : config.visualization_deep_parse
                ? `${countValues.frequent}/${destlistCounts.frequent.data ?? "--"}`
                : countValues.frequent
          }
        />
        <SummaryCard
          disabled={!overviewHealth.stats}
          onOpen={() => openDialog("cleaned")}
          title={t("cleaned")}
          loading={stats.loading && stats.data === null}
          value={stats.data?.total ?? "--"}
        />
        <SummaryCard
          onOpen={() => openDialog("auto")}
          title={t("autoClean")}
          value={autoCleanCardValue}
        />
      </section>

      {counts.error && database?.available !== false ? (
        <StatusNotice
          actions={
            <Button onClick={() => void loadCounts()} size="sm" variant="outline">
              {t("refreshDashboard")}
            </Button>
          }
          role="status"
          tone={counts.data === null ? "error" : "warning"}
          title={t("overviewCountsUnavailable")}
        >
          {counts.error}
        </StatusNotice>
      ) : null}
      {stats.error && database?.available !== false ? (
        <StatusNotice
          actions={
            <Button onClick={() => void loadStats()} size="sm" variant="outline">
              {t("refreshDashboard")}
            </Button>
          }
          role="status"
          tone={stats.data === null ? "error" : "warning"}
          title={t("overviewStatsUnavailable")}
        >
          {stats.error}
        </StatusNotice>
      ) : null}

      <Suspense
        fallback={
          <div
            aria-busy="true"
            className="grid h-[360px] place-items-center bg-card text-sm text-muted-foreground"
            role="status"
          >
            {t("wordCloudLoading")}
          </div>
        }
      >
        <WordCloudPanel
          blacklistTermsEnabled={config.word_cloud_show_blacklist_terms}
          dataRange={config.visualization_data_range}
          dataSource={config.visualization_data_source}
          deepParseEnabled={config.visualization_deep_parse}
          refreshKey={dashboardRevision}
          t={t}
        />
      </Suspense>

      <QuickAccessDialog
        items={activeQaState.items}
        deepParseEnabled={config.visualization_deep_parse}
        language={language}
        loading={activeQaState.loading}
        error={activeQaState.error}
        onOpenChange={(open) => !open && setDialog(null)}
        open={dialog === "recent" || dialog === "frequent"}
        qaType={activeQaType}
        t={t}
      />
      <CleanedItemsDialog
        error={historyError}
        history={history}
        language={language}
        loading={historyLoading}
        onLoadPage={(page) => void loadHistory(page)}
        onOpenChange={(open) => !open && setDialog(null)}
        open={dialog === "cleaned"}
        t={t}
      />
      <AutoCleanDialog
        hours={autoHours}
        mode={autoMode}
        onHoursChange={setAutoHours}
        onModeChange={setAutoMode}
        onOpenChange={(open) => !open && setDialog(null)}
        onSave={() => void saveAutoClean()}
        onTimeChange={setAutoTime}
        open={dialog === "auto"}
        nextRun={draftNextAutoCleanDetail}
        saving={savingAutoClean}
        t={t}
        time={autoTime}
      />

      <Button
        aria-label={t("smartClean")}
        className="primary-fab size-12 rounded-full"
        disabled={
          smartCleaning ||
          privacyActive ||
          !overviewHealth.counts ||
          !overviewHealth.stats
        }
        onClick={requestSmartClean}
        ref={smartCleanTriggerRef}
        size="icon-lg"
        title={privacyActive ? t("privacyWriteDisabled") : t("smartClean")}
        type="button"
      >
        {smartCleaning ? (
          <LoaderCircle className="size-5 animate-spin" />
        ) : (
          <BrushCleaning className="size-5" />
        )}
      </Button>

      <AlertDialog
        open={smartCleanConfirmOpen}
        onOpenChange={(open) => {
          if (!smartCleaning) {
            setSmartCleanConfirmOpen(open);
            if (!open) {
              setSkipSmartCleanConfirm(false);
            }
          }
        }}
      >
        <AlertDialogContent finalFocus={() => smartCleanTriggerRef.current}>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("smartCleanQuestion")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("smartCleanCurrentRulesDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <label className="flex cursor-pointer items-center gap-2 text-sm">
            <Checkbox
              checked={skipSmartCleanConfirm}
              onCheckedChange={(checked) =>
                setSkipSmartCleanConfirm(Boolean(checked))
              }
            />
            <span>{t("doNotAskAgain")}</span>
          </label>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={smartCleaning || privacyActive}
              onClick={() => void confirmSmartClean()}
              type="button"
            >
              <BrushCleaning />
              {t("smartClean")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageFrame>
  );
}

function SummaryCard({
  detail,
  disabled = false,
  loading = false,
  onOpen,
  title,
  value,
}: {
  detail?: ReactNode;
  disabled?: boolean;
  loading?: boolean;
  onOpen: () => void;
  title: string;
  value: ReactNode;
}) {
  return (
    <button
      aria-label={title}
      aria-busy={loading}
      className="surface-panel group flex min-h-28 min-w-0 flex-col items-center justify-center p-3 text-center transition-[color,background-color,border-color,box-shadow,transform] outline-none hover:border-foreground/20 hover:bg-surface-subtle focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:translate-y-px disabled:pointer-events-none disabled:opacity-50"
      disabled={disabled}
      onClick={onOpen}
      title={title}
      type="button"
    >
      <span className="line-clamp-2 h-10 w-full content-center break-words px-1 text-sm font-medium text-muted-foreground transition-colors group-hover:text-foreground">
        {title}
      </span>
      <span className="data-value mt-1 flex h-9 w-full min-w-0 shrink-0 items-center justify-center overflow-hidden whitespace-nowrap text-xl font-semibold">
        {loading ? (
          <Dots aria-label="Loading" className="h-3 w-7 text-muted-foreground" />
        ) : (
          value
        )}
      </span>
      {detail ? (
        <span
          className="mt-0.5 block w-full truncate text-[11px] text-muted-foreground"
          title={typeof detail === "string" ? detail : undefined}
        >
          {detail}
        </span>
      ) : null}
    </button>
  );
}

function settledNumberState(
  current: ResourceState<number>,
  result: PromiseSettledResult<QaItemMetadata[]>,
): ResourceState<number> {
  if (result.status === "fulfilled") {
    return { data: result.value.length, error: null, loading: false };
  }
  return {
    ...current,
    error: errorMessage(result.reason),
    loading: false,
  };
}

function AutoCleanCountdown({
  active,
  target,
}: {
  active: boolean;
  target: Date;
}) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!active) {
      return;
    }
    setNow(Date.now());
    const interval = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(interval);
  }, [active, target]);

  return <>{formatCountdown(target.getTime() - now)}</>;
}

function quickAccessColumnClassName(columnId: string) {
  switch (columnId) {
    case "visible":
      return "w-28";
    case "last_interaction_at":
      return "w-40";
    case "name":
      return "max-w-56";
    case "path":
      return "max-w-[420px]";
    default:
      return undefined;
  }
}

function QuickAccessDialog({
  deepParseEnabled,
  error,
  items,
  language,
  loading,
  onOpenChange,
  open,
  qaType,
  t,
}: {
  deepParseEnabled: boolean;
  error: string | null;
  items: QaItem[];
  language: Language;
  loading: boolean;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  qaType: QaType;
  t: Translate;
}) {
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [sorting, setSorting] = useState<SortingState>([
    { id: "last_interaction_at", desc: true },
  ]);
  useEffect(() => {
    if (!open) {
      setColumnFilters([]);
      setSorting([{ id: "last_interaction_at", desc: true }]);
    }
  }, [open]);
  const columns = useMemo<ColumnDef<QaItem>[]>(
    () => [
      {
        accessorKey: "name",
        enableSorting: false,
        header: ({ column }) => (
          <QuickAccessKeywordFilterHeader
            clearLabel={t("clearQuickAccessNameFilter")}
            column={column}
            filterLabel={t("filterQuickAccessName")}
            label={t("name")}
          />
        ),
        cell: ({ row }) => (
          <span
            className="block max-w-56 truncate font-medium"
            title={row.original.name}
          >
            {row.original.name}
          </span>
        ),
      },
      {
        accessorKey: "path",
        enableSorting: false,
        header: ({ column }) => (
          <QuickAccessKeywordFilterHeader
            clearLabel={t("clearQuickAccessPathFilter")}
            column={column}
            filterLabel={t("filterQuickAccessPath")}
            label={t("path")}
          />
        ),
        cell: ({ row }) => (
          <span
            className="block max-w-[420px] truncate text-muted-foreground"
            title={row.original.path}
          >
            {row.original.path}
          </span>
        ),
      },
      {
        accessorKey: "last_interaction_at",
        sortUndefined: "last",
        header: ({ column }) => (
          <QuickAccessSortableHeader column={column}>
            {t("lastUsed")}
          </QuickAccessSortableHeader>
        ),
        cell: ({ row }) => (
          <span className="text-muted-foreground">
            {row.original.last_interaction_at === null
              ? "--"
              : new Date(
                  row.original.last_interaction_at,
                ).toLocaleString(language)}
          </span>
        ),
      },
      ...(deepParseEnabled
        ? [
            {
              accessorKey: "visible",
              enableSorting: false,
              filterFn: (row, _columnId, value) =>
                value === "visible" ? row.original.visible : !row.original.visible,
              header: ({ column }) => (
                <QuickAccessEnumFilterHeader
                  column={column}
                  label={t("visible")}
                  options={[
                    { label: t("allVisibility"), value: "all" },
                    { label: t("visibleOnly"), value: "visible" },
                    { label: t("hiddenOnly"), value: "hidden" },
                  ]}
                />
              ),
              cell: ({ row }: { row: { original: QaItem } }) =>
                row.original.visible ? t("yes") : t("no"),
            } satisfies ColumnDef<QaItem>,
          ]
        : []),
    ],
    [deepParseEnabled, language, t],
  );
  const table = useReactTable({
    columns,
    data: items,
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getRowId: (item) => item.path,
    getSortedRowModel: getSortedRowModel(),
    enableMultiSort: false,
    onColumnFiltersChange: setColumnFilters,
    onSortingChange: setSorting,
    state: { columnFilters, sorting },
  });
  const rows = table.getRowModel().rows;

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent
        className="max-h-[80svh] overflow-hidden sm:max-w-4xl"
        showCloseButton={false}
      >
        <DialogHeader className="sr-only">
          <DialogTitle>
            {qaType === "recent" ? t("recentFiles") : t("frequentFolders")}
          </DialogTitle>
          <DialogDescription>{t("currentItems")}</DialogDescription>
        </DialogHeader>
        <div
          aria-busy={loading}
          className="h-[58svh] overflow-auto rounded-md border"
        >
          <Table>
            <TableHeader>
              {table.getHeaderGroups().map((headerGroup) => (
                <TableRow key={headerGroup.id}>
                  {headerGroup.headers.map((header) => (
                    <TableHead
                      className={quickAccessColumnClassName(header.column.id)}
                      key={header.id}
                    >
                      {header.isPlaceholder
                        ? null
                        : flexRender(
                            header.column.columnDef.header,
                            header.getContext(),
                          )}
                    </TableHead>
                  ))}
                </TableRow>
              ))}
            </TableHeader>
            <TableBody>
              {rows.length === 0 ? (
                <TableMessage colSpan={deepParseEnabled ? 4 : 3} destructive={Boolean(error)}>
                  {loading
                    ? t("loadingItems")
                    : error ??
                      (columnFilters.length > 0 ? t("noMatches") : t("noItems"))}
                </TableMessage>
              ) : (
                rows.map((row) => (
                  <TableRow key={row.id}>
                    {row.getVisibleCells().map((cell) => (
                      <TableCell
                        className={quickAccessColumnClassName(cell.column.id)}
                        key={cell.id}
                      >
                        {flexRender(
                          cell.column.columnDef.cell,
                          cell.getContext(),
                        )}
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>
        {error && rows.length > 0 ? (
          <p className="text-xs text-destructive" role="status">
            {error}
          </p>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

interface QuickAccessFilterOption {
  label: string;
  value: string;
}

function QuickAccessSortableHeader({
  children,
  column,
}: {
  children: ReactNode;
  column: Column<QaItem>;
}) {
  const sorted = column.getIsSorted();
  const SortIcon =
    sorted === "asc" ? ArrowUp : sorted === "desc" ? ArrowDown : ArrowUpDown;
  return (
    <button
      className="flex h-8 w-full items-center justify-between gap-1.5 text-left text-sm font-medium"
      onClick={column.getToggleSortingHandler()}
      type="button"
    >
      {children}
      <SortIcon
        className={
          sorted ? "size-4 text-primary" : "size-4 text-muted-foreground"
        }
      />
    </button>
  );
}

function QuickAccessKeywordFilterHeader({
  clearLabel,
  column,
  filterLabel,
  label,
}: {
  clearLabel: string;
  column: Column<QaItem, unknown>;
  filterLabel: string;
  label: string;
}) {
  const value = (column.getFilterValue() as string | undefined) ?? "";
  const active = column.getIsFiltered();
  return (
    <div className="flex items-center justify-between gap-2">
      <span>{label}</span>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              aria-label={filterLabel}
              className={
                active
                  ? "bg-primary/10 text-primary hover:bg-primary/15 hover:text-primary"
                  : "text-muted-foreground"
              }
              size="icon-xs"
              title={filterLabel}
              type="button"
              variant="ghost"
            >
              <Search />
            </Button>
          }
        />
        <DropdownMenuContent align="end" className="w-64">
          <div className="px-2 py-1">
            <Input
              aria-label={filterLabel}
              autoFocus
              onChange={(event) =>
                column.setFilterValue(event.target.value || undefined)
              }
              onKeyDown={(event) => event.stopPropagation()}
              placeholder={filterLabel}
              type="search"
              value={value}
            />
          </div>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            disabled={!active}
            onClick={() => column.setFilterValue(undefined)}
          >
            <FilterX />
            {clearLabel}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function QuickAccessEnumFilterHeader({
  column,
  label,
  options,
}: {
  column: Column<QaItem, unknown>;
  label: string;
  options: QuickAccessFilterOption[];
}) {
  const active = column.getIsFiltered();
  const value = (column.getFilterValue() as string | undefined) ?? "all";
  return (
    <div className="flex items-center justify-between gap-2">
      <span>{label}</span>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              aria-label={label}
              className={
                active
                  ? "bg-primary/10 text-primary hover:bg-primary/15 hover:text-primary"
                  : "text-muted-foreground"
              }
              size="icon-xs"
              title={label}
              type="button"
              variant="ghost"
            >
              <ListFilter />
            </Button>
          }
        />
        <DropdownMenuContent align="end" className="w-52">
          <DropdownMenuRadioGroup
            onValueChange={(nextValue) =>
              column.setFilterValue(nextValue === "all" ? undefined : nextValue)
            }
            value={value}
          >
            {options.map((option) => (
              <DropdownMenuRadioItem key={option.value} value={option.value}>
                {option.label}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function CleanedItemsDialog({
  error,
  history,
  language,
  loading,
  onLoadPage,
  onOpenChange,
  open,
  t,
}: {
  error: string | null;
  history: CleanRecordPage | null;
  language: Language;
  loading: boolean;
  onLoadPage: (page: number) => void;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  t: Translate;
}) {
  const page = history?.page ?? 1;
  const pageCount = Math.max(1, Math.ceil((history?.total ?? 0) / HISTORY_PAGE_SIZE));
  const columns = useMemo<ColumnDef<CleanRecord>[]>(
    () => [
      {
        accessorKey: "item_path",
        header: t("path"),
        cell: ({ row }) => (
          <span
            className="block max-w-[480px] truncate"
            title={row.original.item_path}
          >
            {row.original.item_path}
          </span>
        ),
      },
      {
        accessorKey: "item_type",
        header: t("type"),
        cell: ({ row }) =>
          t(
            row.original.item_type === "recent_file"
              ? "recentFile"
              : "frequentFolder",
          ),
      },
      {
        accessorKey: "source",
        header: t("cleanupSource"),
        cell: ({ row }) =>
          t(row.original.source === "auto" ? "automatic" : "manualTrigger"),
      },
      {
        accessorKey: "cleaned_at",
        header: t("cleanedAt"),
        cell: ({ row }) => (
          <span className="text-muted-foreground">
            {new Date(row.original.cleaned_at).toLocaleString(language)}
          </span>
        ),
      },
    ],
    [language, t],
  );
  const table = useReactTable({
    columns,
    data: history?.records ?? emptyCleanRecords,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (record) => String(record.id),
    manualPagination: true,
    pageCount,
    state: {
      pagination: { pageIndex: page - 1, pageSize: HISTORY_PAGE_SIZE },
    },
  });
  const rows = table.getRowModel().rows;

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-h-[80svh] overflow-hidden sm:w-[calc(100%-4rem)] sm:max-w-5xl">
        <DialogHeader>
          <DialogTitle>{t("cleaned")}</DialogTitle>
        </DialogHeader>
        <div
          aria-busy={loading}
          className="h-[52svh] overflow-auto rounded-md border"
        >
          <Table>
            <TableHeader>
              {table.getHeaderGroups().map((headerGroup) => (
                <TableRow key={headerGroup.id}>
                  {headerGroup.headers.map((header) => (
                    <TableHead
                      className={cleanedColumnClassName(header.column.id)}
                      key={header.id}
                    >
                      {header.isPlaceholder
                        ? null
                        : flexRender(
                            header.column.columnDef.header,
                            header.getContext(),
                          )}
                    </TableHead>
                  ))}
                </TableRow>
              ))}
            </TableHeader>
            <TableBody>
              {rows.length === 0 ? (
                <TableMessage colSpan={4} destructive={Boolean(error)}>
                  {loading ? t("loadingHistory") : error ?? t("noHistory")}
                </TableMessage>
              ) : (
                rows.map((row) => (
                  <TableRow key={row.id}>
                    {row.getVisibleCells().map((cell) => (
                      <TableCell
                        className={cleanedColumnClassName(cell.column.id)}
                        key={cell.id}
                      >
                        {flexRender(
                          cell.column.columnDef.cell,
                          cell.getContext(),
                        )}
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>
        {error && rows.length > 0 ? (
          <p className="text-xs text-destructive" role="status">
            {error}
          </p>
        ) : null}
        <DialogFooter className="items-center sm:justify-between">
          <span className="text-xs text-muted-foreground">
            {t("pageStatus", {
              count: history?.total ?? 0,
              page: history?.total === 0 ? 0 : page,
              pageCount: history?.total === 0 ? 0 : pageCount,
            })}
          </span>
          <div className="flex gap-2">
            <Button
              aria-label={t("previous")}
              disabled={loading || !table.getCanPreviousPage()}
              onClick={() => onLoadPage(page - 1)}
              size="icon-sm"
              title={t("previous")}
              type="button"
              variant="outline"
            >
              <ChevronLeft />
            </Button>
            <Button
              aria-label={t("next")}
              disabled={loading || !table.getCanNextPage()}
              onClick={() => onLoadPage(page + 1)}
              size="icon-sm"
              title={t("next")}
              type="button"
              variant="outline"
            >
              <ChevronRight />
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function cleanedColumnClassName(columnId: string) {
  switch (columnId) {
    case "item_type":
      return "w-40";
    case "source":
      return "w-32";
    case "cleaned_at":
      return "w-44";
    case "item_path":
      return "max-w-[480px]";
    default:
      return undefined;
  }
}

function AutoCleanDialog({
  hours,
  mode,
  nextRun,
  onHoursChange,
  onModeChange,
  onOpenChange,
  onSave,
  onTimeChange,
  open,
  saving,
  t,
  time,
}: {
  hours: string;
  mode: AutoCleanFormMode;
  nextRun: string | null;
  onHoursChange: (hours: string) => void;
  onModeChange: (mode: AutoCleanFormMode) => void;
  onOpenChange: (open: boolean) => void;
  onSave: () => void;
  onTimeChange: (time: string) => void;
  open: boolean;
  saving: boolean;
  t: Translate;
  time: string;
}) {
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("autoClean")}</DialogTitle>
          <DialogDescription>{t("autoCleanMode")}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-4">
          <label className="grid gap-2 text-sm">
            <span className="font-medium">{t("autoCleanMode")}</span>
            <Select
              onValueChange={(value) => onModeChange(value as AutoCleanFormMode)}
              value={mode}
            >
              <SelectTrigger className="w-full">
                <SelectValue>{autoCleanModeLabel(mode, t)}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="disabled">{t("disabled")}</SelectItem>
                <SelectItem value="every_hours">{t("autoCleanEveryHours")}</SelectItem>
                <SelectItem value="daily_at">{t("autoCleanDailyAt")}</SelectItem>
                <SelectItem value="monitor">{t("autoCleanMonitor")}</SelectItem>
              </SelectContent>
            </Select>
          </label>
          {mode === "every_hours" ? (
            <label className="grid gap-2 text-sm">
              <span className="font-medium">{t("autoCleanInterval")}</span>
              <Input
                max={168}
                min={1}
                onChange={(event) => onHoursChange(event.target.value)}
                type="number"
                value={hours}
              />
            </label>
          ) : null}
          {mode === "daily_at" ? (
            <label className="grid gap-2 text-sm">
              <span className="font-medium">{t("autoCleanTime")}</span>
              <Input
                onChange={(event) => onTimeChange(event.target.value)}
                type="time"
                value={time}
              />
            </label>
          ) : null}
          {nextRun ? (
            <div className="rounded-md border bg-muted/50 px-3 py-2 text-sm">
              {nextRun}
            </div>
          ) : null}
        </div>
        <DialogFooter>
          <Button disabled={saving} onClick={() => onOpenChange(false)} type="button" variant="outline">
            {t("cancel")}
          </Button>
          <Button disabled={saving} onClick={onSave} type="button">
            {t("save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function TableMessage({
  children,
  colSpan,
  destructive = false,
}: {
  children: string;
  colSpan: number;
  destructive?: boolean;
}) {
  return (
    <TableRow>
      <TableCell
        className={`h-40 text-center ${destructive ? "text-destructive" : "text-muted-foreground"}`}
        colSpan={colSpan}
      >
        {children}
      </TableCell>
    </TableRow>
  );
}

type Translate = (
  key: I18nKey,
  values?: Record<string, string | number>,
) => string;

function formatAutoCleanPolicy(policy: AutoCleanPolicy, t: Translate) {
  switch (policy.kind) {
    case "disabled":
      return "--";
    case "monitor":
      return t("autoCleanMonitor");
    case "every_hours":
      return `${policy.hours}h`;
    case "daily_at":
      return `${String(policy.hour).padStart(2, "0")}:${String(policy.minute).padStart(2, "0")}`;
  }
}

function autoCleanModeLabel(mode: AutoCleanFormMode, t: Translate) {
  switch (mode) {
    case "disabled":
      return t("disabled");
    case "every_hours":
      return t("autoCleanEveryHours");
    case "daily_at":
      return t("autoCleanDailyAt");
    case "monitor":
      return t("autoCleanMonitor");
  }
}

function getNextAutoCleanTime(
  policy: AutoCleanPolicy,
  lastRun: string | null,
  now = new Date(),
) {
  if (policy.kind === "every_hours") {
    const lastRunTime = lastRun ? new Date(lastRun) : null;
    const anchor =
      lastRunTime && !Number.isNaN(lastRunTime.getTime()) ? lastRunTime : now;
    const scheduled = new Date(anchor.getTime() + policy.hours * 3_600_000);
    return scheduled > now ? scheduled : now;
  }
  if (policy.kind === "daily_at") {
    const scheduled = new Date(now);
    scheduled.setHours(policy.hour, policy.minute, 0, 0);
    if (scheduled <= now) {
      scheduled.setDate(scheduled.getDate() + 1);
    }
    return scheduled;
  }
  return null;
}

function getDraftNextAutoCleanTime(
  mode: AutoCleanFormMode,
  hoursValue: string,
  timeValue: string,
  lastRun: string | null,
) {
  if (mode === "every_hours") {
    const hours = Number(hoursValue);
    return Number.isInteger(hours) && hours >= 1 && hours <= 168
      ? getNextAutoCleanTime({ kind: "every_hours", hours }, lastRun)
      : null;
  }
  if (mode === "daily_at") {
    const [hour, minute] = timeValue.split(":").map(Number);
    return Number.isInteger(hour) &&
      Number.isInteger(minute) &&
      hour >= 0 &&
      hour <= 23 &&
      minute >= 0 &&
      minute <= 59
      ? getNextAutoCleanTime({ kind: "daily_at", hour, minute }, lastRun)
      : null;
  }
  return null;
}

function formatAutoCleanDateTime(date: Date, language: Language) {
  return date.toLocaleString(language, {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatCountdown(milliseconds: number) {
  const totalSeconds = Math.max(0, Math.ceil(milliseconds / 1_000));
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  return [hours, minutes, seconds]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
