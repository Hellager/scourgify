import { useCallback, useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  FileClock,
  FolderClock,
  RefreshCw,
  RotateCcw,
} from "lucide-react";

import { Button } from "@/components/ui/button";
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
import { commandErrorMessage, invokeCommand } from "@/lib/commands";

type MockScenario =
  | "normal"
  | "empty"
  | "partial_failure"
  | "post_mutation_warning";

interface QaItem {
  path: string;
  name: string;
  last_interaction_at: number | null;
  pinned: boolean | null;
}

interface MockSnapshot {
  scenario: MockScenario;
  revision: number;
  recent: QaItem[];
  frequent: QaItem[];
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

const scenarios: Array<{ label: string; value: MockScenario }> = [
  { label: "Normal", value: "normal" },
  { label: "Empty", value: "empty" },
  { label: "Partial failure", value: "partial_failure" },
  { label: "Post-mutation warning", value: "post_mutation_warning" },
];

export function MockTestPage() {
  const [state, setState] = useState<MockState | null>(null);
  const [events, setEvents] = useState<EventEntry[]>([]);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
    try {
      setState(await operation());
    } catch (value) {
      setError(commandErrorMessage(value));
    } finally {
      setPending(false);
    }
  };

  const enabled = state?.enabled ?? false;
  const snapshot = state?.snapshot;
  const items = [
    ...(snapshot?.recent.map((item) => ({ ...item, type: "Recent" })) ?? []),
    ...(snapshot?.frequent.map((item) => ({ ...item, type: "Frequent" })) ?? []),
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
                  <TableHead>Pinned</TableHead>
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
                      <TableCell>{item.pinned == null ? "-" : item.pinned ? "Yes" : "No"}</TableCell>
                    </TableRow>
                  ))
                ) : (
                  <TableRow>
                    <TableCell className="h-24 text-center text-muted-foreground" colSpan={4}>
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
          <div className="grid max-h-96 gap-2 overflow-y-auto">
            {events.length ? (
              events.map((event) => (
                <div className="rounded-md border p-2 text-xs" key={event.id}>
                  <div className="font-medium">{event.name}</div>
                  <pre className="mt-1 overflow-x-auto whitespace-pre-wrap text-muted-foreground">
                    {JSON.stringify(event.payload, null, 2)}
                  </pre>
                </div>
              ))
            ) : (
              <div className="flex min-h-24 items-center justify-center rounded-md border border-dashed text-xs text-muted-foreground">
                No events received.
              </div>
            )}
          </div>
        </aside>
      </section>
    </main>
  );
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
