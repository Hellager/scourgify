import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Link } from "react-router-dom";
import { RefreshCw, Search } from "lucide-react";
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
  const [error, setError] = useState<string | null>(null);

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

  const refresh = useCallback(async () => {
    setSelectedPaths(new Set());
    const [countsResult] = await Promise.allSettled([
      loadCounts(),
      loadItems(activeTab),
    ]);
    if (countsResult.status === "rejected") {
      setCounts(emptyCounts);
      setError(errorMessage(countsResult.reason));
    }
  }, [activeTab, loadCounts, loadItems]);

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
                </TableRow>
              </TableHeader>
              <TableBody>
                {tableState ? (
                  <TableRow>
                    <TableCell
                      className="h-40 text-center text-muted-foreground"
                      colSpan={3}
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
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </TabsContent>
        </Tabs>
      </section>
    </main>
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
