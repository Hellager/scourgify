import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
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
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
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

interface CleanRecord {
  id: number;
  item_path: string;
  item_type: "recent_file" | "frequent_folder";
  rule_id: number | null;
  rule_keyword: string | null;
  cleaned_at: string;
}

interface CleanRecordPage {
  records: CleanRecord[];
  total: number;
  overall_total: number;
  page: number;
  page_size: number;
}

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

export function HistoryPage() {
  const { t } = useI18n();
  const [records, setRecords] = useState<CleanRecord[]>([]);
  const [total, setTotal] = useState(0);
  const [overallTotal, setOverallTotal] = useState(0);
  const [database, setDatabase] = useState<DatabaseStatus | null>(null);
  const [privacyActive, setPrivacyActive] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [clearOpen, setClearOpen] = useState(false);
  const [clearing, setClearing] = useState(false);
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
  const [ruleSource, setRuleSource] = useState<"all" | "manual" | "matched">(
    "all",
  );
  const [dateRange, setDateRange] = useState<"all" | "7d" | "30d">("all");
  const requestId = useRef(0);
  const refreshTriggerRef = useRef<HTMLButtonElement | null>(null);
  const clearTriggerRef = useRef<HTMLButtonElement | null>(null);

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
      const [databaseStatus, privacyState] = await Promise.all([
        invoke<DatabaseStatus>("get_database_status"),
        invoke<PrivacyState>("privacy_state"),
      ]);
      if (request !== requestId.current) {
        return;
      }
      setDatabase(databaseStatus);
      setPrivacyActive(privacyState !== "Inactive");
      if (!databaseStatus.available) {
        setRecords([]);
        setTotal(0);
        setOverallTotal(0);
        return;
      }
      const result = await invoke<CleanRecordPage>("get_clean_records", {
        query: {
          page: pagination.pageIndex + 1,
          page_size: pagination.pageSize,
          sort_by: sortingRule.id,
          sort_order: sortingRule.desc ? "desc" : "asc",
          search,
          item_type: itemType === "all" ? null : itemType,
          matched_by_rule:
            ruleSource === "all" ? null : ruleSource === "matched",
          date_range: dateRange === "all" ? null : dateRange,
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
    dateRange,
    itemType,
    pagination.pageIndex,
    pagination.pageSize,
    ruleSource,
    search,
    sortingRule.desc,
    sortingRule.id,
  ]);

  useEffect(() => {
    void loadRecords();
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
      await invoke("clear_clean_records");
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

  const pageCount = table.getPageCount();
  const clearDisabled =
    loading ||
    clearing ||
    overallTotal === 0 ||
    database?.available !== true ||
    privacyActive;

  return (
    <>
      <PageHeader
        actions={
          <>
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
          </>
        }
        subtitle={t("historySubtitle")}
        title={t("history")}
      />

      <div className="mx-auto grid max-w-7xl gap-4 p-6">
        {database && !database.available ? (
          <DatabaseRecoveryPanel
            onRecovered={loadRecords}
            onStatusChange={setDatabase}
            status={database}
          />
        ) : null}

        {privacyActive ? (
          <section className="border-l-2 border-border px-4 py-2">
            <h2 className="text-sm font-semibold">{t("privacyActive")}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("historyClearPrivacyDisabled")}
            </p>
          </section>
        ) : null}

        {error ? (
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

        <section className="grid gap-3" aria-label={t("searchHistory")}>
          <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_12rem_12rem_10rem]">
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
                setRuleSource(value as "all" | "manual" | "matched");
                setPagination((current) => ({ ...current, pageIndex: 0 }));
              }}
              value={ruleSource}
            >
              <SelectTrigger aria-label={t("sourceRule")} className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("all")}</SelectItem>
                <SelectItem value="manual">{t("manualCleanup")}</SelectItem>
                <SelectItem value="matched">{t("ruleMatched")}</SelectItem>
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
              {t("historyCount", { count: total })}
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
      <TableCell className="h-40 text-center text-muted-foreground" colSpan={4}>
        {message}
      </TableCell>
    </TableRow>
  );
}

function formatCleanedAt(value: string) {
  const date = new Date(`${value.replace(" ", "T")}Z`);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
