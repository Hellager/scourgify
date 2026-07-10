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
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
} from "recharts";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  ChevronLeft,
  ChevronRight,
  Columns3,
  Gauge,
  FolderOpen,
  FolderPlus,
  Info,
  Paintbrush,
  RefreshCw,
  RotateCcw,
  Search,
  Settings,
  SlidersHorizontal,
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
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarSeparator,
  SidebarTrigger,
} from "@/components/ui/sidebar";
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
  OPEN_CONFIG_DRAWER_EVENT,
  REFRESH_DASHBOARD_EVENT,
} from "@/lib/app-events";
import {
  configSchema,
  defaultConfig,
  type ConfigForm,
  type SidebarVariant,
} from "@/lib/config";
import { type I18nKey, useI18n } from "@/lib/i18n";
import { ConfigDrawer } from "@/components/ConfigDrawer";

type QaType = "recent" | "frequent";

interface QaItem {
  path: string;
  name: string;
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

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

type PendingAction = "remove" | "empty" | "restore-current" | "restore-all" | null;

const emptyCounts: QaCounts = {
  recent: 0,
  frequent: 0,
  all: 0,
};

const quickAccessChartColors = ["#2563eb", "#16a34a"];

export function Dashboard() {
  const { t } = useI18n();
  const [config, setConfig] = useState<ConfigForm>(defaultConfig);
  const [activeTab, setActiveTab] = useState<QaType>("recent");
  const [counts, setCounts] = useState<QaCounts>(emptyCounts);
  const [items, setItems] = useState<QaItem[]>([]);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [privacyActive, setPrivacyActive] = useState(false);
  const [pendingAction, setPendingAction] = useState<PendingAction>(null);
  const [pinFolderPath, setPinFolderPath] = useState("");
  const [configDrawerOpen, setConfigDrawerOpen] = useState(false);
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
    [selectedPaths, t],
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
    enableRowSelection: true,
    getCoreRowModel: getCoreRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    getRowId: (row) => row.path,
    getSortedRowModel: getSortedRowModel(),
    onColumnVisibilityChange: setColumnVisibility,
    onPaginationChange: setPagination,
    onSortingChange: setSorting,
  });

  const pageRows = table.getRowModel().rows;
  const allPageSelected =
    pageRows.length > 0 && pageRows.every((row) => selectedPaths.has(row.id));
  const somePageSelected =
    pageRows.some((row) => selectedPaths.has(row.id)) && !allPageSelected;
  const visibleColumnCount = table.getVisibleLeafColumns().length;
  const pageCount = table.getPageCount();

  const loadCounts = useCallback(async () => {
    setCounts(await invoke<QaCounts>("get_qa_counts"));
  }, []);

  const loadConfig = useCallback(async () => {
    try {
      setConfig(configSchema.parse(await invoke<ConfigForm>("get_config")));
    } catch (error) {
      toast.error(errorMessage(error));
    }
  }, []);

  const loadItems = useCallback(async (qaType: QaType) => {
    setLoading(true);
    setError(null);
    try {
      setItems(await invoke<QaItem[]>("list_qa_items", { qaType }));
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

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    void loadConfig();
  }, [loadConfig]);

  useEffect(() => {
    setPagination((current) =>
      current.pageIndex === 0 ? current : { ...current, pageIndex: 0 },
    );
  }, [activeTab, query, items.length]);

  useEffect(() => {
    const openConfigDrawer = () => setConfigDrawerOpen(true);
    const refreshDashboard = () => void refresh();

    window.addEventListener(OPEN_CONFIG_DRAWER_EVENT, openConfigDrawer);
    window.addEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
    return () => {
      window.removeEventListener(OPEN_CONFIG_DRAWER_EVENT, openConfigDrawer);
      window.removeEventListener(REFRESH_DASHBOARD_EVENT, refreshDashboard);
    };
  }, [refresh]);

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
      for (const row of pageRows) {
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
  const currentCount = activeTab === "recent" ? counts.recent : counts.frequent;
  const actionsDisabled = loading || mutating || privacyActive;
  const removeDisabled = actionsDisabled || selectedCount === 0;
  const emptyDisabled = actionsDisabled || currentCount === 0;
  const pinDisabled = actionsDisabled || pinFolderPath.trim() === "";

  const openAction = async (action: Exclude<PendingAction, null>) => {
    try {
      if (await syncPrivacyState()) {
        toast.warning(t("privacyWriteDisabled"));
        return;
      }
      setPendingAction(action);
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const confirmPendingAction = async () => {
    if (!pendingAction) {
      return;
    }

    const action = pendingAction;
    setPendingAction(null);
    setMutating(true);
    try {
      if (await syncPrivacyState()) {
        toast.warning(t("privacyWriteDisabled"));
        return;
      }

      if (action === "remove") {
        const result = await invoke<QaBatchResult>("remove_qa_items", {
          qaType: activeTab,
          paths: Array.from(selectedPaths),
        });
        setLastOperationSummary(
          createOperationSummary({
            action: t("actionRemoveSelected"),
            failed: result.failed.length,
            message:
              result.failed.length > 0
                ? t("removedItemsPartial", {
                    failed: result.failed.length,
                    succeeded: result.succeeded.length,
                    total: result.total,
                  })
                : t("removedItems", { succeeded: result.succeeded.length }),
            succeeded: result.succeeded.length,
            target: currentLabel,
            total: result.total,
          }),
        );
        showRemoveToast(result, t);
        if (result.failed.length > 0) {
          void notifyPartialFailure(
            "Scourgify",
            t("removedItemsPartial", {
              failed: result.failed.length,
              succeeded: result.succeeded.length,
              total: result.total,
            }),
          );
        } else {
          void notifyOperationComplete(
            "Scourgify",
            t("removedItems", { succeeded: result.succeeded.length }),
          );
        }
      } else if (action === "empty") {
        const total = currentCount;
        await invoke("empty_qa_items", { qaType: activeTab });
        setLastOperationSummary(
          createOperationSummary({
            action: t("actionClear"),
            failed: 0,
            message: t("clearedLabel", { label: currentLabel }),
            succeeded: total,
            target: currentLabel,
            total,
          }),
        );
        toast.success(t("clearedLabel", { label: currentLabel }));
        void notifyOperationComplete(
          "Scourgify",
          t("clearedLabel", { label: currentLabel }),
        );
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

      await refresh();
    } catch (error) {
      setLastOperationSummary(
        createFailedOperationSummary(action, currentLabel, error, t),
      );
      toast.error(errorMessage(error));
    } finally {
      setMutating(false);
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
    <SidebarProvider>
      <Sidebar
        collapsible="icon"
        variant={config.sidebar_variant as SidebarVariant}
      >
        <SidebarHeader>
          <div className="px-2 py-1">
            <div className="text-sm font-semibold">Scourgify</div>
            <div className="text-xs text-muted-foreground">
              {t("quickAccess")}
            </div>
          </div>
        </SidebarHeader>
        <SidebarSeparator />
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupLabel>{t("commandNavigation")}</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                <SidebarMenuItem>
                  <SidebarMenuButton render={<Link to="/" />}>
                    <Gauge />
                    <span>{t("dashboard")}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
                <SidebarMenuItem>
                  <SidebarMenuButton render={<Link to="/settings" />}>
                    <Settings />
                    <span>{t("settings")}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
                <SidebarMenuItem>
                  <SidebarMenuButton render={<Link to="/about" />}>
                    <Info />
                    <span>{t("about")}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
                <SidebarMenuItem>
                  <SidebarMenuButton
                    onClick={() => setConfigDrawerOpen(true)}
                    type="button"
                  >
                    <Paintbrush />
                    <span>{t("appearance")}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
          <SidebarGroup>
            <SidebarGroupLabel>{t("counts")}</SidebarGroupLabel>
            <SidebarGroupContent>
              <div className="grid gap-1 px-2 text-xs text-muted-foreground">
                <span>
                  {t("recent")}: {counts.recent}
                </span>
                <span>
                  {t("frequent")}: {counts.frequent}
                </span>
                <span>
                  {t("selected")}: {selectedPaths.size}
                </span>
              </div>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
      </Sidebar>
      <SidebarInset className="min-h-screen bg-background text-foreground">
      <header className="flex h-14 items-center justify-between border-b px-6">
        <div className="flex items-center gap-3">
          <SidebarTrigger />
          <div>
            <h1 className="text-base font-semibold">Scourgify</h1>
            <p className="text-xs text-muted-foreground">{t("dashboard")}</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            aria-label={t("openAppearanceDrawer")}
            onClick={() => setConfigDrawerOpen(true)}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            <SlidersHorizontal />
          </Button>
          <Button
            aria-label={t("refreshQuickAccess")}
            disabled={loading}
            onClick={() => void refresh()}
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
        </div>
      </header>

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
                            disabled={pageRows.length === 0}
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
        t={t}
      />
      <ConfigDrawer
        config={config}
        onConfigSaved={setConfig}
        onOpenChange={setConfigDrawerOpen}
        open={configDrawerOpen}
        privacyActive={privacyActive}
      />
      </SidebarInset>
    </SidebarProvider>
  );
}

function ConfirmActionDialog({
  action,
  currentLabel,
  isFrequent,
  onClose,
  onConfirm,
  selectedCount,
  t,
}: {
  action: PendingAction;
  currentLabel: string;
  isFrequent: boolean;
  onClose: () => void;
  onConfirm: () => void;
  selectedCount: number;
  t: (key: I18nKey, values?: Record<string, string | number>) => string;
}) {
  const isEmpty = action === "empty";
  const isRestore = action === "restore-current" || action === "restore-all";
  const title = isEmpty
    ? t("clearCurrentQuestion", { label: currentLabel })
    : isRestore
      ? action === "restore-all"
        ? t("restoreAllDefaultsQuestion")
        : t("restoreCurrentDefaultsQuestion", { label: currentLabel })
      : t("removeSelectedQuestion");
  const description = isEmpty
    ? t("emptyCurrentDescription", { label: currentLabel })
    : isRestore
      ? action === "restore-all"
        ? t("restoreAllDescription")
        : t("restoreCurrentDescription", { label: currentLabel })
      : t("removeSelectedDescription", { count: selectedCount });

  return (
    <AlertDialog open={action !== null} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>
            {description}
            {isEmpty && isFrequent
              ? t("emptyFrequentWarning")
              : ""}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            type="button"
            variant="destructive"
          >
            {isEmpty ? t("actionClear") : isRestore ? t("restore") : t("actionRemoveSelected")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
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
    name: "name",
    path: "path",
    type: "type",
  };
  return labels[columnId] ? t(labels[columnId]) : columnId;
}

function toQaType(value: unknown): QaType {
  return value === "frequent" ? "frequent" : "recent";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function showRemoveToast(
  result: QaBatchResult,
  t: (key: I18nKey, values?: Record<string, string | number>) => string,
) {
  if (result.failed.length === 0) {
    toast.success(t("removedItems", { succeeded: result.succeeded.length }));
    return;
  }

  if (result.succeeded.length > 0) {
    toast.warning(
      t("removedItemsPartial", {
        failed: result.failed.length,
        succeeded: result.succeeded.length,
        total: result.total,
      }),
    );
    return;
  }

  toast.error(t("failedRemoveItems", { failed: result.failed.length }));
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
