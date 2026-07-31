import { useCallback, useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  ChartArea,
  Dices,
  FileClock,
  FolderClock,
  History,
  RefreshCw,
  RotateCcw,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { REFRESH_DASHBOARD_EVENT, dispatchAppEvent } from "@/lib/app-events";
import { commandErrorMessage, invokeCommand } from "@/lib/commands";

type MockScenario =
  | "normal"
  | "empty"
  | "partial_failure"
  | "post_mutation_warning";
type DestListRange = "24h" | "7d" | "30d";

interface QaItem {
  path: string;
  name: string;
  last_interaction_at: number | null;
  pinned: boolean | null;
}

interface QaItemMetadata {
  path: string;
  access_count: number;
  score: number | null;
  recent_rank: number;
  mru_position: number;
  pinned: boolean;
  pin_order: number | null;
  warning_count: number;
}

interface MockSnapshot {
  scenario: MockScenario;
  revision: number;
  recent: QaItem[];
  frequent: QaItem[];
  recent_metadata: QaItemMetadata[];
  frequent_metadata: QaItemMetadata[];
  visibility: {
    recent: boolean;
    frequent: boolean;
    start_recommended: boolean;
  };
}

interface MockState {
  enabled: boolean;
  snapshot: MockSnapshot;
}

interface EventEntry {
  id: number;
  name: string;
  payload: unknown;
}

interface MockRulesResult {
  generated: number;
}

interface MockHistoryResult {
  generated: number;
}

interface DestListTrendPoint {
  date: string;
  recent_files: number;
  frequent_folders: number;
}

const scenarios: Array<{ label: string; value: MockScenario }> = [
  { label: "Normal", value: "normal" },
  { label: "Empty", value: "empty" },
  { label: "Partial failure", value: "partial_failure" },
  { label: "Post-mutation warning", value: "post_mutation_warning" },
];
const MAX_MOCK_RULES = 1_000;
const MAX_MOCK_HISTORY_RECORDS = 1_000;

export function MockTestPage() {
  const [state, setState] = useState<MockState | null>(null);
  const [events, setEvents] = useState<EventEntry[]>([]);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ruleCount, setRuleCount] = useState("20");
  const [generatedRuleCount, setGeneratedRuleCount] = useState<number | null>(
    null,
  );
  const [historyCount, setHistoryCount] = useState("50");
  const [generatedHistoryCount, setGeneratedHistoryCount] = useState<
    number | null
  >(null);
  const [destListRange, setDestListRange] = useState<DestListRange>("30d");
  const [destListTrend, setDestListTrend] = useState<DestListTrendPoint[] | null>(
    null,
  );

  const recordEvent = useCallback((name: string, payload: unknown) => {
    setEvents((current) => [
      { id: Date.now() + current.length, name, payload },
      ...current.slice(0, 19),
    ]);
  }, []);

  const load = useCallback(async () => {
    setState(await invokeCommand<MockState>("get_mock_state"));
  }, []);

  useEffect(() => {
    void load().catch((value) => setError(commandErrorMessage(value)));
  }, [load]);

  useEffect(() => {
    const listeners = [
      listen("quick-access-changed", ({ payload }) =>
        recordEvent("quick-access-changed", payload),
      ),
      listen("auto-clean-finished", ({ payload }) =>
        recordEvent("auto-clean-finished", payload),
      ),
    ];
    return () => {
      listeners.forEach((listener) => void listener.then((unlisten) => unlisten()));
    };
  }, [recordEvent]);

  const run = async (operation: () => Promise<MockState>) => {
    setPending(true);
    setError(null);
    setDestListTrend(null);
    try {
      setState(await operation());
    } catch (value) {
      setError(commandErrorMessage(value));
    } finally {
      setPending(false);
    }
  };

  const generateRules = async () => {
    const count = Number(ruleCount);
    if (!Number.isInteger(count) || count < 1 || count > MAX_MOCK_RULES) {
      setError("Rule count must be between 1 and 1000.");
      return;
    }

    setPending(true);
    setError(null);
    try {
      const result = await invokeCommand<MockRulesResult>("generate_mock_rules", {
        count,
      });
      setGeneratedRuleCount(result.generated);
    } catch (value) {
      setError(commandErrorMessage(value));
    } finally {
      setPending(false);
    }
  };

  const generateHistory = async () => {
    const count = Number(historyCount);
    if (
      !Number.isInteger(count) ||
      count < 1 ||
      count > MAX_MOCK_HISTORY_RECORDS
    ) {
      setError("History count must be between 1 and 1000.");
      return;
    }

    setPending(true);
    setError(null);
    try {
      const result = await invokeCommand<MockHistoryResult>(
        "generate_mock_history",
        { count },
      );
      setGeneratedHistoryCount(result.generated);
      dispatchAppEvent(REFRESH_DASHBOARD_EVENT);
    } catch (value) {
      setError(commandErrorMessage(value));
    } finally {
      setPending(false);
    }
  };

  const fetchDestListTrend = async () => {
    setPending(true);
    setError(null);
    try {
      setDestListTrend(
        await invokeCommand<DestListTrendPoint[]>("get_qa_activity_trend", {
          range: destListRange,
        }),
      );
    } catch (value) {
      setDestListTrend(null);
      setError(commandErrorMessage(value));
    } finally {
      setPending(false);
    }
  };

  const enabled = state?.enabled ?? false;
  const snapshot = state?.snapshot;
  const metadata = new Map(
    [
      ...(snapshot?.recent_metadata ?? []),
      ...(snapshot?.frequent_metadata ?? []),
    ].map((item) => [item.path, item]),
  );
  const items = [
    ...(snapshot?.recent.map((item) => ({
      ...item,
      metadata: metadata.get(item.path),
      type: "Recent",
    })) ?? []),
    ...(snapshot?.frequent.map((item) => ({
      ...item,
      metadata: metadata.get(item.path),
      type: "Frequent",
    })) ?? []),
  ];

  return (
    <main className="mx-auto w-full max-w-5xl px-5 py-6 sm:px-8">
      <header className="flex flex-wrap items-start justify-between gap-4 border-b pb-5">
        <div>
          <h1 className="text-lg font-semibold">Mock Lab</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Debug-only Quick Access data and event controls.
          </p>
        </div>
        <label className="flex items-center gap-3 text-sm font-medium">
          <span>{enabled ? "Mock backend" : "Real backend"}</span>
          <Switch
            checked={enabled}
            disabled={pending}
            onCheckedChange={(checked) =>
              void run(() => invokeCommand("set_mock_mode", { enabled: checked }))
            }
          />
        </label>
      </header>

      <section className="grid gap-4 border-b py-5 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-end">
        <label className="grid gap-2 text-sm font-medium">
          Scenario
          <Select
            disabled={!enabled || pending}
            value={snapshot?.scenario ?? "normal"}
            onValueChange={(scenario) =>
              void run(() =>
                invokeCommand("set_mock_scenario", {
                  scenario: scenario as MockScenario,
                }),
              )
            }
          >
            <SelectTrigger className="w-full max-w-sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {scenarios.map((scenario) => (
                <SelectItem key={scenario.value} value={scenario.value}>
                  {scenario.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
        <div className="flex flex-wrap gap-2">
          <Button
            disabled={!enabled || pending}
            onClick={() => void run(() => invokeCommand("refresh_mock_data"))}
            type="button"
            variant="outline"
          >
            <RefreshCw />
            Refresh
          </Button>
          <Button
            disabled={!enabled || pending}
            onClick={() => void run(() => invokeCommand("reset_mock_data"))}
            type="button"
            variant="outline"
          >
            <RotateCcw />
            Reset
          </Button>
        </div>
      </section>

      <section className="border-b py-5">
        <h2 className="mb-3 text-sm font-semibold">Mock rules</h2>
        <div className="flex flex-wrap items-end gap-3">
          <label className="grid w-40 gap-2 text-sm font-medium">
            Number of rules
            <Input
              disabled={!enabled || pending}
              inputMode="numeric"
              max={MAX_MOCK_RULES}
              min={1}
              onChange={(event) => setRuleCount(event.target.value)}
              type="number"
              value={ruleCount}
            />
          </label>
          <Button
            disabled={!enabled || pending}
            onClick={() => void generateRules()}
            type="button"
          >
            <Dices />
            Generate rules
          </Button>
          {generatedRuleCount !== null ? (
            <span className="pb-2 text-xs text-muted-foreground">
              {generatedRuleCount} mock rules generated.
            </span>
          ) : null}
        </div>
      </section>

      <section className="border-b py-5">
        <h2 className="mb-3 text-sm font-semibold">Mock cleanup history</h2>
        <div className="flex flex-wrap items-end gap-3">
          <label className="grid w-40 gap-2 text-sm font-medium">
            Number of records
            <Input
              disabled={!enabled || pending}
              inputMode="numeric"
              max={MAX_MOCK_HISTORY_RECORDS}
              min={1}
              onChange={(event) => {
                const value = event.target.value;
                const count = Number(value);
                setHistoryCount(
                  value !== "" && count > MAX_MOCK_HISTORY_RECORDS
                    ? String(MAX_MOCK_HISTORY_RECORDS)
                    : value,
                );
              }}
              type="number"
              value={historyCount}
            />
          </label>
          <Button
            disabled={!enabled || pending}
            onClick={() => void generateHistory()}
            type="button"
          >
            <History />
            Generate history
          </Button>
          {generatedHistoryCount !== null ? (
            <span className="pb-2 text-xs text-muted-foreground">
              {generatedHistoryCount} mock history records generated.
            </span>
          ) : null}
        </div>
      </section>

      <section className="border-b py-5">
        <div className="mb-3">
          <h2 className="text-sm font-semibold">Mock DestList acquisition</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            Fetches the Dashboard trend through the production command and the active mock backend.
          </p>
        </div>
        <div className="flex flex-wrap items-end gap-3">
          <label className="grid w-40 gap-2 text-sm font-medium">
            Date range
            <Select
              disabled={!enabled || pending}
              onValueChange={(value) => {
                setDestListRange(value as DestListRange);
                setDestListTrend(null);
              }}
              value={destListRange}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="24h">24h</SelectItem>
                <SelectItem value="7d">7D</SelectItem>
                <SelectItem value="30d">30D</SelectItem>
              </SelectContent>
            </Select>
          </label>
          <Button
            disabled={!enabled || pending}
            onClick={() => void fetchDestListTrend()}
            type="button"
          >
            <ChartArea />
            Fetch DestList
          </Button>
        </div>
        {destListTrend ? (
          <div className="mt-4 max-h-72 overflow-auto rounded-md border">
            <Table>
              <TableHeader className="sticky top-0 bg-background">
                <TableRow>
                  <TableHead>Date</TableHead>
                  <TableHead className="text-right">Recent files</TableHead>
                  <TableHead className="text-right">Frequent folders</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {destListTrend.map((point) => (
                  <TableRow key={point.date}>
                    <TableCell>{point.date}</TableCell>
                    <TableCell className="text-right tabular-nums">
                      {point.recent_files}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {point.frequent_folders}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        ) : null}
      </section>

      <section className="border-b py-5">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-sm font-semibold">Event controls</h2>
            <p className="text-xs text-muted-foreground">
              Events pass through the backend cache or the existing Tauri event channel.
            </p>
          </div>
          <span className="text-xs text-muted-foreground">
            Revision {snapshot?.revision ?? 0}
          </span>
        </div>
        <div className="flex flex-wrap gap-2">
          <EventButton
            disabled={!enabled || pending}
            icon={<FileClock />}
            label="Recent changed"
            onClick={() =>
              void run(() =>
                invokeCommand("trigger_mock_event", {
                  event: "quick_access_recent",
                }),
              )
            }
          />
          <EventButton
            disabled={!enabled || pending}
            icon={<FolderClock />}
            label="Frequent changed"
            onClick={() =>
              void run(() =>
                invokeCommand("trigger_mock_event", {
                  event: "quick_access_frequent",
                }),
              )
            }
          />
          <EventButton
            disabled={!enabled || pending}
            icon={<Activity />}
            label="Auto-clean finished"
            onClick={() =>
              void run(() =>
                invokeCommand("trigger_mock_event", {
                  event: "auto_clean_finished",
                }),
              )
            }
          />
        </div>
      </section>

      {error ? <p className="py-4 text-sm text-destructive">{error}</p> : null}

      <section className="grid gap-6 py-5 lg:grid-cols-[minmax(0,1fr)_20rem]">
        <div className="min-w-0">
          <div className="mb-3 flex items-center justify-between gap-3">
            <h2 className="text-sm font-semibold">Mock Quick Access</h2>
            <span className="text-xs text-muted-foreground">
              {snapshot?.recent.length ?? 0} recent / {snapshot?.frequent.length ?? 0} frequent
            </span>
          </div>
          <div className="overflow-hidden rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Type</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>Path</TableHead>
                  <TableHead>Last used</TableHead>
                  <TableHead>Access</TableHead>
                  <TableHead>Score</TableHead>
                  <TableHead>Pinned</TableHead>
                  <TableHead>Warnings</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.length ? (
                  items.map((item) => (
                    <TableRow key={`${item.type}:${item.path}`}>
                      <TableCell>{item.type}</TableCell>
                      <TableCell>{item.name}</TableCell>
                      <TableCell className="max-w-72 truncate" title={item.path}>
                        {item.path}
                      </TableCell>
                      <TableCell>{formatTimestamp(item.last_interaction_at)}</TableCell>
                      <TableCell>{item.metadata?.access_count ?? "-"}</TableCell>
                      <TableCell>
                        {item.metadata?.score == null
                          ? "-"
                          : item.metadata.score.toFixed(2)}
                      </TableCell>
                      <TableCell>{item.pinned == null ? "-" : item.pinned ? "Yes" : "No"}</TableCell>
                      <TableCell>{item.metadata?.warning_count ?? "-"}</TableCell>
                    </TableRow>
                  ))
                ) : (
                  <TableRow>
                    <TableCell className="h-24 text-center text-muted-foreground" colSpan={8}>
                      No mock items.
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </div>
        </div>

        <aside className="min-w-0 border-l pl-5">
          <div className="mb-3 flex items-center justify-between gap-2">
            <h2 className="text-sm font-semibold">Event log</h2>
            <Button
              disabled={!events.length}
              onClick={() => setEvents([])}
              size="sm"
              type="button"
              variant="ghost"
            >
              Clear
            </Button>
          </div>
          {events.length ? (
            <ScrollArea className="h-96">
              <div className="grid gap-2 pr-3">
                {events.map((event) => (
                  <div className="rounded-md border p-2 text-xs" key={event.id}>
                    <div className="font-medium">{event.name}</div>
                    <pre className="mt-1 whitespace-pre-wrap text-muted-foreground">
                      {JSON.stringify(event.payload, null, 2)}
                    </pre>
                  </div>
                ))}
              </div>
            </ScrollArea>
          ) : (
            <div className="flex min-h-24 items-center justify-center rounded-md border border-dashed text-xs text-muted-foreground">
              No events received.
            </div>
          )}
        </aside>
      </section>
    </main>
  );
}

function formatTimestamp(value: number | null) {
  return value == null ? "-" : new Date(value).toLocaleString();
}

function EventButton({
  disabled,
  icon,
  label,
  onClick,
}: {
  disabled: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <Button disabled={disabled} onClick={onClick} type="button" variant="outline">
      {icon}
      {label}
    </Button>
  );
}
