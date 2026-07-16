import { useCallback, useEffect, useState } from "react";
import { save as saveFile } from "@tauri-apps/plugin-dialog";
import { ChevronLeft, ChevronRight, Download, LoaderCircle, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
import { REFRESH_HISTORY_EVENT } from "@/lib/app-events";
import { invokeCommand } from "@/lib/commands";
import { type I18nKey, useI18n } from "@/lib/i18n";

type RunAction = "remove_selected" | "empty" | "smart_clean" | "auto_clean";
type RunTrigger = "manual" | "monitor" | "scheduled";
type RunStatus = "running" | "success" | "partial" | "failed" | "noop" | "interrupted";
type RunDateRange = "all" | "7d" | "30d";
type ExportFormat = "csv" | "json";

interface CleanupRun {
  id: number;
  action: RunAction;
  trigger: RunTrigger;
  qa_type: "recent" | "frequent" | "all";
  status: RunStatus;
  requested_count: number;
  succeeded_count: number;
  failed_count: number;
  protected_count: number;
  warning_count: number;
  history_error_count: number;
  section_error_count: number;
  incident_id: string | null;
  started_at: string;
  completed_at: string | null;
}

interface CleanupRunPage {
  runs: CleanupRun[];
  total: number;
  page: number;
  page_size: number;
}

interface RunFilter {
  action: RunAction | null;
  trigger: RunTrigger | null;
  status: RunStatus | null;
  date_range: Exclude<RunDateRange, "all"> | null;
}

interface ExportResult {
  count: number;
  path: string;
}

const PAGE_SIZE = 20;

export function CleanupRunsPanel() {
  const { t } = useI18n();
  const [runs, setRuns] = useState<CleanupRun[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [action, setAction] = useState<"all" | RunAction>("all");
  const [trigger, setTrigger] = useState<"all" | RunTrigger>("all");
  const [status, setStatus] = useState<"all" | RunStatus>("all");
  const [dateRange, setDateRange] = useState<RunDateRange>("all");
  const [loading, setLoading] = useState(true);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const filter: RunFilter = {
    action: action === "all" ? null : action,
    trigger: trigger === "all" ? null : trigger,
    status: status === "all" ? null : status,
    date_range: dateRange === "all" ? null : dateRange,
  };

  const loadRuns = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invokeCommand<CleanupRunPage>("get_cleanup_runs", {
        query: { page, page_size: PAGE_SIZE, ...filter },
      });
      setRuns(result.runs);
      setTotal(result.total);
    } catch (loadError) {
      setRuns([]);
      setTotal(0);
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, [action, dateRange, page, status, trigger]);

  useEffect(() => {
    void loadRuns();
  }, [loadRuns]);

  useEffect(() => {
    const refresh = () => void loadRuns();
    window.addEventListener(REFRESH_HISTORY_EVENT, refresh);
    return () => window.removeEventListener(REFRESH_HISTORY_EVENT, refresh);
  }, [loadRuns]);

  const updateFilter = (update: () => void) => {
    update();
    setPage(1);
  };

  const exportRuns = async (format: ExportFormat) => {
    setExporting(true);
    try {
      const path = await saveFile({
        defaultPath: exportFileName(format),
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
        title: t("exportCleanupRuns"),
      });
      if (!path) {
        return;
      }
      const result = await invokeCommand<ExportResult>("export_cleanup_runs", {
        path,
        format,
        filter,
      });
      toast.success(t("historyExported", { count: result.count }), {
        action: {
          label: t("openFolder"),
          onClick: () => void invokeCommand("open_in_explorer", { path: result.path }),
        },
      });
    } catch (exportError) {
      toast.error(errorMessage(exportError));
    } finally {
      setExporting(false);
    }
  };

  const pageCount = Math.ceil(total / PAGE_SIZE);

  return (
    <section className="grid gap-4" aria-labelledby="cleanup-runs-title">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-sm font-semibold" id="cleanup-runs-title">
          {t("cleanupRuns")}
        </h2>
        <div className="flex items-center gap-2">
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <Button disabled={exporting} size="sm" type="button" variant="outline">
                  {exporting ? <LoaderCircle className="animate-spin" /> : <Download />}
                  {t("exportHistory")}
                </Button>
              }
            />
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => void exportRuns("csv")}>CSV</DropdownMenuItem>
              <DropdownMenuItem onSelect={() => void exportRuns("json")}>JSON</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <Button
            aria-label={t("refreshHistory")}
            disabled={loading}
            onClick={() => void loadRuns()}
            size="icon-sm"
            title={t("refreshHistory")}
            type="button"
            variant="outline"
          >
            <RefreshCw className={loading ? "animate-spin" : ""} />
          </Button>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <RunSelect
          label={t("cleanupAction")}
          onChange={(value) => updateFilter(() => setAction(value as "all" | RunAction))}
          options={[
            ["all", t("all")],
            ["remove_selected", t("actionRemoveSelected")],
            ["empty", t("actionClear")],
            ["smart_clean", t("smartClean")],
            ["auto_clean", t("autoClean")],
          ]}
          value={action}
        />
        <RunSelect
          label={t("cleanupTrigger")}
          onChange={(value) => updateFilter(() => setTrigger(value as "all" | RunTrigger))}
          options={[
            ["all", t("all")],
            ["manual", t("manualTrigger")],
            ["monitor", t("monitorTrigger")],
            ["scheduled", t("scheduledTrigger")],
          ]}
          value={trigger}
        />
        <RunSelect
          label={t("status")}
          onChange={(value) => updateFilter(() => setStatus(value as "all" | RunStatus))}
          options={[
            ["all", t("all")],
            ["success", t("runStatusSuccess")],
            ["partial", t("runStatusPartial")],
            ["failed", t("runStatusFailed")],
            ["noop", t("runStatusNoop")],
            ["interrupted", t("runStatusInterrupted")],
          ]}
          value={status}
        />
        <RunSelect
          label={t("dateRange")}
          onChange={(value) => updateFilter(() => setDateRange(value as RunDateRange))}
          options={[
            ["all", t("all")],
            ["7d", t("last7Days")],
            ["30d", t("last30Days")],
          ]}
          value={dateRange}
        />
      </div>

      {error ? <p className="text-sm text-destructive">{error}</p> : null}

      <div className="overflow-x-auto rounded-md border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("startedAt")}</TableHead>
              <TableHead>{t("cleanupAction")}</TableHead>
              <TableHead>{t("cleanupTrigger")}</TableHead>
              <TableHead>{t("scope")}</TableHead>
              <TableHead>{t("result")}</TableHead>
              <TableHead>{t("status")}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <RunMessage message={t("loadingHistory")} />
            ) : runs.length === 0 ? (
              <RunMessage message={t("noOperations")} />
            ) : (
              runs.map((run) => (
                <TableRow key={run.id}>
                  <TableCell>{formatTimestamp(run.started_at)}</TableCell>
                  <TableCell>{t(actionKey(run.action))}</TableCell>
                  <TableCell>{t(triggerKey(run.trigger))}</TableCell>
                  <TableCell>{t(scopeKey(run.qa_type))}</TableCell>
                  <TableCell className="tabular-nums">
                    {t("runResultSummary", {
                      failed: run.failed_count + run.section_error_count,
                      requested: run.requested_count,
                      succeeded: run.succeeded_count,
                    })}
                  </TableCell>
                  <TableCell>{t(statusKey(run.status))}</TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 text-sm text-muted-foreground">
        <span>{t("historyCount", { count: total })}</span>
        <div className="flex items-center gap-2">
          <Button
            disabled={loading || page <= 1}
            onClick={() => setPage((current) => current - 1)}
            size="sm"
            type="button"
            variant="outline"
          >
            <ChevronLeft />
            {t("previous")}
          </Button>
          <span className="tabular-nums">{pageCount === 0 ? 0 : page} / {pageCount}</span>
          <Button
            disabled={loading || page >= pageCount}
            onClick={() => setPage((current) => current + 1)}
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
  );
}

function RunSelect({
  label,
  onChange,
  options,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  options: Array<[string, string]>;
  value: string;
}) {
  return (
    <Select onValueChange={onChange} value={value}>
      <SelectTrigger aria-label={label} className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map(([option, text]) => (
          <SelectItem key={option} value={option}>
            {text}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function RunMessage({ message }: { message: string }) {
  return (
    <TableRow>
      <TableCell className="h-40 text-center text-muted-foreground" colSpan={6}>
        {message}
      </TableCell>
    </TableRow>
  );
}

function actionKey(action: RunAction): I18nKey {
  return {
    remove_selected: "actionRemoveSelected",
    empty: "actionClear",
    smart_clean: "smartClean",
    auto_clean: "autoClean",
  }[action] as I18nKey;
}

function triggerKey(trigger: RunTrigger): I18nKey {
  return {
    manual: "manualTrigger",
    monitor: "monitorTrigger",
    scheduled: "scheduledTrigger",
  }[trigger] as I18nKey;
}

function statusKey(status: RunStatus): I18nKey {
  return `runStatus${status[0].toUpperCase()}${status.slice(1)}` as I18nKey;
}

function scopeKey(scope: CleanupRun["qa_type"]): I18nKey {
  return scope === "recent" ? "recentFiles" : scope === "frequent" ? "frequentFolders" : "all";
}

function formatTimestamp(value: string) {
  const date = new Date(`${value.replace(" ", "T")}Z`);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function exportFileName(format: ExportFormat) {
  const date = new Date().toISOString().slice(0, 10);
  return `scourgify-cleanup-runs-${date}.${format}`;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
