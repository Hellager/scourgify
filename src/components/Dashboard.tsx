import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Link } from "react-router-dom";
import {
  type Column,
  type ColumnDef,
  flexRender,
  getCoreRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  type PaginationState,
  type RowSelectionState,
  type SortingState,
  useReactTable,
  type VisibilityState,
} from "@tanstack/react-table";
import {
  CartesianGrid,
  Cell,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  ChevronLeft,
  ChevronRight,
  Columns3,
  FolderOpen,
  FolderPlus,
  RefreshCw,
  RotateCcw,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  SlidersHorizontal,
  Target,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
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
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  notifyOperationComplete,
  notifyPartialFailure,
} from "@/lib/notifications";
import {
  dispatchAppEvent,
  OPEN_CONFIG_DRAWER_EVENT,
  REFRESH_DASHBOARD_EVENT,
} from "@/lib/app-events";
import { type I18nKey, useI18n } from "@/lib/i18n";
import { PageHeader, useAppShell } from "@/components/AppShell";

type QaType = "recent" | "frequent";

type QaMatch =
  | { status: "protected"; rule_id: number; keyword: string }
  | { status: "targeted"; rule_id: number; keyword: string }
  | { status: "neutral" };

interface QaItem {
  path: string;
  name: string;
  item_type: "recent_file" | "frequent_folder";
  last_interaction_at: number | null;
  match: QaMatch;
}

interface QaTableRow extends QaItem {
  type: string;
}

interface QaCounts {
  recent: number;
  frequent: number;
  all: number;
}

interface QaBatchResult {
  total: number;
  succeeded: string[];
  failed: Array<{ path: string; error: string }>;
  skipped_protected: string[];
  history_error: string | null;
}

interface DatabaseStatus {
  available: boolean;
}

interface QaRestoreResult {
  success: boolean;
  recent: QaRestoreSectionResult | null;
  frequent: QaRestoreSectionResult | null;
}

interface QaRestoreSectionResult {
  success: boolean;
  deleted_lnk_count: number;
  error: string | null;
}

interface OperationSummary {
  action: string;
  failed: number;
  message: string;
  succeeded: number;
  target: string;
  total: number;
  updatedAt: string;
}

interface QuickAccessChartItem {
  color: string;
  name: string;
  value: number;
}

interface StatsTrendPoint {
  period: string;
  count: number;
}

interface RuleHitStat {
  keyword: string;
  count: number;
}

interface Stats {
  total: number;
  recent_files: number;
  frequent_folders: number;
  daily_trend: StatsTrendPoint[];
  weekly_trend: StatsTrendPoint[];
  rule_hits: RuleHitStat[];
}

type StatsRange = "7d" | "30d" | "all";

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

type PendingAction =
  | "remove"
  | "empty"
  | "smart"
  | "restore-current"
  | "restore-all"
  | null;

const emptyCounts: QaCounts = {
  recent: 0,
  frequent: 0,
  all: 0,
};

const quickAccessChartColors = ["#2563eb", "#16a34a"];

export function Dashboard() {
  const { language, t } = useI18n();
  const { config, updateDashboardSummary } = useAppShell();
  const [activeTab, setActiveTab] = useState<QaType>("recent");
  const [counts, setCounts] = useState<QaCounts>(emptyCounts);
  const [items, setItems] = useState<QaItem[]>([]);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [databaseAvailable, setDatabaseAvailable] = useState(true);
  const [stats, setStats] = useState<Stats | null>(null);
  const [statsRange, setStatsRange] = useState<StatsRange>("30d");
  const [statsLoading, setStatsLoading] = useState(true);
  const [statsError, setStatsError] = useState<string | null>(null);
  const [privacyActive, setPrivacyActive] = useState(false);
  const [pendingAction, setPendingAction] = useState<PendingAction>(null);
  const [pinFolderPath, setPinFolderPath] = useState("");
  const [lastOperationSummary, setLastOperationSummary] =
    useState<OperationSummary | null>(null);
  const [sorting, setSorting] = useState<SortingState>([]);
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  });
  const [columnVisibility, setColumnVisibility] = useState<VisibilityState>({});

  const filteredItems = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) {
      return items;
    }

    return items.filter((item) => {
      return (
        item.name.toLowerCase().includes(normalized) ||
        item.path.toLowerCase().includes(normalized)
      );
    });
  }, [items, query]);

  const quickAccessChartData = useMemo(
    () => [
      {
        color: quickAccessChartColors[0],
        name: t("recentFiles"),
        value: counts.recent,
      },
      {
        color: quickAccessChartColors[1],
        name: t("frequentFolders"),
        value: counts.frequent,
      },
    ],
    [counts.frequent, counts.recent, t],
  );
  const quickAccessTotal = counts.recent + counts.frequent;
  const smartTargets = useMemo(
    () => items.filter((item) => item.match.status === "targeted"),
    [items],
  );
  const cleanableCount = useMemo(
    () => items.filter((item) => item.match.status !== "protected").length,
    [items],
  );

  const tableData = useMemo<QaTableRow[]>(
    () =>
      filteredItems.map((item) => ({
        ...item,
        type: activeTab === "recent" ? t("recentFile") : t("frequentFolders"),
      })),
    [activeTab, filteredItems, t],
  );

  const rowSelection = useMemo<RowSelectionState>(
    () =>
      Object.fromEntries(Array.from(selectedPaths).map((path) => [path, true])),
    [selectedPaths],
  );

  const columns = useMemo<ColumnDef<QaTableRow>[]>(
    () => [
      {
        id: "select",
        enableHiding: false,
        enableSorting: false,
        header: t("selected"),
        cell: ({ row }) => (
          <Checkbox
            aria-label={t("selectItem", { name: row.original.name })}
            checked={selectedPaths.has(row.original.path)}
            disabled={row.original.match.status === "protected"}
            onCheckedChange={(checked) =>
              togglePath(row.original.path, checked)
            }
          />
        ),
      },
      {
        accessorKey: "name",
        header: t("name"),
        cell: ({ row }) => (
          <span className="block max-w-64 truncate font-medium">
            {row.original.name}
          </span>
        ),
      },
      {
        accessorKey: "path",
        header: t("path"),
        cell: ({ row }) => (
          <span className="block max-w-[560px] truncate text-muted-foreground">
            {row.original.path}
          </span>
        ),
      },
      {
        accessorKey: "last_interaction_at",
        header: t("lastUsed"),
        cell: ({ row }) => {
          const timestamp = row.original.last_interaction_at;
          return timestamp === null ? null : (
            <span
              className="whitespace-nowrap text-muted-foreground"
              title={new Date(timestamp).toLocaleString(language)}
            >
              {formatRelativeTime(timestamp, language)}
            </span>
          );
        },
      },
      {
        id: "match",
        accessorFn: (row) => row.match.status,
        header: t("ruleMatch"),
        cell: ({ row }) => <MatchStatusLabel match={row.original.match} t={t} />,
      },
      {
        accessorKey: "type",
        header: t("type"),
        cell: ({ row }) => (
          <span className="whitespace-nowrap text-muted-foreground">
            {row.original.type}
          </span>
        ),
      },
      {
        id: "location",
        enableSorting: false,
        header: t("location"),
        cell: ({ row }) => (
          <div className="text-right">
            <Button
              aria-label={t("openLocationFor", { name: row.original.name })}
              onClick={() => void openLocation(row.original.path)}
              size="icon-sm"
              title={t("openLocation")}
              type="button"
              variant="ghost"
            >
              <FolderOpen />
            </Button>
          </div>
        ),
      },
    ],
    [language, selectedPaths, t],
  );

  const table = useReactTable({
    data: tableData,
    columns,
    state: {
      sorting,
      pagination,
      columnVisibility,
      rowSelection,
    },
    enableRowSelection: (row) => row.original.match.status !== "protected",
    getCoreRowModel: getCoreRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    getRowId: (row) => row.path,
    getSortedRowModel: getSortedRowModel(),
    onColumnVisibilityChange: setColumnVisibility,
    onPaginationChange: setPagination,
    onSortingChange: setSorting,
  });

  const pageRows = table.getRowModel().rows;
  const selectablePageRows = pageRows.filter(
    (row) => row.original.match.status !== "protected",
  );
  const allPageSelected =
    selectablePageRows.length > 0 &&
    selectablePageRows.every((row) => selectedPaths.has(row.id));
  const somePageSelected =
    selectablePageRows.some((row) => selectedPaths.has(row.id)) &&
    !allPageSelected;
  const visibleColumnCount = table.getVisibleLeafColumns().length;
  const pageCount = table.getPageCount();

  const loadCounts = useCallback(async () => {
    setCounts(await invoke<QaCounts>("get_qa_counts"));
  }, []);

  const loadStats = useCallback(async () => {
    setStatsLoading(true);
    setStatsError(null);
    try {
      setStats(await invoke<Stats>("get_stats", { range: statsRange }));
    } catch (error) {
      setStats(null);
      setStatsError(errorMessage(error));
    } finally {
      setStatsLoading(false);
    }
  }, [statsRange]);

  const loadItems = useCallback(async (qaType: QaType) => {
    setLoading(true);
    setError(null);
    try {
      const database = await invoke<DatabaseStatus>("get_database_status");
      setDatabaseAvailable(database.available);
      if (database.available) {
        setItems(
          await invoke<QaItem[]>("list_qa_items_classified", { qaType }),
        );
      } else {
        const legacyItems = await invoke<
          Array<Pick<QaItem, "path" | "name" | "last_interaction_at">>
        >("list_qa_items", { qaType });
        setItems(
          legacyItems.map((item) => ({
            ...item,
            item_type:
              qaType === "recent" ? "recent_file" : "frequent_folder",
            match: { status: "neutral" },
          })),
        );
      }
    } catch (error) {
      setItems([]);
      setError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  const syncPrivacyState = useCallback(async () => {
    const state = await invoke<PrivacyState>("privacy_state");
    const active = state !== "Inactive";
    setPrivacyActive(active);
    return active;
  }, []);

  const refresh = useCallback(async () => {
    setSelectedPaths(new Set());
    setPagination((current) => ({ ...current, pageIndex: 0 }));
    const [countsResult, privacyResult] = await Promise.allSettled([
      loadCounts(),
      syncPrivacyState(),
      loadItems(activeTab),
    ]);
    if (countsResult.status === "rejected") {
      setCounts(emptyCounts);
      setError(errorMessage(countsResult.reason));
    }
    if (privacyResult.status === "rejected") {
      setPrivacyActive(false);
    }
  }, [activeTab, loadCounts, loadItems, syncPrivacyState]);

  const refreshAll = useCallback(
    () => Promise.all([refresh(), loadStats()]),
    [loadStats, refresh],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    void loadStats();
  }, [loadStats]);

  useEffect(() => {
    setPagination((current) =>
      current.pageIndex === 0 ? current : { ...current, pageIndex: 0 },
    );
  }, [activeTab, query, items.length]);

  useEffect(() => {
    const refreshDashboard = () => void refreshAll();

    window.addEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
    return () => {
      window.removeEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
    };
  }, [refreshAll]);

  useEffect(() => {
    updateDashboardSummary({
      recent: counts.recent,
      frequent: counts.frequent,
      selected: selectedPaths.size,
    });
  }, [counts.frequent, counts.recent, selectedPaths.size, updateDashboardSummary]);

  const switchTab = (value: unknown) => {
    const nextTab = toQaType(value);
    setActiveTab(nextTab);
    setSelectedPaths(new Set());
    setQuery("");
    setPagination((current) => ({ ...current, pageIndex: 0 }));
  };

  const togglePath = (path: string, checked: boolean) => {
    setSelectedPaths((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(path);
      } else {
        next.delete(path);
      }
      return next;
    });
  };

  const toggleCurrentPage = (checked: boolean) => {
    setSelectedPaths((current) => {
      const next = new Set(current);
      for (const row of selectablePageRows) {
        if (checked) {
          next.add(row.id);
        } else {
          next.delete(row.id);
        }
      }
      return next;
    });
  };

  const selectedCount = selectedPaths.size;
  const currentLabel =
    activeTab === "recent" ? t("recentFiles") : t("frequentFolders");
  const actionsDisabled = loading || mutating || privacyActive;
  const cleanupActionsDisabled = actionsDisabled || !databaseAvailable;
  const removeDisabled = cleanupActionsDisabled || selectedCount === 0;
  const emptyDisabled = cleanupActionsDisabled || cleanableCount === 0;
  const smartDisabled = cleanupActionsDisabled || smartTargets.length === 0;
  const pinDisabled = actionsDisabled || pinFolderPath.trim() === "";

  const executeAction = async (action: Exclude<PendingAction, null>) => {
    setPendingAction(null);
    setMutating(true);
    try {
      if (await syncPrivacyState()) {
        toast.warning(t("privacyWriteDisabled"));
        return;
      }

      if (action === "remove" || action === "empty" || action === "smart") {
        const result =
          action === "remove"
            ? await invoke<QaBatchResult>("remove_qa_items", {
                qaType: activeTab,
                paths: Array.from(selectedPaths),
              })
            : action === "empty"
              ? await invoke<QaBatchResult>("empty_qa_items", {
                  qaType: activeTab,
                })
              : await invoke<QaBatchResult>("smart_clean", {
                  qaType: activeTab,
                });
        const actionLabel = getOperationLabel(action, t);
        const message = cleanupResultMessage(result, t);
        setLastOperationSummary(
          createOperationSummary({
            action: actionLabel,
            failed: result.failed.length,
            message,
            succeeded: result.succeeded.length,
            target: currentLabel,
            total: result.total,
          }),
        );
        showCleanupToast(result, t);
        if (result.failed.length > 0 || result.history_error) {
          void notifyPartialFailure(
            "Scourgify",
            result.history_error ? t("cleanupHistoryWarning") : message,
          );
        } else {
          void notifyOperationComplete("Scourgify", message);
        }
      } else {
        const result = await invoke<QaRestoreResult>("restore_qa_defaults", {
          qaType: action === "restore-all" ? "all" : activeTab,
        });
        setLastOperationSummary(
          createRestoreOperationSummary(
            result,
            action === "restore-all" ? t("all") : currentLabel,
            t,
          ),
        );
        showRestoreToast(result, t);
        if (result.success) {
          void notifyOperationComplete("Scourgify", t("restoredDefaults"));
        } else {
          void notifyPartialFailure(
            "Scourgify",
            t("partialRestore", { failed: getRestoreFailedCount(result) }),
          );
        }
      }

      await refreshAll();
    } catch (error) {
      setLastOperationSummary(
        createFailedOperationSummary(action, currentLabel, error, t),
      );
      toast.error(errorMessage(error));
    } finally {
      setMutating(false);
    }
  };

  const openAction = async (action: Exclude<PendingAction, null>) => {
    try {
      if (await syncPrivacyState()) {
        toast.warning(t("privacyWriteDisabled"));
        return;
      }
      if (action === "smart" && smartTargets.length === 0) {
        toast.info(t("noSmartCleanTargets"));
        return;
      }
      if (action === "smart" && !config.smart_clean_confirm) {
        await executeAction(action);
        return;
      }
      setPendingAction(action);
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const confirmPendingAction = async () => {
    if (pendingAction) {
      await executeAction(pendingAction);
    }
  };

  const choosePinFolder = async () => {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === "string") {
        setPinFolderPath(selected);
      }
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const pinFolder = async () => {
    const path = pinFolderPath.trim();
    if (!path) {
      return;
    }

    setMutating(true);
    try {
      if (await syncPrivacyState()) {
        toast.warning(t("privacyWriteDisabled"));
        return;
      }
      await invoke("pin_qa_folder", { path });
      setPinFolderPath("");
      toast.success(t("pinnedFolder"));
      await refresh();
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setMutating(false);
    }
  };

  const openLocation = async (path: string) => {
    try {
      await invoke("open_in_explorer", { path });
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const tableState = getTableState({
    error,
    filteredCount: filteredItems.length,
    itemCount: items.length,
    loading,
    query,
    t,
  });

  return (
    <>
      <PageHeader
        actions={
          <>
          <Button
            aria-label={t("openAppearanceDrawer")}
            onClick={() => dispatchAppEvent(OPEN_CONFIG_DRAWER_EVENT)}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            <SlidersHorizontal />
          </Button>
          <Button
            aria-label={t("refreshQuickAccess")}
            disabled={loading}
            onClick={() => void refreshAll()}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            <RefreshCw className={loading ? "animate-spin" : ""} />
          </Button>
          <Button
            aria-label={t("openSettings")}
            render={<Link to="/settings" />}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            <Settings />
          </Button>
          <Link
            className="text-sm text-muted-foreground hover:text-foreground"
            to="/about"
          >
            {t("about")}
          </Link>
          </>
        }
        subtitle={t("dashboard")}
        title="Scourgify"
      />

      <section className="grid gap-4 p-6 sm:grid-cols-2 lg:grid-cols-4">
        <CountCard label={t("recentFiles")} value={counts.recent} />
        <CountCard label={t("frequentFolders")} value={counts.frequent} />
        <CountCard label={t("visible")} value={filteredItems.length} />
        <CountCard label={t("selected")} value={selectedPaths.size} />
      </section>

      <section className="grid gap-4 px-6 pb-6 lg:grid-cols-[minmax(0,1fr)_minmax(320px,420px)]">
        <OverviewChart data={quickAccessChartData} t={t} total={quickAccessTotal} />
        <OperationSummaryPanel summary={lastOperationSummary} t={t} />
      </section>

      <HistoryStats
        available={databaseAvailable}
        error={statsError}
        loading={statsLoading}
        onRangeChange={setStatsRange}
        range={statsRange}
        stats={stats}
        t={t}
      />

      <section className="grid gap-3 px-6 pb-6 md:grid-cols-[1fr_auto_auto]">
        <label className="min-w-0">
          <span className="sr-only">{t("pinFolderPath")}</span>
          <Input
            disabled={actionsDisabled}
            onChange={(event) => setPinFolderPath(event.target.value)}
            placeholder={t("pinFolderPath")}
            value={pinFolderPath}
          />
        </label>
        <Button
          disabled={actionsDisabled}
          onClick={() => void choosePinFolder()}
          type="button"
          variant="outline"
        >
          {t("browse")}
        </Button>
        <Button
          disabled={pinDisabled}
          onClick={() => void pinFolder()}
          type="button"
        >
          <FolderPlus />
          {t("pinFolder")}
        </Button>
      </section>

      <section className="px-6 pb-6">
        <Tabs value={activeTab} onValueChange={switchTab}>
          <div className="flex flex-col gap-3 border-b pb-4 md:flex-row md:items-center md:justify-between">
            <div className="flex flex-col gap-3 md:flex-row md:items-center">
              <TabsList>
                {(["recent", "frequent"] as const).map((tab) => (
                  <TabsTrigger key={tab} value={tab}>
                    {tab === "recent" ? t("recentFiles") : t("frequentFolders")}
                  </TabsTrigger>
                ))}
              </TabsList>
              <label className="relative w-full md:w-80">
                <span className="sr-only">{t("searchQuickAccess")}</span>
                <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <input
                  className="h-9 w-full rounded-md border bg-background pl-9 pr-3 text-sm outline-none transition-colors placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t("searchPlaceholder")}
                  type="search"
                  value={query}
                />
              </label>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {privacyActive ? (
                <span className="text-xs text-muted-foreground">
                  {t("privacyActive")}
                </span>
              ) : null}
              {!databaseAvailable ? (
                <span className="text-xs text-destructive">
                  {t("databaseUnavailable")}
                </span>
              ) : null}
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button size="sm" type="button" variant="outline">
                      <Columns3 />
                      {t("columns")}
                    </Button>
                  }
                />
                <DropdownMenuContent align="end" className="w-48">
                  <DropdownMenuLabel>{t("visibleColumns")}</DropdownMenuLabel>
                  <DropdownMenuSeparator />
                  {table
                    .getAllLeafColumns()
                    .filter((column) => column.getCanHide())
                    .map((column) => (
                      <DropdownMenuCheckboxItem
                        checked={column.getIsVisible()}
                        key={column.id}
                        onCheckedChange={(checked) =>
                          column.toggleVisibility(checked)
                        }
                      >
                        {getColumnLabel(column.id, t)}
                      </DropdownMenuCheckboxItem>
                    ))}
                </DropdownMenuContent>
              </DropdownMenu>
              <Button
                disabled={smartDisabled}
                onClick={() => void openAction("smart")}
                size="sm"
                type="button"
              >
                <Sparkles />
                {t("smartCleanCount", { count: smartTargets.length })}
              </Button>
              <Button
                disabled={removeDisabled}
                onClick={() => void openAction("remove")}
                size="sm"
                type="button"
                variant="outline"
              >
                <Trash2 />
                {t("actionRemoveSelected")}
              </Button>
              <Button
                disabled={emptyDisabled}
                onClick={() => void openAction("empty")}
                size="sm"
                type="button"
                variant="destructive"
              >
                {t("clearCurrent", { label: currentLabel })}
              </Button>
              <Button
                disabled={actionsDisabled}
                onClick={() => void openAction("restore-current")}
                size="sm"
                type="button"
                variant="outline"
              >
                <RotateCcw />
                {t("restoreCurrent", { label: currentLabel })}
              </Button>
              <Button
                disabled={actionsDisabled}
                onClick={() => void openAction("restore-all")}
                size="sm"
                type="button"
                variant="outline"
              >
                {t("restoreAll")}
              </Button>
            </div>
          </div>

          <TabsContent className="pt-4" value={activeTab}>
            <Table>
              <TableHeader>
                {table.getHeaderGroups().map((headerGroup) => (
                  <TableRow key={headerGroup.id}>
                    {headerGroup.headers.map((header) => (
                      <TableHead
                        className={getHeaderClassName(header.column.id)}
                        key={header.id}
                      >
                        {header.column.id === "select" ? (
                          <Checkbox
                            aria-label={t("selectCurrentPage")}
                            checked={allPageSelected}
                            disabled={selectablePageRows.length === 0}
                            indeterminate={somePageSelected}
                            onCheckedChange={toggleCurrentPage}
                            parent
                          />
                        ) : header.isPlaceholder ? null : header.column.getCanSort() ? (
                          <SortableHeader column={header.column}>
                            {flexRender(
                              header.column.columnDef.header,
                              header.getContext(),
                            )}
                          </SortableHeader>
                        ) : (
                          flexRender(
                            header.column.columnDef.header,
                            header.getContext(),
                          )
                        )}
                      </TableHead>
                    ))}
                  </TableRow>
                ))}
              </TableHeader>
              <TableBody>
                {tableState ? (
                  <TableRow>
                    <TableCell
                      className="h-40 text-center text-muted-foreground"
                      colSpan={visibleColumnCount}
                    >
                      <div className="flex flex-col items-center gap-3">
                        <span>{tableState}</span>
                        {error ? (
                          <Button
                            onClick={() => void refresh()}
                            size="sm"
                            type="button"
                            variant="outline"
                          >
                            {t("refreshQuickAccess")}
                          </Button>
                        ) : null}
                      </div>
                    </TableCell>
                  </TableRow>
                ) : (
                  table.getRowModel().rows.map((row) => (
                    <TableRow
                      data-state={
                        selectedPaths.has(row.id) ? "selected" : undefined
                      }
                      key={row.id}
                    >
                      {row.getVisibleCells().map((cell) => (
                        <TableCell
                          className={getCellClassName(cell.column.id)}
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
            <div className="flex flex-col gap-3 border-t py-4 text-sm text-muted-foreground md:flex-row md:items-center md:justify-between">
              <div>
                {t("pageStatus", {
                  count: filteredItems.length,
                  page: pageCount === 0 ? 0 : pagination.pageIndex + 1,
                  pageCount,
                })}
              </div>
              <div className="flex items-center gap-2">
                <span>{t("rowsPerPage")}</span>
                <Select
                  onValueChange={(value) => table.setPageSize(Number(value))}
                  value={String(pagination.pageSize)}
                >
                  <SelectTrigger className="h-8 w-20">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="10">10</SelectItem>
                    <SelectItem value="20">20</SelectItem>
                    <SelectItem value="50">50</SelectItem>
                  </SelectContent>
                </Select>
                <Button
                  disabled={!table.getCanPreviousPage()}
                  onClick={() => table.previousPage()}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  <ChevronLeft />
                  {t("previous")}
                </Button>
                <Button
                  disabled={!table.getCanNextPage()}
                  onClick={() => table.nextPage()}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {t("next")}
                  <ChevronRight />
                </Button>
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </section>
      <ConfirmActionDialog
        action={pendingAction}
        currentLabel={currentLabel}
        isFrequent={activeTab === "frequent"}
        onClose={() => setPendingAction(null)}
        onConfirm={() => void confirmPendingAction()}
        selectedCount={selectedCount}
        smartTargets={smartTargets}
        t={t}
      />
    </>
  );
}

function ConfirmActionDialog({
  action,
  currentLabel,
  isFrequent,
  onClose,
  onConfirm,
  selectedCount,
  smartTargets,
  t,
}: {
  action: PendingAction;
  currentLabel: string;
  isFrequent: boolean;
  onClose: () => void;
  onConfirm: () => void;
  selectedCount: number;
  smartTargets: QaItem[];
  t: (key: I18nKey, values?: Record<string, string | number>) => string;
}) {
  const isEmpty = action === "empty";
  const isSmart = action === "smart";
  const isRestore = action === "restore-current" || action === "restore-all";
  const title = isSmart
    ? t("smartCleanQuestion")
    : isEmpty
    ? t("clearCurrentQuestion", { label: currentLabel })
    : isRestore
      ? action === "restore-all"
        ? t("restoreAllDefaultsQuestion")
        : t("restoreCurrentDefaultsQuestion", { label: currentLabel })
      : t("removeSelectedQuestion");
  const description = isSmart
    ? t("smartCleanDescription", { count: smartTargets.length })
    : isEmpty
    ? t("emptyCurrentDescription", { label: currentLabel })
    : isRestore
      ? action === "restore-all"
        ? t("restoreAllDescription")
        : t("restoreCurrentDescription", { label: currentLabel })
      : t("removeSelectedDescription", { count: selectedCount });

  return (
    <AlertDialog open={action !== null} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent className={isSmart ? "sm:max-w-2xl" : undefined}>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>
            {description}
            {isEmpty && isFrequent
              ? t("emptyFrequentWarning")
              : ""}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {isSmart ? (
          <div className="max-h-64 overflow-y-auto rounded-md border">
            {smartTargets.map((item) => (
              <div
                className="grid gap-1 border-b px-3 py-2 text-sm last:border-b-0"
                key={item.path}
              >
                <div className="flex min-w-0 items-center justify-between gap-3">
                  <span className="truncate font-medium">{item.name}</span>
                  {item.match.status === "targeted" ? (
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {item.match.keyword}
                    </span>
                  ) : null}
                </div>
                <span className="truncate text-xs text-muted-foreground">
                  {item.path}
                </span>
              </div>
            ))}
          </div>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            type="button"
            variant="destructive"
          >
            {isSmart
              ? t("smartClean")
              : isEmpty
                ? t("actionClear")
                : isRestore
                  ? t("restore")
                  : t("actionRemoveSelected")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function MatchStatusLabel({
  match,
  t,
}: {
  match: QaMatch;
  t: (key: I18nKey, values?: Record<string, string | number>) => string;
}) {
  if (match.status === "neutral") {
    return <span className="text-xs text-muted-foreground">{t("neutral")}</span>;
  }

  const Icon = match.status === "protected" ? ShieldCheck : Target;
  return (
    <span
      className={
        match.status === "protected"
          ? "inline-flex max-w-48 items-center gap-1.5 text-xs text-emerald-700 dark:text-emerald-400"
          : "inline-flex max-w-48 items-center gap-1.5 text-xs text-amber-700 dark:text-amber-400"
      }
      title={match.keyword}
    >
      <Icon className="size-3.5 shrink-0" />
      <span className="shrink-0">
        {t(match.status === "protected" ? "protected" : "targeted")}
      </span>
      <span className="truncate">{match.keyword}</span>
    </span>
  );
}

function CountCard({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-md border bg-card p-4 text-card-foreground">
      <p className="text-sm text-muted-foreground">{label}</p>
      <p className="mt-3 text-2xl font-semibold tabular-nums">{value}</p>
    </div>
  );
}

function OverviewChart({
  data,
  t,
  total,
}: {
  data: QuickAccessChartItem[];
  t: (key: I18nKey, values?: Record<string, string | number>) => string;
  total: number;
}) {
  return (
    <div className="rounded-md border bg-card p-4 text-card-foreground">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="text-sm font-semibold">
            {t("quickAccessComposition")}
          </h2>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("currentSplit")}
          </p>
        </div>
        <span className="text-2xl font-semibold tabular-nums">{total}</span>
      </div>
      {total === 0 ? (
        <div className="grid h-48 place-items-center text-sm text-muted-foreground">
          {t("noItems")}
        </div>
      ) : (
        <div className="mt-4 grid gap-4 md:grid-cols-[240px_1fr] md:items-center">
          <div className="h-48">
            <ResponsiveContainer height="100%" width="100%">
              <PieChart>
                <Pie
                  data={data}
                  dataKey="value"
                  innerRadius={54}
                  nameKey="name"
                  outerRadius={78}
                  paddingAngle={2}
                >
                  {data.map((item) => (
                    <Cell fill={item.color} key={item.name} />
                  ))}
                </Pie>
                <Tooltip />
              </PieChart>
            </ResponsiveContainer>
          </div>
          <div className="grid gap-3">
            {data.map((item) => (
              <div
                className="flex items-center justify-between gap-3 text-sm"
                key={item.name}
              >
                <span className="flex min-w-0 items-center gap-2">
                  <span
                    className="size-2.5 rounded-full"
                    style={{ backgroundColor: item.color }}
                  />
                  <span className="truncate">{item.name}</span>
                </span>
                <span className="font-medium tabular-nums">
                  {item.value} / {Math.round((item.value / total) * 100)}%
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function OperationSummaryPanel({
  summary,
  t,
}: {
  summary: OperationSummary | null;
  t: (key: I18nKey, values?: Record<string, string | number>) => string;
}) {
  return (
    <div className="rounded-md border bg-card p-4 text-card-foreground">
      <div>
        <h2 className="text-sm font-semibold">{t("lastOperation")}</h2>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("latestBatchResult")}
        </p>
      </div>
      {summary ? (
        <div className="mt-5 grid gap-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm font-medium">{summary.action}</p>
              <p className="mt-1 text-xs text-muted-foreground">
                {summary.target} / {summary.updatedAt}
              </p>
            </div>
            <span className="rounded-sm border px-2 py-1 text-xs">
              {summary.failed > 0 ? t("needsAttention") : t("complete")}
            </span>
          </div>
          <div className="grid grid-cols-3 gap-2 border-y py-3 text-center">
            <SummaryMetric label={t("succeeded")} value={summary.succeeded} />
            <SummaryMetric label={t("failed")} value={summary.failed} />
            <SummaryMetric label={t("total")} value={summary.total} />
          </div>
          <p className="text-sm text-muted-foreground">{summary.message}</p>
        </div>
      ) : (
        <div className="grid h-48 place-items-center text-sm text-muted-foreground">
          {t("noOperations")}
        </div>
      )}
    </div>
  );
}

function HistoryStats({
  available,
  error,
  loading,
  onRangeChange,
  range,
  stats,
  t,
}: {
  available: boolean;
  error: string | null;
  loading: boolean;
  onRangeChange: (range: StatsRange) => void;
  range: StatsRange;
  stats: Stats | null;
  t: (key: I18nKey, values?: Record<string, string | number>) => string;
}) {
  const [period, setPeriod] = useState<"daily" | "weekly">("daily");
  const trend = period === "daily" ? stats?.daily_trend : stats?.weekly_trend;
  const maxRuleHits = stats?.rule_hits[0]?.count ?? 0;

  return (
    <section className="px-6 pb-6" aria-labelledby="history-stats-title">
      <h2 className="text-sm font-semibold" id="history-stats-title">
        {t("historyStatistics")}
      </h2>
      {!available ? (
        <p className="mt-3 text-sm text-muted-foreground">
          {t("databaseUnavailable")}
        </p>
      ) : loading ? (
        <p className="mt-3 text-sm text-muted-foreground">
          {t("loadingStatistics")}
        </p>
      ) : error ? (
        <p className="mt-3 text-sm text-destructive">{error}</p>
      ) : stats ? (
        <>
          <div className="mt-4 grid gap-4 sm:grid-cols-3">
            <CountCard label={t("totalCleaned")} value={stats.total} />
            <CountCard label={t("recentFiles")} value={stats.recent_files} />
            <CountCard
              label={t("frequentFolders")}
              value={stats.frequent_folders}
            />
          </div>
          <div className="mt-4 grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(280px,1fr)]">
            <div className="min-w-0 rounded-md border bg-card p-4 text-card-foreground">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <h3 className="text-sm font-semibold">{t("cleaningTrend")}</h3>
                <div className="flex flex-wrap items-center gap-2">
                  <div
                    aria-label={t("dateRange")}
                    className="inline-flex rounded-md border p-0.5"
                    role="group"
                  >
                    {(["7d", "30d", "all"] as const).map((value) => {
                      const label = t(
                        value === "7d"
                          ? "last7Days"
                          : value === "30d"
                            ? "last30Days"
                            : "all",
                      );
                      return (
                        <Button
                          aria-label={label}
                          aria-pressed={range === value}
                          className="h-7 px-2.5"
                          key={value}
                          onClick={() => onRangeChange(value)}
                          size="sm"
                          title={label}
                          type="button"
                          variant={range === value ? "secondary" : "ghost"}
                        >
                          {value === "7d"
                            ? "7D"
                            : value === "30d"
                              ? "30D"
                              : t("all")}
                        </Button>
                      );
                    })}
                  </div>
                  <div className="inline-flex rounded-md border p-0.5">
                    {(["daily", "weekly"] as const).map((value) => (
                      <Button
                        aria-pressed={period === value}
                        className="h-7 px-2.5"
                        key={value}
                        onClick={() => setPeriod(value)}
                        size="sm"
                        type="button"
                        variant={period === value ? "secondary" : "ghost"}
                      >
                        {t(value)}
                      </Button>
                    ))}
                  </div>
                </div>
              </div>
              {trend && trend.length > 0 ? (
                <div className="mt-4 h-60 min-w-0">
                  <ResponsiveContainer height="100%" width="100%">
                    <LineChart data={trend} margin={{ left: -20, right: 8 }}>
                      <CartesianGrid strokeDasharray="3 3" vertical={false} />
                      <XAxis dataKey="period" minTickGap={24} tickLine={false} />
                      <YAxis allowDecimals={false} tickLine={false} />
                      <Tooltip />
                      <Line
                        dataKey="count"
                        dot={false}
                        stroke="#2563eb"
                        strokeWidth={2}
                        type="monotone"
                      />
                    </LineChart>
                  </ResponsiveContainer>
                </div>
              ) : (
                <div className="grid h-60 place-items-center text-sm text-muted-foreground">
                  {t("noHistory")}
                </div>
              )}
            </div>
            <div className="rounded-md border bg-card p-4 text-card-foreground">
              <h3 className="text-sm font-semibold">{t("ruleHitRanking")}</h3>
              {stats.rule_hits.length > 0 ? (
                <ol className="mt-5 grid gap-4">
                  {stats.rule_hits.map((rule, index) => (
                    <li
                      className="grid grid-cols-[1.5rem_minmax(0,1fr)_auto] items-center gap-2"
                      key={rule.keyword}
                    >
                      <span className="text-xs tabular-nums text-muted-foreground">
                        {index + 1}
                      </span>
                      <div className="min-w-0">
                        <div className="truncate text-sm" title={rule.keyword}>
                          {rule.keyword}
                        </div>
                        <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-muted">
                          <div
                            className="h-full rounded-full bg-amber-500"
                            style={{
                              width: `${(rule.count / maxRuleHits) * 100}%`,
                            }}
                          />
                        </div>
                      </div>
                      <span className="text-sm font-medium tabular-nums">
                        {rule.count}
                      </span>
                    </li>
                  ))}
                </ol>
              ) : (
                <div className="grid h-60 place-items-center text-sm text-muted-foreground">
                  {t("noRuleHits")}
                </div>
              )}
            </div>
          </div>
        </>
      ) : null}
    </section>
  );
}

function SummaryMetric({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <p className="text-lg font-semibold tabular-nums">{value}</p>
      <p className="mt-1 text-xs text-muted-foreground">{label}</p>
    </div>
  );
}

function SortableHeader({
  children,
  column,
}: {
  children: ReactNode;
  column: Column<QaTableRow>;
}) {
  const sorted = column.getIsSorted();
  const SortIcon =
    sorted === "asc" ? ArrowUp : sorted === "desc" ? ArrowDown : ArrowUpDown;

  return (
    <button
      className="flex h-8 items-center gap-1.5 rounded-sm text-left text-sm font-medium hover:text-foreground"
      onClick={column.getToggleSortingHandler()}
      type="button"
    >
      {children}
      <SortIcon className="size-4 text-muted-foreground" />
    </button>
  );
}

function getHeaderClassName(columnId: string) {
  if (columnId === "select") {
    return "w-10";
  }
  if (columnId === "name") {
    return "w-64";
  }
  if (columnId === "match") {
    return "w-48";
  }
  if (columnId === "type") {
    return "w-36";
  }
  if (columnId === "location") {
    return "w-24 text-right";
  }
  return undefined;
}

function getCellClassName(columnId: string) {
  return columnId === "location" ? "text-right" : undefined;
}

function getColumnLabel(
  columnId: string,
  t: (key: I18nKey, values?: Record<string, string | number>) => string,
) {
  const labels: Record<string, I18nKey> = {
    location: "location",
    last_interaction_at: "lastUsed",
    match: "ruleMatch",
    name: "name",
    path: "path",
    type: "type",
  };
  return labels[columnId] ? t(labels[columnId]) : columnId;
}

function formatRelativeTime(timestamp: number, language: string) {
  const divisions: Array<[number, Intl.RelativeTimeFormatUnit]> = [
    [60, "second"],
    [60, "minute"],
    [24, "hour"],
    [7, "day"],
    [4.345, "week"],
    [12, "month"],
    [Number.POSITIVE_INFINITY, "year"],
  ];
  let value = (timestamp - Date.now()) / 1_000;
  const formatter = new Intl.RelativeTimeFormat(language, { numeric: "auto" });

  for (const [amount, unit] of divisions) {
    if (Math.abs(value) < amount) {
      return formatter.format(Math.round(value), unit);
    }
    value /= amount;
  }
  return "";
}

function toQaType(value: unknown): QaType {
  return value === "frequent" ? "frequent" : "recent";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function cleanupResultMessage(
  result: QaBatchResult,
  t: (key: I18nKey, values?: Record<string, string | number>) => string,
) {
  if (result.failed.length > 0) {
    return t("removedItemsPartial", {
      failed: result.failed.length,
      succeeded: result.succeeded.length,
      total: result.total,
    });
  }
  if (result.skipped_protected.length > 0) {
    return t("cleanedItemsProtected", {
      protected: result.skipped_protected.length,
      succeeded: result.succeeded.length,
    });
  }
  return t("removedItems", { succeeded: result.succeeded.length });
}

function showCleanupToast(
  result: QaBatchResult,
  t: (key: I18nKey, values?: Record<string, string | number>) => string,
) {
  const message = cleanupResultMessage(result, t);
  if (result.succeeded.length > 0) {
    if (result.failed.length > 0 || result.skipped_protected.length > 0) {
      toast.warning(message);
    } else {
      toast.success(message);
    }
  } else if (result.failed.length > 0) {
    toast.error(t("failedRemoveItems", { failed: result.failed.length }));
  } else {
    toast.info(message);
  }

  if (result.history_error) {
    toast.warning(t("cleanupHistoryWarning"));
  }
}

function showRestoreToast(
  result: QaRestoreResult,
  t: (key: I18nKey, values?: Record<string, string | number>) => string,
) {
  if (result.success) {
    toast.success(t("restoredDefaults"));
    return;
  }

  toast.warning(t("partialRestore", { failed: getRestoreFailedCount(result) }));
}

function createOperationSummary({
  action,
  failed,
  message,
  succeeded,
  target,
  total,
}: Omit<OperationSummary, "updatedAt">): OperationSummary {
  return {
    action,
    failed,
    message,
    succeeded,
    target,
    total,
    updatedAt: new Date().toLocaleTimeString(),
  };
}

function createRestoreOperationSummary(
  result: QaRestoreResult,
  target: string,
  t: (key: I18nKey, values?: Record<string, string | number>) => string,
): OperationSummary {
  const sections = [result.recent, result.frequent].filter(
    (section): section is QaRestoreSectionResult => section !== null,
  );
  const succeeded = sections.filter((section) => section.success).length;
  const failed = sections.length - succeeded;
  return createOperationSummary({
    action: t("actionRestoreDefaults"),
    failed,
    message:
      failed > 0
        ? t("partialRestore", { failed })
        : t("restoredDefaults"),
    succeeded,
    target,
    total: sections.length,
  });
}

function createFailedOperationSummary(
  action: Exclude<PendingAction, null>,
  target: string,
  error: unknown,
  t: (key: I18nKey, values?: Record<string, string | number>) => string,
): OperationSummary {
  return createOperationSummary({
    action: getOperationLabel(action, t),
    failed: 1,
    message: errorMessage(error),
    succeeded: 0,
    target,
    total: 1,
  });
}

function getOperationLabel(
  action: Exclude<PendingAction, null>,
  t: (key: I18nKey, values?: Record<string, string | number>) => string,
) {
  if (action === "remove") {
    return t("actionRemoveSelected");
  }
  if (action === "empty") {
    return t("actionClear");
  }
  if (action === "smart") {
    return t("actionSmartClean");
  }
  return t("actionRestoreDefaults");
}

function getRestoreFailedCount(result: QaRestoreResult) {
  return [result.recent, result.frequent].filter(
    (section) => section && !section.success,
  ).length;
}

function getTableState({
  error,
  filteredCount,
  itemCount,
  loading,
  query,
  t,
}: {
  error: string | null;
  filteredCount: number;
  itemCount: number;
  loading: boolean;
  query: string;
  t: (key: I18nKey, values?: Record<string, string | number>) => string;
}) {
  if (loading) {
    return t("loadingItems");
  }
  if (error) {
    return error;
  }
  if (itemCount === 0) {
    return t("noItems");
  }
  if (query.trim() && filteredCount === 0) {
    return t("noMatches");
  }
  return null;
}
