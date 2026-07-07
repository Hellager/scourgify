import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Link } from "react-router-dom";
import { FolderOpen, RefreshCw, Search, Trash2 } from "lucide-react";
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
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

type QaType = "recent" | "frequent";

interface QaItem {
  path: string;
  name: string;
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

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

type PendingAction = "remove" | "empty" | null;

const tabs: Array<{ value: QaType; label: string }> = [
  { value: "recent", label: "Recent Files" },
  { value: "frequent", label: "Frequent Folders" },
];

const emptyCounts: QaCounts = {
  recent: 0,
  frequent: 0,
  all: 0,
};

export function Dashboard() {
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

  const selectedVisibleCount = filteredItems.filter((item) =>
    selectedPaths.has(item.path),
  ).length;
  const allVisibleSelected =
    filteredItems.length > 0 && selectedVisibleCount === filteredItems.length;
  const someVisibleSelected =
    selectedVisibleCount > 0 && selectedVisibleCount < filteredItems.length;

  const loadCounts = useCallback(async () => {
    setCounts(await invoke<QaCounts>("get_qa_counts"));
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

  const switchTab = (value: unknown) => {
    const nextTab = toQaType(value);
    setActiveTab(nextTab);
    setSelectedPaths(new Set());
    setQuery("");
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

  const toggleVisible = (checked: boolean) => {
    setSelectedPaths((current) => {
      const next = new Set(current);
      for (const item of filteredItems) {
        if (checked) {
          next.add(item.path);
        } else {
          next.delete(item.path);
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
      } else {
        await invoke("empty_qa_items", { qaType: activeTab });
        toast.success(`Cleared ${currentLabel}.`);
      }

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
    <main className="min-h-screen bg-background text-foreground">
      <header className="flex h-14 items-center justify-between border-b px-6">
        <div>
          <h1 className="text-base font-semibold">Scourgify</h1>
          <p className="text-xs text-muted-foreground">Dashboard</p>
        </div>
        <div className="flex items-center gap-2">
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
            </div>
          </div>

          <TabsContent className="pt-4" value={activeTab}>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-10">
                    <Checkbox
                      aria-label="Select visible items"
                      checked={allVisibleSelected}
                      disabled={filteredItems.length === 0}
                      indeterminate={someVisibleSelected}
                      onCheckedChange={toggleVisible}
                      parent
                    />
                  </TableHead>
                  <TableHead className="w-64">Name</TableHead>
                  <TableHead>Path</TableHead>
                  <TableHead className="w-24 text-right">Location</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {tableState ? (
                  <TableRow>
                    <TableCell
                      className="h-40 text-center text-muted-foreground"
                      colSpan={4}
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
                  filteredItems.map((item) => (
                    <TableRow
                      data-state={
                        selectedPaths.has(item.path) ? "selected" : undefined
                      }
                      key={item.path}
                    >
                      <TableCell>
                        <Checkbox
                          aria-label={`Select ${item.name}`}
                          checked={selectedPaths.has(item.path)}
                          onCheckedChange={(checked) =>
                            togglePath(item.path, checked)
                          }
                        />
                      </TableCell>
                      <TableCell className="max-w-64 truncate font-medium">
                        {item.name}
                      </TableCell>
                      <TableCell className="max-w-[560px] truncate text-muted-foreground">
                        {item.path}
                      </TableCell>
                      <TableCell className="text-right">
                        <Button
                          aria-label={`Open location for ${item.name}`}
                          onClick={() => void openLocation(item.path)}
                          size="icon-sm"
                          title="Open location"
                          type="button"
                          variant="ghost"
                        >
                          <FolderOpen />
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
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
    </main>
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

  return (
    <AlertDialog open={action !== null} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {isEmpty ? `Clear ${currentLabel}?` : "Remove selected items?"}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {isEmpty
              ? `This will clear ${currentLabel} from Quick Access.`
              : `This will remove ${selectedCount} selected item(s) from Quick Access.`}
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
            {isEmpty ? "Clear" : "Remove"}
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
