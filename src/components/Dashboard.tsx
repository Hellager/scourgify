import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import Lottie from "lottie-react";
import { useLocation } from "react-router-dom";
import {
  Area,
  AreaChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
} from "recharts";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { toast } from "sonner";
import ecgMonitorAnimation from "@/assets/ecg-monitor.json";
import { useAppShell } from "@/components/AppShell";
import { Button } from "@/components/ui/button";
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
import { QUICK_ACCESS_CHANGED_EVENT, REFRESH_DASHBOARD_EVENT } from "@/lib/app-events";
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
type StatsRange = "24h" | "7d" | "30d";
type TrendSeries = "recent_files" | "frequent_folders";

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

interface QuickAccessChanged {
  qa_type: QaType;
  revision: number;
}

const emptyCounts: QaCounts = { recent: 0, frequent: 0, all: 0 };
const HISTORY_PAGE_SIZE = 10;

export function Dashboard() {
  const { language, t } = useI18n();
  const { config, setConfig } = useAppShell();
  const location = useLocation();
  const [counts, setCounts] = useState<QaCounts>(emptyCounts);
  const [stats, setStats] = useState<Stats | null>(null);
  const [range, setRange] = useState<StatsRange>("30d");
  const [chartFocused, setChartFocused] = useState(false);
  const [visibleSeries, setVisibleSeries] = useState<Record<TrendSeries, boolean>>({
    recent_files: true,
    frequent_folders: true,
  });
  const [overviewLoading, setOverviewLoading] = useState(true);
  const [overviewError, setOverviewError] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DashboardDialog>(null);
  const [qaItems, setQaItems] = useState<QaItem[]>([]);
  const [qaLoading, setQaLoading] = useState(false);
  const [qaError, setQaError] = useState<string | null>(null);
  const [history, setHistory] = useState<CleanRecordPage | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [autoMode, setAutoMode] = useState<AutoCleanFormMode>("disabled");
  const [autoHours, setAutoHours] = useState("24");
  const [autoTime, setAutoTime] = useState("09:00");
  const [savingAutoClean, setSavingAutoClean] = useState(false);

  const loadCounts = useCallback(async () => {
    setCounts(await invokeCommand<QaCounts>("get_qa_counts", { fresh: false }));
  }, []);

  const loadStats = useCallback(async () => {
    setStats(await invokeCommand<Stats>("get_stats", { range }));
  }, [range]);

  const loadOverview = useCallback(async () => {
    setOverviewLoading(true);
    setOverviewError(null);
    const [countsResult, statsResult] = await Promise.allSettled([
      loadCounts(),
      loadStats(),
    ]);
    if (countsResult.status === "rejected") {
      setCounts(emptyCounts);
    }
    if (statsResult.status === "rejected") {
      setStats(null);
      setOverviewError(errorMessage(statsResult.reason));
    }
    setOverviewLoading(false);
  }, [loadCounts, loadStats]);

  const loadQaItems = useCallback(async (qaType: QaType) => {
    setQaLoading(true);
    setQaError(null);
    try {
      setQaItems(
        await invokeCommand<QaItem[]>("list_qa_items", {
          fresh: false,
          qaType,
        }),
      );
    } catch (error) {
      setQaItems([]);
      setQaError(errorMessage(error));
    } finally {
      setQaLoading(false);
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
      setHistory(null);
      setHistoryError(errorMessage(error));
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  useEffect(() => {
    if (location.pathname === "/") {
      void loadOverview();
    }
  }, [loadOverview, location.pathname]);

  useEffect(() => {
    const refreshDashboard = () => void loadOverview();
    window.addEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
    return () => window.removeEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
  }, [loadOverview]);

  useEffect(() => {
    let timeout: number | undefined;
    const unlisten = listen<QuickAccessChanged>(QUICK_ACCESS_CHANGED_EVENT, () => {
      window.clearTimeout(timeout);
      timeout = window.setTimeout(
        () => void loadCounts().catch(() => setCounts(emptyCounts)),
        100,
      );
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

      <CleanupHistoryChart
        chartFocused={chartFocused}
        error={overviewError}
        language={language}
        loading={overviewLoading}
        onChartFocusChange={setChartFocused}
        onRangeChange={setRange}
        onSeriesToggle={(series) =>
          setVisibleSeries((current) => ({ ...current, [series]: !current[series] }))
        }
        range={range}
        stats={stats}
        t={t}
        visibleSeries={visibleSeries}
      />

      <QuickAccessDialog
        items={qaItems}
        language={language}
        loading={qaLoading}
        error={qaError}
        onOpenChange={(open) => !open && setDialog(null)}
        open={dialog === "recent" || dialog === "frequent"}
        qaType={dialog === "frequent" ? "frequent" : "recent"}
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

function AutoCleanMonitorPulse({ label }: { label: string }) {
  const [reduceMotion, setReduceMotion] = useState(false);

  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    const updatePreference = () => setReduceMotion(mediaQuery.matches);
    updatePreference();
    mediaQuery.addEventListener("change", updatePreference);
    return () => mediaQuery.removeEventListener("change", updatePreference);
  }, []);

  return (
    <div className="relative h-9 w-20 max-w-full overflow-hidden">
      <Lottie
        animationData={ecgMonitorAnimation}
        aria-hidden="true"
        autoplay={!reduceMotion}
        className="h-full w-full"
        loop={!reduceMotion}
        rendererSettings={{ preserveAspectRatio: "xMidYMid slice" }}
      />
      <span className="sr-only">{label}</span>
    </div>
  );
}

function CleanupHistoryChart({
  chartFocused,
  error,
  language,
  loading,
  onChartFocusChange,
  onRangeChange,
  onSeriesToggle,
  range,
  stats,
  t,
  visibleSeries,
}: {
  chartFocused: boolean;
  error: string | null;
  language: Language;
  loading: boolean;
  onChartFocusChange: (focused: boolean) => void;
  onRangeChange: (range: StatsRange) => void;
  onSeriesToggle: (series: TrendSeries) => void;
  range: StatsRange;
  stats: Stats | null;
  t: Translate;
  visibleSeries: Record<TrendSeries, boolean>;
}) {
  const trend = stats?.daily_trend ?? [];
  return (
    <section
      className="min-w-0 bg-card text-card-foreground outline-none"
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          onChartFocusChange(false);
        }
      }}
      onClick={() => onChartFocusChange(true)}
      onFocusCapture={() => onChartFocusChange(true)}
      tabIndex={0}
    >
      <div className="flex min-h-8 justify-end">
        {chartFocused ? (
          <div aria-label={t("dateRange")} className="inline-flex rounded-md border p-0.5">
            {(["24h", "7d", "30d"] as const).map((value) => (
              <Button
                aria-pressed={range === value}
                className="h-7 px-2.5"
                key={value}
                onClick={() => onRangeChange(value)}
                size="sm"
                type="button"
                variant={range === value ? "secondary" : "ghost"}
              >
                {value === "24h" ? "24h" : value === "7d" ? "7D" : "30D"}
              </Button>
            ))}
          </div>
        ) : null}
      </div>
      {loading ? (
        <ChartMessage>{t("loadingStatistics")}</ChartMessage>
      ) : error ? (
        <ChartMessage destructive>{error}</ChartMessage>
      ) : trend.length === 0 ? (
        <ChartMessage>{t("noHistory")}</ChartMessage>
      ) : (
        <div className="h-[360px] min-w-0">
          <ResponsiveContainer height="100%" width="100%">
            <AreaChart data={trend} margin={{ bottom: 8, left: 0, right: 12, top: 8 }}>
              <XAxis
                dataKey="period"
                minTickGap={28}
                tickFormatter={(value) => formatChartDate(String(value), language)}
                tickLine={false}
              />
              <Tooltip
                labelFormatter={(value) => formatFullDate(String(value), language)}
              />
              <Area
                dataKey="recent_files"
                fill="#2563eb"
                fillOpacity={0.22}
                hide={!visibleSeries.recent_files}
                name={t("recentFiles")}
                stroke="#2563eb"
                strokeWidth={2}
                type="monotone"
              />
              <Area
                dataKey="frequent_folders"
                fill="#16a34a"
                fillOpacity={0.18}
                hide={!visibleSeries.frequent_folders}
                name={t("frequentFolders")}
                stroke="#16a34a"
                strokeWidth={2}
                type="monotone"
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}
      <div className="flex flex-wrap justify-center gap-2 pt-3">
        {(["recent_files", "frequent_folders"] as const).map((series) => (
          <Button
            aria-pressed={visibleSeries[series]}
            className="h-8 gap-2 px-3"
            key={series}
            onClick={() => onSeriesToggle(series)}
            size="sm"
            type="button"
            variant={visibleSeries[series] ? "secondary" : "ghost"}
          >
            <span
              aria-hidden="true"
              className={`size-2 rounded-full ${
                series === "recent_files" ? "bg-blue-600" : "bg-green-600"
              }`}
            />
            {series === "recent_files" ? t("recentFiles") : t("frequentFolders")}
          </Button>
        ))}
      </div>
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
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-h-[80svh] overflow-hidden sm:max-w-4xl">
        <DialogHeader className="sr-only">
          <DialogTitle>
            {qaType === "recent" ? t("recentFiles") : t("frequentFolders")}
          </DialogTitle>
          <DialogDescription>{t("currentItems")}</DialogDescription>
        </DialogHeader>
        <div className="max-h-[58svh] overflow-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("name")}</TableHead>
                <TableHead>{t("path")}</TableHead>
                <TableHead className="w-40">{t("lastUsed")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {loading || error || items.length === 0 ? (
                <TableMessage colSpan={3} destructive={Boolean(error)}>
                  {loading ? t("loadingItems") : error ?? t("noItems")}
                </TableMessage>
              ) : (
                items.map((item) => (
                  <TableRow key={item.path}>
                    <TableCell className="max-w-56 truncate font-medium" title={item.name}>
                      {item.name}
                    </TableCell>
                    <TableCell className="max-w-[420px] truncate text-muted-foreground" title={item.path}>
                      {item.path}
                    </TableCell>
                    <TableCell className="whitespace-nowrap text-muted-foreground">
                      {item.last_interaction_at === null
                        ? "--"
                        : new Date(item.last_interaction_at).toLocaleString(language)}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>
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
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-h-[80svh] overflow-hidden sm:max-w-5xl">
        <DialogHeader>
          <DialogTitle>{t("cleaned")}</DialogTitle>
          <DialogDescription>
            {t("retainedHistory")}: {history?.overall_total ?? "--"}
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-[52svh] overflow-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("path")}</TableHead>
                <TableHead className="w-40">{t("type")}</TableHead>
                <TableHead className="w-32">{t("cleanupSource")}</TableHead>
                <TableHead className="w-44">{t("cleanedAt")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {loading || error || !history || history.records.length === 0 ? (
                <TableMessage colSpan={4} destructive={Boolean(error)}>
                  {loading ? t("loadingHistory") : error ?? t("noHistory")}
                </TableMessage>
              ) : (
                history.records.map((record) => (
                  <TableRow key={record.id}>
                    <TableCell className="max-w-[480px] truncate" title={record.item_path}>
                      {record.item_path}
                    </TableCell>
                    <TableCell>
                      {t(record.item_type === "recent_file" ? "recentFile" : "frequentFolder")}
                    </TableCell>
                    <TableCell>
                      {t(record.source === "auto" ? "automatic" : "manualTrigger")}
                    </TableCell>
                    <TableCell className="whitespace-nowrap text-muted-foreground">
                      {new Date(record.cleaned_at).toLocaleString(language)}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>
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
              disabled={loading || page <= 1}
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
              disabled={loading || page >= pageCount}
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
