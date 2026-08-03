import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  type ColumnDef,
  flexRender,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table";
import { useLocation } from "react-router-dom";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
} from "recharts";
import {
  BrushCleaning,
  ChevronLeft,
  ChevronRight,
  LoaderCircle,
} from "lucide-react";
import { toast } from "sonner";
import { useAppShell } from "@/components/AppShell";
import { AutoCleanMonitorPulse } from "@/components/AutoCleanMonitorPulse";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
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
import { invokeCommand } from "@/lib/commands";
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

interface QaActivityTrendPoint {
  date: string;
  recent_files: number;
  frequent_folders: number;
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

const emptyCounts: QaCounts = { recent: 0, frequent: 0, all: 0 };
const emptyCleanRecords: CleanRecord[] = [];
const HISTORY_PAGE_SIZE = 10;

export function Dashboard() {
  const { language, t } = useI18n();
  const { config, setConfig } = useAppShell();
  const location = useLocation();
  const [counts, setCounts] = useState<QaCounts>(emptyCounts);
  const [stats, setStats] = useState<Stats | null>(null);
  const [activityTrend, setActivityTrend] = useState<QaActivityTrendPoint[]>([]);
  const [activityTrendLoading, setActivityTrendLoading] = useState(true);
  const [activityTrendError, setActivityTrendError] = useState<string | null>(null);
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

  const loadCounts = useCallback(async () => {
    setCounts(await invokeCommand<QaCounts>("get_qa_counts", { fresh: false }));
  }, []);

  const loadStats = useCallback(async () => {
    setStats(await invokeCommand<Stats>("get_stats", { range: "30d" }));
  }, []);

  const loadActivityTrend = useCallback(async () => {
    setActivityTrendLoading(true);
    setActivityTrendError(null);
    try {
      setActivityTrend(
        await invokeCommand<QaActivityTrendPoint[]>("get_qa_activity_trend", {
          range: "30d",
        }),
      );
    } catch (error) {
      setActivityTrend([]);
      setActivityTrendError(errorMessage(error));
    } finally {
      setActivityTrendLoading(false);
    }
  }, []);

  const loadOverview = useCallback(async () => {
    const [countsResult, statsResult] = await Promise.allSettled([
      loadCounts(),
      loadStats(),
    ]);
    if (countsResult.status === "rejected") {
      setCounts(emptyCounts);
    }
    if (statsResult.status === "rejected") {
      setStats(null);
    }
  }, [loadCounts, loadStats]);

  const loadQaItems = useCallback(async (qaType: QaType) => {
    setQaDialogState((current) => ({
      ...current,
      [qaType]: { ...current[qaType], loading: true, error: null },
    }));
    try {
      const items = await invokeCommand<QaItem[]>("list_qa_items", {
        fresh: false,
        qaType,
      });
      setQaDialogState((current) => ({
        ...current,
        [qaType]: { items, loading: false, error: null },
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
  }, []);

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
      void loadOverview();
      void loadActivityTrend();
    }
  }, [loadActivityTrend, loadOverview, location.pathname]);

  useEffect(() => {
    const refreshDashboard = () => {
      void loadOverview();
      void loadActivityTrend();
    };
    window.addEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
    return () => window.removeEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
  }, [loadActivityTrend, loadOverview]);

  useEffect(() => {
    let timeout: number | undefined;
    const unlisten = listen<QuickAccessChanged>(QUICK_ACCESS_CHANGED_EVENT, () => {
      window.clearTimeout(timeout);
      timeout = window.setTimeout(() => {
        void loadCounts().catch(() => setCounts(emptyCounts));
        void loadActivityTrend();
      }, 100);
    });
    return () => {
      window.clearTimeout(timeout);
      void unlisten.then((cleanup) => cleanup());
    };
  }, [loadActivityTrend, loadCounts]);

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
      const saved = await invokeCommand<ConfigForm>("update_config", {
        nextConfig,
      });
      setConfig(configSchema.parse(saved));
      setDialog(null);
      toast.success(t("settingsSaved"));
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setSavingAutoClean(false);
    }
  };

  const runSmartClean = async () => {
    if (smartCleaning) {
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
        const saved = await invokeCommand<ConfigForm>("update_config", {
          nextConfig: { ...config, smart_clean_confirm: false },
        });
        setConfig(configSchema.parse(saved));
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

  return (
    <main className="mx-auto grid w-full max-w-7xl gap-6 p-6">
      <section className="grid grid-cols-4 gap-4">
        <SummaryCard
          onOpen={() => openDialog("recent")}
          title={t("recentFiles")}
          value={counts.recent}
        />
        <SummaryCard
          onOpen={() => openDialog("frequent")}
          title={t("frequentFolders")}
          value={counts.frequent}
        />
        <SummaryCard
          onOpen={() => openDialog("cleaned")}
          title={t("cleaned")}
          value={stats?.total ?? "--"}
        />
        <SummaryCard
          onOpen={() => openDialog("auto")}
          title={t("autoClean")}
          value={autoCleanCardValue}
        />
      </section>

      <QuickAccessActivityChart
        data={activityTrend}
        error={activityTrendError}
        language={language}
        loading={activityTrendLoading}
        t={t}
      />

      <QuickAccessDialog
        items={activeQaState.items}
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
        className="fixed bottom-6 right-6 z-20 size-12 rounded-full shadow-lg"
        disabled={smartCleaning}
        onClick={requestSmartClean}
        ref={smartCleanTriggerRef}
        size="icon-lg"
        title={t("smartClean")}
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
              onClick={() => void confirmSmartClean()}
              type="button"
            >
              <BrushCleaning />
              {t("smartClean")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </main>
  );
}

function SummaryCard({
  detail,
  onOpen,
  title,
  value,
}: {
  detail?: ReactNode;
  onOpen: () => void;
  title: string;
  value: ReactNode;
}) {
  return (
    <div className="flex h-24 min-w-0 flex-col items-center justify-center rounded-md border bg-card p-3 text-center text-card-foreground">
      <button
        className="rounded-sm text-sm font-medium text-muted-foreground underline-offset-4 hover:text-foreground hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        onClick={onOpen}
        type="button"
      >
        {title}
      </button>
      <div className="mt-1 max-w-full text-xl font-semibold tabular-nums">
        {value}
      </div>
      {detail ? (
        <div
          className="mt-0.5 w-full truncate text-[11px] text-muted-foreground"
          title={typeof detail === "string" ? detail : undefined}
        >
          {detail}
        </div>
      ) : null}
    </div>
  );
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

function QuickAccessActivityChart({
  data,
  error,
  language,
  loading,
  t,
}: {
  data: QaActivityTrendPoint[];
  error: string | null;
  language: Language;
  loading: boolean;
  t: Translate;
}) {
  return (
    <section className="min-w-0 bg-card text-card-foreground">
      {loading ? (
        <ChartMessage>{t("loadingItems")}</ChartMessage>
      ) : error ? (
        <ChartMessage destructive>{error}</ChartMessage>
      ) : data.length === 0 ? (
        <ChartMessage>{t("noItems")}</ChartMessage>
      ) : (
        <div className="h-[360px] min-w-0">
          <ResponsiveContainer height="100%" width="100%">
            <AreaChart
              accessibilityLayer
              data={data}
              margin={{ left: 12, right: 12 }}
            >
              <CartesianGrid vertical={false} />
              <XAxis
                axisLine={false}
                dataKey="date"
                minTickGap={28}
                tickMargin={8}
                tickFormatter={(value) => formatChartDate(String(value), language)}
                tickLine={false}
              />
              <Tooltip
                contentStyle={{
                  background: "var(--popover)",
                  borderColor: "var(--border)",
                  borderRadius: "var(--radius-md)",
                  color: "var(--popover-foreground)",
                }}
                cursor={false}
                labelFormatter={(value) => formatFullDate(String(value), language)}
              />
              <Area
                dataKey="recent_files"
                fill="var(--chart-1)"
                fillOpacity={0.1}
                name={t("recentFiles")}
                stackId="quick-access"
                stroke="var(--chart-1)"
                strokeWidth={1}
                type="monotone"
              />
              <Area
                dataKey="frequent_folders"
                fill="var(--chart-2)"
                fillOpacity={0.15}
                name={t("frequentFolders")}
                stackId="quick-access"
                stroke="var(--chart-2)"
                strokeWidth={1}
                type="monotone"
              />
              <Legend
                iconSize={8}
                iconType="circle"
                wrapperStyle={{ paddingTop: 12 }}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}
    </section>
  );
}

function ChartMessage({
  children,
  destructive = false,
}: {
  children: string;
  destructive?: boolean;
}) {
  return (
    <div
      className={`grid h-[360px] place-items-center text-sm ${
        destructive ? "text-destructive" : "text-muted-foreground"
      }`}
    >
      {children}
    </div>
  );
}

function quickAccessColumnClassName(columnId: string) {
  switch (columnId) {
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
  error,
  items,
  language,
  loading,
  onOpenChange,
  open,
  qaType,
  t,
}: {
  error: string | null;
  items: QaItem[];
  language: Language;
  loading: boolean;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  qaType: QaType;
  t: Translate;
}) {
  const columns = useMemo<ColumnDef<QaItem>[]>(
    () => [
      {
        accessorKey: "name",
        header: t("name"),
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
        header: t("path"),
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
        header: t("lastUsed"),
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
    ],
    [language, t],
  );
  const table = useReactTable({
    columns,
    data: items,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (item) => item.path,
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
                <TableMessage colSpan={3} destructive={Boolean(error)}>
                  {loading ? t("loadingItems") : error ?? t("noItems")}
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

function formatChartDate(value: string, language: Language) {
  const date = new Date(`${value}T00:00:00`);
  return date.toLocaleDateString(language, {
    month: "short",
    day: "numeric",
  });
}

function formatFullDate(value: string, language: Language) {
  return new Date(`${value}T00:00:00`).toLocaleDateString(language, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
