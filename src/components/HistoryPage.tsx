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
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { PageHeader } from "@/components/AppShell";
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
  page: number;
  page_size: number;
}

interface DatabaseStatus {
  available: boolean;
  error: string | null;
}

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

export function HistoryPage() {
  const { t } = useI18n();
  const [records, setRecords] = useState<CleanRecord[]>([]);
  const [total, setTotal] = useState(0);
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
  const requestId = useRef(0);

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
        return;
      }
      const result = await invoke<CleanRecordPage>("get_clean_records", {
        query: {
          page: pagination.pageIndex + 1,
          page_size: pagination.pageSize,
          sort_by: sortingRule.id,
          sort_order: sortingRule.desc ? "desc" : "asc",
        },
      });
      if (request !== requestId.current) {
        return;
      }
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
      setError(errorMessage(loadError));
    } finally {
      if (request === requestId.current) {
        setLoading(false);
      }
    }
  }, [pagination.pageIndex, pagination.pageSize, sortingRule.desc, sortingRule.id]);

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
    total === 0 ||
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
          <section className="border-l-2 border-destructive px-4 py-2">
            <h2 className="text-sm font-semibold">{t("databaseUnavailable")}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("databaseUnavailableDescription")}
            </p>
            {database.error ? (
              <p className="mt-2 break-all font-mono text-xs text-muted-foreground">
                {database.error}
              </p>
            ) : null}
          </section>
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
                  <HistoryTableMessage message={t("noHistory")} />
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
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("clearHistoryQuestion")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("clearHistoryDescription", { count: total })}
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
