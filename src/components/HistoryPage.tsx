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
  type ColumnFiltersState,
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
  Eye,
  EyeOff,
  FilterX,
  ListFilter,
  LoaderCircle,
  MoreHorizontal,
  Search,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
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
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
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
  REFRESH_DASHBOARD_EVENT,
  REFRESH_HISTORY_EVENT,
} from "@/lib/app-events";
import { type ActionReceipt, invokeCommand } from "@/lib/commands";
import { useI18n } from "@/lib/i18n";

interface CleanRecord {
  id: number;
  run_id: number | null;
  item_path: string;
  item_type: "recent_file" | "frequent_folder";
  rule_id: number | null;
  rule_keyword: string | null;
  source: "manual" | "auto";
  cleaned_at: string;
  visible: boolean;
}

interface CleanRecordPage {
  records: CleanRecord[];
  total: number;
  overall_total: number;
  hidden_total: number;
  page: number;
  page_size: number;
}

interface HistoryFilter {
  search: string;
  item_type: CleanRecord["item_type"] | null;
  matched_by_rule: boolean | null;
  source: CleanRecord["source"] | null;
  run_id: number | null;
  date_range: "7d" | "30d" | null;
}

interface HistoryExportResult {
  count: number;
  path: string;
  format: "csv";
}

interface FilterOption {
  label: string;
  value: string;
}

const HISTORY_PAGE_SIZE = 10;
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
  const [hiddenTotal, setHiddenTotal] = useState(0);
  const [database, setDatabase] = useState<DatabaseStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [clearOpen, setClearOpen] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<CleanRecord | null>(null);
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const [hidingId, setHidingId] = useState<number | null>(null);
  const [restoringHidden, setRestoringHidden] = useState(false);
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: HISTORY_PAGE_SIZE,
  });
  const [sorting, setSorting] = useState<SortingState>([
    { id: "cleaned_at", desc: true },
  ]);
  const requestId = useRef(0);
  const actionsTriggerRef = useRef<HTMLButtonElement | null>(null);
  const deleteTriggerRef = useRef<HTMLButtonElement | null>(null);

  const historyFilter = useMemo<HistoryFilter>(() => {
    const itemType = getFilterValue(columnFilters, "item_type");
    const source = getFilterValue(columnFilters, "source");
    const ruleMatch = getFilterValue(columnFilters, "rule_keyword");
    return {
      ...EMPTY_HISTORY_FILTER,
      search: getFilterValue(columnFilters, "item_path") ?? "",
      item_type:
        itemType === "recent_file" || itemType === "frequent_folder"
          ? itemType
          : null,
      source: source === "manual" || source === "auto" ? source : null,
      matched_by_rule:
        ruleMatch === "matched"
          ? true
          : ruleMatch === "unmatched"
            ? false
            : null,
    };
  }, [columnFilters]);

  const sortingRule = sorting[0] ?? { id: "cleaned_at", desc: true };
  const loadRecords = useCallback(async () => {
    const request = ++requestId.current;
    setLoading(true);
    setError(null);
    try {
      const databaseStatus =
        await invokeCommand<DatabaseStatus>("get_database_status");
      if (request !== requestId.current) {
        return;
      }
      setDatabase(databaseStatus);
      if (!databaseStatus.available) {
        setRecords([]);
        setTotal(0);
        setOverallTotal(0);
        setHiddenTotal(0);
        return;
      }

      const result = await invokeCommand<CleanRecordPage>("get_clean_records", {
        query: {
          page: pagination.pageIndex + 1,
          page_size: HISTORY_PAGE_SIZE,
          sort_by: sortingRule.id,
          sort_order: sortingRule.desc ? "desc" : "asc",
          ...historyFilter,
        },
      });
      if (request !== requestId.current) {
        return;
      }

      setOverallTotal(result.overall_total);
      setHiddenTotal(result.hidden_total);
      const lastPageIndex = Math.max(
        0,
        Math.ceil(result.total / HISTORY_PAGE_SIZE) - 1,
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
      setHiddenTotal(0);
      setError(errorMessage(loadError));
    } finally {
      if (request === requestId.current) {
        setLoading(false);
      }
    }
  }, [historyFilter, pagination.pageIndex, sortingRule.desc, sortingRule.id]);

  useEffect(() => {
    void loadRecords();
  }, [loadRecords]);

  useEffect(() => {
    const refreshHistory = () => void loadRecords();
    window.addEventListener(REFRESH_HISTORY_EVENT, refreshHistory);
    return () =>
      window.removeEventListener(REFRESH_HISTORY_EVENT, refreshHistory);
  }, [loadRecords]);

  const clearHistory = async () => {
    setClearing(true);
    try {
      await invokeCommand("clear_clean_records");
      setClearOpen(false);
      setRecords([]);
      setTotal(0);
      setOverallTotal(0);
      setHiddenTotal(0);
      setPagination((current) => ({ ...current, pageIndex: 0 }));
      dispatchAppEvent(REFRESH_DASHBOARD_EVENT, { fresh: false });
      toast.success(t("historyCleared"));
    } catch (clearError) {
      toast.error(errorMessage(clearError));
    } finally {
      setClearing(false);
    }
  };

  const deleteRecord = async () => {
    if (!pendingDelete) {
      return;
    }
    const record = pendingDelete;
    setDeletingId(record.id);
    try {
      await invokeCommand("remove_clean_record", { id: record.id });
      setPendingDelete(null);
      await loadRecords();
      dispatchAppEvent(REFRESH_DASHBOARD_EVENT, { fresh: false });
      toast.success(t("historyRecordDeleted"));
    } catch (deleteError) {
      toast.error(errorMessage(deleteError));
    } finally {
      setDeletingId(null);
    }
  };

  const hideRecord = useCallback(
    async (record: CleanRecord) => {
      setHidingId(record.id);
      try {
        await invokeCommand("set_clean_record_visibility", {
          id: record.id,
          visible: false,
        });
        await loadRecords();
        dispatchAppEvent(REFRESH_DASHBOARD_EVENT, { fresh: false });
        toast.success(t("historyRecordHidden"));
      } catch (hideError) {
        toast.error(errorMessage(hideError));
      } finally {
        setHidingId(null);
      }
    },
    [loadRecords, t],
  );

  const restoreHiddenRecords = useCallback(async () => {
    setRestoringHidden(true);
    try {
      const result = await invokeCommand<ActionReceipt>(
        "restore_hidden_clean_records",
      );
      await loadRecords();
      dispatchAppEvent(REFRESH_DASHBOARD_EVENT, { fresh: false });
      toast.success(t("hiddenHistoryRestored", { count: result.affected }));
    } catch (restoreError) {
      toast.error(errorMessage(restoreError));
    } finally {
      setRestoringHidden(false);
    }
  }, [loadRecords, t]);

  const exportHistory = useCallback(async () => {
    setExporting(true);
    try {
      const path = await saveFile({
        defaultPath: exportFileName(),
        filters: [{ name: "CSV", extensions: ["csv"] }],
        title: t("exportHistory"),
      });
      if (!path) {
        return;
      }
      const result = await invokeCommand<HistoryExportResult>(
        "export_clean_records",
        { path, format: "csv", filter: EMPTY_HISTORY_FILTER },
      );
      toast.success(t("historyExported", { count: result.count }), {
        action: {
          label: t("openFolder"),
          onClick: () => {
            void invokeCommand("open_in_explorer", { path: result.path }).catch(
              (openError) => toast.error(errorMessage(openError)),
            );
          },
        },
      });
    } catch (exportError) {
      toast.error(errorMessage(exportError));
    } finally {
      setExporting(false);
    }
  }, [t]);

  const clearDisabled =
    loading || clearing || overallTotal === 0 || database?.available !== true;
  const exportDisabled =
    loading || exporting || overallTotal === 0 || database?.available !== true;
  const columns = useMemo<ColumnDef<CleanRecord>[]>(
    () => [
      {
        accessorKey: "cleaned_at",
        header: ({ column }) => (
          <SortableHeader column={column}>{t("cleanedAt")}</SortableHeader>
        ),
        cell: ({ row }) => formatCleanedAt(row.original.cleaned_at),
      },
      {
        accessorKey: "item_path",
        enableSorting: false,
        header: HistoryPathFilterHeader,
        cell: ({ row }) => (
          <span
            className="block max-w-xl truncate font-medium"
            title={row.original.item_path}
          >
            {row.original.item_path}
          </span>
        ),
      },
      {
        accessorKey: "item_type",
        enableSorting: false,
        header: ({ column }) => (
          <EnumFilterHeader
            column={column}
            emptyValue="all"
            label={t("type")}
            options={[
              { label: t("allItemTypes"), value: "all" },
              { label: t("recentFile"), value: "recent_file" },
              { label: t("frequentFolder"), value: "frequent_folder" },
            ]}
          />
        ),
        cell: ({ row }) =>
          t(
            row.original.item_type === "recent_file"
              ? "recentFile"
              : "frequentFolder",
          ),
      },
      {
        accessorKey: "source",
        enableSorting: false,
        header: ({ column }) => (
          <EnumFilterHeader
            column={column}
            emptyValue="all"
            label={t("cleanupSource")}
            options={[
              { label: t("allCleanupSources"), value: "all" },
              { label: t("manualCleanup"), value: "manual" },
              { label: t("automatic"), value: "auto" },
            ]}
          />
        ),
        cell: ({ row }) =>
          row.original.source === "manual"
            ? t("manualCleanup")
            : t("automatic"),
      },
      {
        accessorKey: "rule_keyword",
        enableSorting: false,
        header: ({ column }) => (
          <EnumFilterHeader
            column={column}
            emptyValue="all"
            label={t("sourceRule")}
            options={[
              { label: t("allRuleMatches"), value: "all" },
              { label: t("ruleMatched"), value: "matched" },
              { label: t("ruleUnmatched"), value: "unmatched" },
            ]}
          />
        ),
        cell: ({ row }) => row.original.rule_keyword ?? t("noRule"),
      },
      {
        id: "actions",
        enableSorting: false,
        header: () => (
          <div className="flex min-w-0 items-center justify-between gap-1">
            <span className="truncate" title={t("actions")}>
              {t("actions")}
            </span>
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    aria-label={t("historyDataActions")}
                    className="text-muted-foreground"
                    disabled={
                      restoringHidden || (clearDisabled && hiddenTotal === 0)
                    }
                    ref={actionsTriggerRef}
                    size="icon-xs"
                    title={t("historyDataActions")}
                    type="button"
                    variant="ghost"
                  >
                    {clearing || restoringHidden ? (
                      <LoaderCircle className="animate-spin" />
                    ) : (
                      <MoreHorizontal />
                    )}
                  </Button>
                }
              />
              <DropdownMenuContent align="end" className="w-48">
                {hiddenTotal > 0 ? (
                  <>
                    <DropdownMenuItem
                      disabled={restoringHidden}
                      onClick={() => void restoreHiddenRecords()}
                    >
                      <Eye />
                      {t("showHiddenHistoryRecords", {
                        count: hiddenTotal,
                      })}
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                  </>
                ) : null}
                <DropdownMenuItem
                  disabled={clearDisabled}
                  onClick={() => setClearOpen(true)}
                  variant="destructive"
                >
                  <Trash2 />
                  {t("clearHistory")}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        ),
        cell: ({ row }) => (
          <div className="flex gap-1">
            <Button
              aria-label={t("hideHistoryRecord")}
              disabled={hidingId !== null || deletingId !== null}
              onClick={() => void hideRecord(row.original)}
              size="icon-sm"
              title={t("hideHistoryRecord")}
              type="button"
              variant="ghost"
            >
              {hidingId === row.original.id ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <EyeOff />
              )}
            </Button>
            <Button
              aria-label={t("deleteHistoryRecord")}
              disabled={deletingId !== null}
              onClick={(event) => {
                deleteTriggerRef.current = event.currentTarget;
                setPendingDelete(row.original);
              }}
              size="icon-sm"
              title={t("deleteHistoryRecord")}
              type="button"
              variant="ghost"
            >
              <Trash2 />
            </Button>
          </div>
        ),
      },
    ],
    [
      clearDisabled,
      clearing,
      deletingId,
      hiddenTotal,
      hideRecord,
      hidingId,
      restoreHiddenRecords,
      restoringHidden,
      t,
    ],
  );

  const table = useReactTable({
    columns,
    data: records,
    autoResetPageIndex: false,
    enableMultiSort: false,
    getCoreRowModel: getCoreRowModel(),
    manualFiltering: true,
    manualPagination: true,
    manualSorting: true,
    onColumnFiltersChange: (updater) => {
      setColumnFilters(updater);
      setPagination((current) =>
        current.pageIndex === 0 ? current : { ...current, pageIndex: 0 },
      );
    },
    onPaginationChange: setPagination,
    onSortingChange: (next) => {
      setSorting(next);
      setPagination((current) => ({ ...current, pageIndex: 0 }));
    },
    pageCount: Math.ceil(total / HISTORY_PAGE_SIZE),
    state: { columnFilters, pagination, sorting },
  });
  const pageCount = table.getPageCount();

  return (
    <>
      <div className="mx-auto grid h-full max-w-6xl content-start gap-4 p-4">
        {database && !database.available ? (
          <DatabaseRecoveryPanel
            onRecovered={loadRecords}
            onStatusChange={setDatabase}
            status={database}
          />
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

        <section aria-label={t("historyItems")} className="min-w-0">
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                {table.getHeaderGroups().map((headerGroup) => (
                  <TableRow key={headerGroup.id}>
                    {headerGroup.headers.map((header) => (
                      <TableHead
                        className={historyColumnClassName(header.column.id)}
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
                {loading ? (
                  <HistoryTableMessage message={t("loadingHistory")} />
                ) : records.length === 0 ? (
                  <HistoryTableMessage
                    message={
                      columnFilters.length > 0 ? t("noMatches") : t("noHistory")
                    }
                  />
                ) : (
                  table.getRowModel().rows.map((row) => (
                    <TableRow key={row.id}>
                      {row.getVisibleCells().map((cell) => (
                        <TableCell
                          className={historyColumnClassName(cell.column.id)}
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
          {!loading && total > 0 ? (
            <div className="grid gap-y-3 py-4 text-sm text-muted-foreground sm:grid-cols-[minmax(0,1fr)_auto_7rem] sm:items-center">
              <span>
                {t("pageStatus", {
                  count: total,
                  page: pagination.pageIndex + 1,
                  pageCount,
                })}
              </span>
              <div className="flex items-center justify-end gap-2 pr-16 sm:justify-self-end sm:pr-0">
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
              <span aria-hidden="true" className="hidden sm:block" />
            </div>
          ) : null}
        </section>
      </div>

      <Button
        aria-label={t("exportAllHistoryCsv")}
        className="fixed bottom-6 right-6 z-20 size-12 rounded-full shadow-lg"
        disabled={exportDisabled}
        onClick={() => void exportHistory()}
        title={t("exportAllHistoryCsv")}
        type="button"
      >
        {exporting ? <LoaderCircle className="animate-spin" /> : <Download />}
      </Button>

      <AlertDialog open={clearOpen} onOpenChange={setClearOpen}>
        <AlertDialogContent finalFocus={() => actionsTriggerRef.current}>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("clearHistoryQuestion")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("clearHistoryDescription", { count: overallTotal })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={clearing}>{t("cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={clearing}
              onClick={() => void clearHistory()}
              type="button"
              variant="destructive"
            >
              {clearing ? <LoaderCircle className="animate-spin" /> : <Trash2 />}
              {t("clearHistory")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent finalFocus={() => deleteTriggerRef.current}>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("deleteHistoryRecord")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("deleteHistoryRecordDescription", {
                path: pendingDelete?.item_path ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deletingId !== null}>
              {t("cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={deletingId !== null}
              onClick={() => void deleteRecord()}
              type="button"
              variant="destructive"
            >
              {deletingId !== null ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <Trash2 />
              )}
              {t("deleteHistoryRecord")}
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
      className="flex h-8 w-full items-center justify-between gap-1.5 text-left text-sm font-medium"
      onClick={column.getToggleSortingHandler()}
      type="button"
    >
      {children}
      <SortIcon
        className={sorted ? "size-4 text-primary" : "size-4 text-muted-foreground"}
      />
    </button>
  );
}

function KeywordFilterHeader({
  clearLabel,
  column,
  filterLabel,
  label,
}: {
  clearLabel: string;
  column: Column<CleanRecord, unknown>;
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

function HistoryPathFilterHeader({
  column,
}: {
  column: Column<CleanRecord, unknown>;
}) {
  const { t } = useI18n();
  return (
    <KeywordFilterHeader
      clearLabel={t("clearHistoryPathFilter")}
      column={column}
      filterLabel={t("filterHistoryPath")}
      label={t("path")}
    />
  );
}

function EnumFilterHeader({
  column,
  emptyValue,
  label,
  options,
}: {
  column: Column<CleanRecord, unknown>;
  emptyValue: string;
  label: string;
  options: FilterOption[];
}) {
  const active = column.getIsFiltered();
  const value = (column.getFilterValue() as string | undefined) ?? emptyValue;
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
              column.setFilterValue(
                nextValue === emptyValue ? undefined : nextValue,
              )
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

function getFilterValue(filters: ColumnFiltersState, id: string) {
  const value = filters.find((filter) => filter.id === id)?.value;
  return typeof value === "string" ? value : undefined;
}

function historyColumnClassName(columnId: string): string {
  switch (columnId) {
    case "cleaned_at":
      return "min-w-44";
    case "item_path":
      return "min-w-80";
    case "item_type":
      return "min-w-40";
    case "source":
      return "min-w-40";
    case "rule_keyword":
      return "min-w-44";
    case "actions":
      return "w-28";
    default:
      return "";
  }
}

function HistoryTableMessage({ message }: { message: string }) {
  return (
    <TableRow>
      <TableCell className="h-40 text-center text-muted-foreground" colSpan={6}>
        {message}
      </TableCell>
    </TableRow>
  );
}

function formatCleanedAt(value: string) {
  const date = new Date(`${value.replace(" ", "T")}Z`);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function exportFileName() {
  const now = new Date();
  const date = [now.getFullYear(), now.getMonth() + 1, now.getDate()]
    .map((part) => String(part).padStart(2, "0"))
    .join("-");
  return `scourgify-history-${date}.csv`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
