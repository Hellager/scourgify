import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { save as saveFile } from "@tauri-apps/plugin-dialog";
import {
  type Column,
  type ColumnDef,
  flexRender,
  getCoreRowModel,
  type PaginationState,
  type SortingState,
  useReactTable,
} from "@tanstack/react-table";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  ChevronLeft,
  ChevronRight,
  Download,
  LoaderCircle,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { CleanupRunsPanel } from "@/components/CleanupRunsPanel";
import { PageHeader } from "@/components/AppShell";
import {
  DatabaseRecoveryPanel,
  type DatabaseStatus,
} from "@/components/DatabaseRecoveryPanel";
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
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
import { type I18nKey, useI18n } from "@/lib/i18n";
import { REFRESH_HISTORY_EVENT } from "@/lib/app-events";
import { invokeCommand } from "@/lib/commands";

interface CleanRecord {
  id: number;
  run_id: number | null;
  item_path: string;
  item_type: "recent_file" | "frequent_folder";
  rule_id: number | null;
  rule_keyword: string | null;
  source: "manual" | "auto";
  cleaned_at: string;
}

interface CleanRecordPage {
  records: CleanRecord[];
  total: number;
  overall_total: number;
  page: number;
  page_size: number;
}

interface HistoryFilter {
  search: string;
  item_type: CleanRecord["item_type"] | null;
  matched_by_rule: boolean | null;
  source: "manual" | "auto" | null;
  run_id: number | null;
  date_range: "7d" | "30d" | null;
}

interface HistoryExportResult {
  count: number;
  path: string;
  format: HistoryExportFormat;
}

type HistoryExportFormat = "csv" | "json";
type HistoryExportScope = "filtered" | "all";

const EMPTY_HISTORY_FILTER: HistoryFilter = {
  search: "",
  item_type: null,
  matched_by_rule: null,
  source: null,
  run_id: null,
  date_range: null,
};

export function HistoryPage() {
  const { t } = useI18n();
  const [records, setRecords] = useState<CleanRecord[]>([]);
  const [total, setTotal] = useState(0);
  const [overallTotal, setOverallTotal] = useState(0);
  const [database, setDatabase] = useState<DatabaseStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [clearOpen, setClearOpen] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [historyView, setHistoryView] = useState<"items" | "runs">("items");
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  });
  const [sorting, setSorting] = useState<SortingState>([
    { id: "cleaned_at", desc: true },
  ]);
  const [search, setSearch] = useState("");
  const [itemType, setItemType] = useState<
    "all" | CleanRecord["item_type"]
  >("all");
  const [sourceFilter, setSourceFilter] = useState<"all" | "manual" | "auto">("all");
  const [ruleSource, setRuleSource] = useState<"all" | "matched" | "unmatched">("all");
  const [dateRange, setDateRange] = useState<"all" | "7d" | "30d">("all");
  const requestId = useRef(0);
  const refreshTriggerRef = useRef<HTMLButtonElement | null>(null);
  const clearTriggerRef = useRef<HTMLButtonElement | null>(null);

  const historyFilter = useMemo<HistoryFilter>(
    () => ({
      search,
      item_type: itemType === "all" ? null : itemType,
      matched_by_rule: ruleSource === "all" ? null : ruleSource === "matched",
      source: sourceFilter === "all" ? null : sourceFilter,
      run_id: null,
      date_range: dateRange === "all" ? null : dateRange,
    }),
    [dateRange, itemType, ruleSource, search, sourceFilter],
  );

  const columns = useMemo<ColumnDef<CleanRecord>[]>(
    () => [
      {
        accessorKey: "cleaned_at",
        header: t("cleanedAt"),
        cell: ({ row }) => formatCleanedAt(row.original.cleaned_at),
      },
      {
        accessorKey: "item_path",
        header: t("path"),
        cell: ({ row }) => (
          <span className="block max-w-xl truncate" title={row.original.item_path}>
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
          row.original.source === "manual" ? t("manualCleanup") : t("automatic"),
      },
      {
        accessorKey: "rule_keyword",
        header: t("sourceRule"),
        cell: ({ row }) => row.original.rule_keyword ?? t("noRule"),
      },
    ],
    [t],
  );

  const sortingRule = sorting[0] ?? { id: "cleaned_at", desc: true };
  const loadRecords = useCallback(async () => {
    const request = ++requestId.current;
    setLoading(true);
    setError(null);
    try {
      const databaseStatus = await invokeCommand<DatabaseStatus>("get_database_status");
      if (request !== requestId.current) {
        return;
      }
      setDatabase(databaseStatus);
      if (!databaseStatus.available) {
        setRecords([]);
        setTotal(0);
        setOverallTotal(0);
        return;
      }
      const result = await invokeCommand<CleanRecordPage>("get_clean_records", {
        query: {
          page: pagination.pageIndex + 1,
          page_size: pagination.pageSize,
          sort_by: sortingRule.id,
          sort_order: sortingRule.desc ? "desc" : "asc",
          ...historyFilter,
        },
      });
      if (request !== requestId.current) {
        return;
      }
      setOverallTotal(result.overall_total);
      const lastPageIndex = Math.max(
        0,
        Math.ceil(result.total / pagination.pageSize) - 1,
      );
      if (pagination.pageIndex > lastPageIndex) {
        setRecords([]);
        setTotal(result.total);
        setPagination((current) => ({ ...current, pageIndex: lastPageIndex }));
        return;
      }
      setRecords(result.records);
      setTotal(result.total);
    } catch (loadError) {
      if (request !== requestId.current) {
        return;
      }
      setRecords([]);
      setTotal(0);
      setOverallTotal(0);
      setError(errorMessage(loadError));
    } finally {
      if (request === requestId.current) {
        setLoading(false);
      }
    }
  }, [
    historyFilter,
    pagination.pageIndex,
    pagination.pageSize,
    sortingRule.desc,
    sortingRule.id,
  ]);

  useEffect(() => {
    void loadRecords();
  }, [loadRecords]);

  useEffect(() => {
    const refreshHistory = () => void loadRecords();
    window.addEventListener(REFRESH_HISTORY_EVENT, refreshHistory);
    return () =>
      window.removeEventListener(REFRESH_HISTORY_EVENT, refreshHistory);
  }, [loadRecords]);

  const table = useReactTable({
    columns,
    data: records,
    getCoreRowModel: getCoreRowModel(),
    enableMultiSort: false,
    manualPagination: true,
    manualSorting: true,
    onPaginationChange: setPagination,
    onSortingChange: (next) => {
      setSorting(next);
      setPagination((current) => ({ ...current, pageIndex: 0 }));
    },
    pageCount: Math.ceil(total / pagination.pageSize),
    state: { pagination, sorting },
  });

  const clearHistory = async () => {
    setClearing(true);
    try {
      await invokeCommand("clear_clean_records");
      setClearOpen(false);
      setRecords([]);
      setTotal(0);
      setOverallTotal(0);
      setPagination((current) => ({ ...current, pageIndex: 0 }));
      toast.success(t("historyCleared"));
    } catch (clearError) {
      toast.error(errorMessage(clearError));
    } finally {
      setClearing(false);
    }
  };

  const exportHistory = async (
    format: HistoryExportFormat,
    scope: HistoryExportScope,
  ) => {
    setExporting(true);
    try {
      const path = await saveFile({
        defaultPath: exportFileName(format),
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
        title: t("exportHistory"),
      });
      if (!path) {
        return;
      }
      const result = await invokeCommand<HistoryExportResult>("export_clean_records", {
        path,
        format,
        filter: scope === "filtered" ? historyFilter : EMPTY_HISTORY_FILTER,
      });
      toast.success(t("historyExported", { count: result.count }), {
        action: {
          label: t("openFolder"),
          onClick: () => {
            void invokeCommand("open_in_explorer", { path: result.path }).catch((openError) =>
              toast.error(errorMessage(openError)),
            );
          },
        },
      });
    } catch (exportError) {
      toast.error(errorMessage(exportError));
    } finally {
      setExporting(false);
    }
  };

  const pageCount = table.getPageCount();
  const clearDisabled =
    loading || clearing || overallTotal === 0 || database?.available !== true;
  const exportDisabled =
    loading || exporting || database?.available !== true;

  return (
    <>
      <PageHeader
        actions={
          historyView === "items" ? <>
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    disabled={exportDisabled}
                    type="button"
                    variant="outline"
                  >
                    {exporting ? (
                      <LoaderCircle className="animate-spin" />
                    ) : (
                      <Download />
                    )}
                    {t(exporting ? "exportingHistory" : "exportHistory")}
                  </Button>
                }
              />
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>
                  {t("exportCurrentFilters")}
                </DropdownMenuLabel>
                <DropdownMenuItem
                  onSelect={() => void exportHistory("csv", "filtered")}
                >
                  CSV
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => void exportHistory("json", "filtered")}
                >
                  JSON
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>{t("exportAllRecords")}</DropdownMenuLabel>
                <DropdownMenuItem
                  onSelect={() => void exportHistory("csv", "all")}
                >
                  CSV
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => void exportHistory("json", "all")}
                >
                  JSON
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <Button
              aria-label={t("refreshHistory")}
              disabled={loading}
              onClick={() => void loadRecords()}
              ref={refreshTriggerRef}
              size="icon-sm"
              title={t("refreshHistory")}
              type="button"
              variant="outline"
            >
              <RefreshCw className={loading ? "animate-spin" : ""} />
            </Button>
            <Button
              disabled={clearDisabled}
              onClick={() => setClearOpen(true)}
              ref={clearTriggerRef}
              type="button"
              variant="destructive"
            >
              <Trash2 />
              {t("clearHistory")}
            </Button>
          </> : undefined
        }
        subtitle={t("historySubtitle")}
        title={t("history")}
      />

      <div className="mx-auto grid max-w-7xl gap-4 p-6">
        <div className="inline-flex w-fit rounded-md border p-0.5" role="group">
          <Button
            onClick={() => setHistoryView("items")}
            size="sm"
            type="button"
            variant={historyView === "items" ? "secondary" : "ghost"}
          >
            {t("historyItems")}
          </Button>
          <Button
            onClick={() => setHistoryView("runs")}
            size="sm"
            type="button"
            variant={historyView === "runs" ? "secondary" : "ghost"}
          >
            {t("cleanupRuns")}
          </Button>
        </div>

        {database && !database.available ? (
          <DatabaseRecoveryPanel
            onRecovered={loadRecords}
            onStatusChange={setDatabase}
            status={database}
          />
        ) : null}

        {historyView === "items" && error ? (
          <section className="flex items-center justify-between gap-4 border-l-2 border-destructive px-4 py-2">
            <p className="min-w-0 break-words text-sm text-destructive">
              {error}
            </p>
            <Button
              onClick={() => void loadRecords()}
              size="sm"
              type="button"
              variant="outline"
            >
              {t("refreshHistory")}
            </Button>
          </section>
        ) : null}

        {historyView === "runs" ? (
          <CleanupRunsPanel />
        ) : (
          <>
        <section className="grid gap-3" aria-label={t("searchHistory")}>
          <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_11rem_10rem_10rem_9rem]">
            <label className="relative min-w-0">
              <span className="sr-only">{t("searchHistory")}</span>
              <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="pl-9"
                onChange={(event) => {
                  setSearch(event.target.value);
                  setPagination((current) => ({ ...current, pageIndex: 0 }));
                }}
                placeholder={t("searchHistory")}
                type="search"
                value={search}
              />
            </label>
            <Select
              onValueChange={(value) => {
                setItemType(value as "all" | CleanRecord["item_type"]);
                setPagination((current) => ({ ...current, pageIndex: 0 }));
              }}
              value={itemType}
            >
              <SelectTrigger aria-label={t("type")} className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("all")}</SelectItem>
                <SelectItem value="recent_file">{t("recentFile")}</SelectItem>
                <SelectItem value="frequent_folder">
                  {t("frequentFolder")}
                </SelectItem>
              </SelectContent>
            </Select>
              <Select
                onValueChange={(value) => {
                  setSourceFilter(value as "all" | "manual" | "auto");
                  setPagination((current) => ({ ...current, pageIndex: 0 }));
                }}
                value={sourceFilter}
              >
                <SelectTrigger aria-label={t("cleanupSource")} className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t("all")}</SelectItem>
                  <SelectItem value="manual">{t("manualCleanup")}</SelectItem>
                  <SelectItem value="auto">{t("automatic")}</SelectItem>
                </SelectContent>
              </Select>
              <Select
                onValueChange={(value) => {
                setRuleSource(value as "all" | "matched" | "unmatched");
                setPagination((current) => ({ ...current, pageIndex: 0 }));
              }}
                value={ruleSource}
              >
                <SelectTrigger aria-label={t("ruleMatch")} className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t("all")}</SelectItem>
                  <SelectItem value="matched">{t("ruleMatched")}</SelectItem>
                  <SelectItem value="unmatched">{t("ruleUnmatched")}</SelectItem>
                </SelectContent>
            </Select>
            <Select
              onValueChange={(value) => {
                setDateRange(value as "all" | "7d" | "30d");
                setPagination((current) => ({ ...current, pageIndex: 0 }));
              }}
              value={dateRange}
            >
              <SelectTrigger aria-label={t("dateRange")} className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("all")}</SelectItem>
                <SelectItem value="7d">{t("last7Days")}</SelectItem>
                <SelectItem value="30d">{t("last30Days")}</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </section>

        <section aria-labelledby="history-list-title" className="min-w-0">
          <div className="mb-3 flex items-center justify-between gap-4">
            <h2 className="text-sm font-semibold" id="history-list-title">
              {t("history")}
            </h2>
            <span className="text-sm tabular-nums text-muted-foreground">
              {total === overallTotal
                ? t("historyCount", { count: overallTotal })
                : `${t("historyCount", { count: total })} / ${overallTotal}`}
            </span>
          </div>
          <div className="overflow-x-auto rounded-md border">
            <Table>
              <TableHeader>
                {table.getHeaderGroups().map((headerGroup) => (
                  <TableRow key={headerGroup.id}>
                    {headerGroup.headers.map((header) => (
                      <TableHead key={header.id}>
                        {header.isPlaceholder ? null : (
                          <SortableHeader column={header.column}>
                            {flexRender(
                              header.column.columnDef.header,
                              header.getContext(),
                            )}
                          </SortableHeader>
                        )}
                      </TableHead>
                    ))}
                  </TableRow>
                ))}
              </TableHeader>
              <TableBody>
                {loading ? (
                  <HistoryTableMessage message={t("loadingHistory")} />
                ) : records.length === 0 ? (
                  <HistoryTableMessage
                    message={
                      search ||
                      itemType !== "all" ||
                      sourceFilter !== "all" ||
                      ruleSource !== "all" ||
                      dateRange !== "all"
                        ? t("noMatches")
                        : t("noHistory")
                    }
                  />
                ) : (
                  table.getRowModel().rows.map((row) => (
                    <TableRow key={row.id}>
                      {row.getVisibleCells().map((cell) => (
                        <TableCell key={cell.id}>
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </TableCell>
                      ))}
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
          <div className="flex flex-col gap-3 py-4 text-sm text-muted-foreground sm:flex-row sm:items-center sm:justify-between">
            <span>
              {t("pageStatus", {
                count: total,
                page: pageCount === 0 ? 0 : pagination.pageIndex + 1,
                pageCount,
              })}
            </span>
            <div className="flex flex-wrap items-center gap-2">
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
                disabled={!table.getCanPreviousPage() || loading}
                onClick={() => table.previousPage()}
                size="sm"
                type="button"
                variant="outline"
              >
                <ChevronLeft />
                {t("previous")}
              </Button>
              <Button
                disabled={!table.getCanNextPage() || loading}
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
        </section>
          </>
        )}
      </div>

      <AlertDialog open={clearOpen} onOpenChange={setClearOpen}>
        <AlertDialogContent
          finalFocus={() =>
            clearTriggerRef.current && !clearTriggerRef.current.disabled
              ? clearTriggerRef.current
              : refreshTriggerRef.current
          }
        >
          <AlertDialogHeader>
            <AlertDialogTitle>{t("clearHistoryQuestion")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("clearHistoryDescription", { count: overallTotal })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={clearing}
              onClick={() => void clearHistory()}
              type="button"
              variant="destructive"
            >
              {t("clearHistory")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function SortableHeader({
  children,
  column,
}: {
  children: ReactNode;
  column: Column<CleanRecord>;
}) {
  const sorted = column.getIsSorted();
  const SortIcon =
    sorted === "asc" ? ArrowUp : sorted === "desc" ? ArrowDown : ArrowUpDown;
  return (
    <button
      className="flex h-8 items-center gap-1.5 text-left text-sm font-medium"
      onClick={column.getToggleSortingHandler()}
      type="button"
    >
      {children}
      <SortIcon className="size-4 text-muted-foreground" />
    </button>
  );
}

function HistoryTableMessage({ message }: { message: string }) {
  return (
    <TableRow>
      <TableCell className="h-40 text-center text-muted-foreground" colSpan={5}>
        {message}
      </TableCell>
    </TableRow>
  );
}

function formatCleanedAt(value: string) {
  const date = new Date(`${value.replace(" ", "T")}Z`);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function exportFileName(format: HistoryExportFormat) {
  const now = new Date();
  const date = [now.getFullYear(), now.getMonth() + 1, now.getDate()]
    .map((part) => String(part).padStart(2, "0"))
    .join("-");
  return `scourgify-history-${date}.${format}`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
