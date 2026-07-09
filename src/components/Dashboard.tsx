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

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

type PendingAction = "remove" | "empty" | "restore-current" | "restore-all" | null;

const tabs: Array<{ value: QaType; label: string }> = [
  { value: "recent", label: "Recent Files" },
  { value: "frequent", label: "Frequent Folders" },
];

const emptyCounts: QaCounts = {
  recent: 0,
  frequent: 0,
  all: 0,
};

const columnLabels: Record<string, string> = {
  name: "Name",
  path: "Path",
  type: "Type",
  location: "Location",
};

export function Dashboard() {
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

  const tableData = useMemo<QaTableRow[]>(
    () =>
      filteredItems.map((item) => ({
        ...item,
        type: activeTab === "recent" ? "Recent File" : "Frequent Folder",
      })),
    [activeTab, filteredItems],
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
        header: "Select",
        cell: ({ row }) => (
          <Checkbox
            aria-label={`Select ${row.original.name}`}
            checked={selectedPaths.has(row.original.path)}
            onCheckedChange={(checked) =>
              togglePath(row.original.path, checked)
            }
          />
        ),
      },
      {
        accessorKey: "name",
        header: "Name",
        cell: ({ row }) => (
          <span className="block max-w-64 truncate font-medium">
            {row.original.name}
          </span>
        ),
      },
      {
        accessorKey: "path",
        header: "Path",
        cell: ({ row }) => (
          <span className="block max-w-[560px] truncate text-muted-foreground">
            {row.original.path}
          </span>
        ),
      },
      {
        accessorKey: "type",
        header: "Type",
        cell: ({ row }) => (
          <span className="whitespace-nowrap text-muted-foreground">
            {row.original.type}
          </span>
        ),
      },
      {
        id: "location",
        enableSorting: false,
        header: "Location",
        cell: ({ row }) => (
          <div className="text-right">
            <Button
              aria-label={`Open location for ${row.original.name}`}
              onClick={() => void openLocation(row.original.path)}
              size="icon-sm"
              title="Open location"
              type="button"
              variant="ghost"
            >
              <FolderOpen />
            </Button>
          </div>
        ),
      },
    ],
    [selectedPaths],
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
  const currentLabel = activeTab === "recent" ? "Recent Files" : "Frequent Folders";
  const currentCount = activeTab === "recent" ? counts.recent : counts.frequent;
  const actionsDisabled = loading || mutating || privacyActive;
  const removeDisabled = actionsDisabled || selectedCount === 0;
  const emptyDisabled = actionsDisabled || currentCount === 0;
  const pinDisabled = actionsDisabled || pinFolderPath.trim() === "";

  const openAction = async (action: Exclude<PendingAction, null>) => {
    try {
      if (await syncPrivacyState()) {
        toast.warning("Privacy mode is active; write operations are disabled.");
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
        toast.warning("Privacy mode is active; write operations are disabled.");
        return;
      }

      if (action === "remove") {
        const result = await invoke<QaBatchResult>("remove_qa_items", {
          qaType: activeTab,
          paths: Array.from(selectedPaths),
        });
        showRemoveToast(result);
        if (result.failed.length > 0) {
          void notifyPartialFailure(
            "Scourgify",
            `Removed ${result.succeeded.length} of ${result.total} item(s); ${result.failed.length} failed.`,
          );
        } else {
          void notifyOperationComplete(
            "Scourgify",
            `Removed ${result.succeeded.length} item(s).`,
          );
        }
      } else if (action === "empty") {
        await invoke("empty_qa_items", { qaType: activeTab });
        toast.success(`Cleared ${currentLabel}.`);
        void notifyOperationComplete("Scourgify", `Cleared ${currentLabel}.`);
      } else {
        const result = await invoke<QaRestoreResult>("restore_qa_defaults", {
          qaType: action === "restore-all" ? "all" : activeTab,
        });
        showRestoreToast(result);
        if (result.success) {
          void notifyOperationComplete("Scourgify", "Restored defaults.");
        } else {
          void notifyPartialFailure(
            "Scourgify",
            `Restored defaults with ${getRestoreFailedCount(result)} failed section(s).`,
          );
        }
      }

      await refresh();
    } catch (error) {
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
        toast.warning("Privacy mode is active; write operations are disabled.");
        return;
      }
      await invoke("pin_qa_folder", { path });
      setPinFolderPath("");
      toast.success("Pinned folder.");
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
            <div className="text-xs text-muted-foreground">Quick Access</div>
          </div>
        </SidebarHeader>
        <SidebarSeparator />
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupLabel>Navigation</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                <SidebarMenuItem>
                  <SidebarMenuButton render={<Link to="/" />}>
                    <Gauge />
                    <span>Dashboard</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
                <SidebarMenuItem>
                  <SidebarMenuButton render={<Link to="/settings" />}>
                    <Settings />
                    <span>Settings</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
                <SidebarMenuItem>
                  <SidebarMenuButton render={<Link to="/about" />}>
                    <Info />
                    <span>About</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
                <SidebarMenuItem>
                  <SidebarMenuButton
                    onClick={() => setConfigDrawerOpen(true)}
                    type="button"
                  >
                    <Paintbrush />
                    <span>Appearance</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
          <SidebarGroup>
            <SidebarGroupLabel>Counts</SidebarGroupLabel>
            <SidebarGroupContent>
              <div className="grid gap-1 px-2 text-xs text-muted-foreground">
                <span>Recent: {counts.recent}</span>
                <span>Frequent: {counts.frequent}</span>
                <span>Selected: {selectedPaths.size}</span>
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
            <p className="text-xs text-muted-foreground">Dashboard</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            aria-label="Open appearance drawer"
            onClick={() => setConfigDrawerOpen(true)}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            <SlidersHorizontal />
          </Button>
          <Button
            aria-label="Refresh Quick Access"
            disabled={loading}
            onClick={() => void refresh()}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            <RefreshCw className={loading ? "animate-spin" : ""} />
          </Button>
          <Button
            aria-label="Open settings"
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
            About
          </Link>
        </div>
      </header>

      <section className="grid gap-4 p-6 sm:grid-cols-2 lg:grid-cols-4">
        <CountCard label="Recent Files" value={counts.recent} />
        <CountCard label="Frequent Folders" value={counts.frequent} />
        <CountCard label="Visible" value={filteredItems.length} />
        <CountCard label="Selected" value={selectedPaths.size} />
      </section>

      <section className="grid gap-3 px-6 pb-6 md:grid-cols-[1fr_auto_auto]">
        <label className="min-w-0">
          <span className="sr-only">Folder path to pin</span>
          <Input
            disabled={actionsDisabled}
            onChange={(event) => setPinFolderPath(event.target.value)}
            placeholder="Folder path to pin"
            value={pinFolderPath}
          />
        </label>
        <Button
          disabled={actionsDisabled}
          onClick={() => void choosePinFolder()}
          type="button"
          variant="outline"
        >
          Browse
        </Button>
        <Button
          disabled={pinDisabled}
          onClick={() => void pinFolder()}
          type="button"
        >
          <FolderPlus />
          Pin folder
        </Button>
      </section>

      <section className="px-6 pb-6">
        <Tabs value={activeTab} onValueChange={switchTab}>
          <div className="flex flex-col gap-3 border-b pb-4 md:flex-row md:items-center md:justify-between">
            <div className="flex flex-col gap-3 md:flex-row md:items-center">
              <TabsList>
                {tabs.map((tab) => (
                  <TabsTrigger key={tab.value} value={tab.value}>
                    {tab.label}
                  </TabsTrigger>
                ))}
              </TabsList>
              <label className="relative w-full md:w-80">
                <span className="sr-only">Search Quick Access</span>
                <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <input
                  className="h-9 w-full rounded-md border bg-background pl-9 pr-3 text-sm outline-none transition-colors placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Search name or path"
                  type="search"
                  value={query}
                />
              </label>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {privacyActive ? (
                <span className="text-xs text-muted-foreground">
                  Privacy mode active
                </span>
              ) : null}
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button size="sm" type="button" variant="outline">
                      <Columns3 />
                      Columns
                    </Button>
                  }
                />
                <DropdownMenuContent align="end" className="w-48">
                  <DropdownMenuLabel>Visible columns</DropdownMenuLabel>
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
                        {columnLabels[column.id] ?? column.id}
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
                Remove selected
              </Button>
              <Button
                disabled={emptyDisabled}
                onClick={() => void openAction("empty")}
                size="sm"
                type="button"
                variant="destructive"
              >
                Clear {currentLabel}
              </Button>
              <Button
                disabled={actionsDisabled}
                onClick={() => void openAction("restore-current")}
                size="sm"
                type="button"
                variant="outline"
              >
                <RotateCcw />
                Restore {currentLabel}
              </Button>
              <Button
                disabled={actionsDisabled}
                onClick={() => void openAction("restore-all")}
                size="sm"
                type="button"
                variant="outline"
              >
                Restore All
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
                            aria-label="Select current page items"
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
                            Retry
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
                Page {pageCount === 0 ? 0 : pagination.pageIndex + 1} of{" "}
                {pageCount} / {filteredItems.length} result
                {filteredItems.length === 1 ? "" : "s"}
              </div>
              <div className="flex items-center gap-2">
                <span>Rows per page</span>
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
                  Previous
                </Button>
                <Button
                  disabled={!table.getCanNextPage()}
                  onClick={() => table.nextPage()}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  Next
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
        onClose={() => setPendingAction(null)}
        onConfirm={() => void confirmPendingAction()}
        selectedCount={selectedCount}
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
  onClose,
  onConfirm,
  selectedCount,
}: {
  action: PendingAction;
  currentLabel: string;
  onClose: () => void;
  onConfirm: () => void;
  selectedCount: number;
}) {
  const isEmpty = action === "empty";
  const isRestore = action === "restore-current" || action === "restore-all";
  const title = isEmpty
    ? `Clear ${currentLabel}?`
    : isRestore
      ? action === "restore-all"
        ? "Restore all defaults?"
        : `Restore ${currentLabel} defaults?`
      : "Remove selected items?";
  const description = isEmpty
    ? `This will clear ${currentLabel} from Quick Access.`
    : isRestore
      ? action === "restore-all"
        ? "This will restore Recent Files and Frequent Folders to their default state."
        : `This will restore ${currentLabel} to its default state.`
      : `This will remove ${selectedCount} selected item(s) from Quick Access.`;

  return (
    <AlertDialog open={action !== null} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>
            {description}
            {isEmpty && currentLabel === "Frequent Folders"
              ? " Windows may rebuild default Explorer folder entries after this operation."
              : ""}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            type="button"
            variant="destructive"
          >
            {isEmpty ? "Clear" : isRestore ? "Restore" : "Remove"}
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

function toQaType(value: unknown): QaType {
  return value === "frequent" ? "frequent" : "recent";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function showRemoveToast(result: QaBatchResult) {
  if (result.failed.length === 0) {
    toast.success(`Removed ${result.succeeded.length} item(s).`);
    return;
  }

  if (result.succeeded.length > 0) {
    toast.warning(
      `Removed ${result.succeeded.length} of ${result.total} item(s); ${result.failed.length} failed.`,
    );
    return;
  }

  toast.error(`Failed to remove ${result.failed.length} item(s).`);
}

function showRestoreToast(result: QaRestoreResult) {
  if (result.success) {
    toast.success("Restored defaults.");
    return;
  }

  toast.warning(
    `Restored defaults with ${getRestoreFailedCount(result)} failed section(s).`,
  );
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
}: {
  error: string | null;
  filteredCount: number;
  itemCount: number;
  loading: boolean;
  query: string;
}) {
  if (loading) {
    return "Loading Quick Access items...";
  }
  if (error) {
    return error;
  }
  if (itemCount === 0) {
    return "No Quick Access items found.";
  }
  if (query.trim() && filteredCount === 0) {
    return "No matching items found.";
  }
  return null;
}
