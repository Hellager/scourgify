import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { zodResolver } from "@hookform/resolvers/zod";
import { Controller, useForm } from "react-hook-form";
import {
  type Column,
  type ColumnDef,
  type ColumnFiltersState,
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  useReactTable,
} from "@tanstack/react-table";
import {
  FilterX,
  ListFilter,
  Pencil,
  Plus,
  Search,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";
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
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { useI18n } from "@/lib/i18n";
import { invokeCommand } from "@/lib/commands";

const ruleFormSchema = z.object({
  keyword: z.string().trim().min(1),
  rule_type: z.enum(["whitelist", "blacklist"]),
  scope: z.enum(["all", "files", "folders"]),
  enabled: z.boolean(),
});

type RuleForm = z.infer<typeof ruleFormSchema>;

interface Rule extends RuleForm {
  id: number;
  created_at: string;
}

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

const emptyRule: RuleForm = {
  keyword: "",
  rule_type: "whitelist",
  scope: "all",
  enabled: true,
};

export function RulesPage() {
  const { t } = useI18n();
  const [rules, setRules] = useState<Rule[]>([]);
  const [database, setDatabase] = useState<DatabaseStatus | null>(null);
  const [privacyActive, setPrivacyActive] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [editingRule, setEditingRule] = useState<Rule | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Rule | null>(null);
  const [mutatingId, setMutatingId] = useState<number | null>(null);
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const addTriggerRef = useRef<HTMLButtonElement | null>(null);
  const formTriggerRef = useRef<HTMLButtonElement | null>(null);
  const deleteTriggerRef = useRef<HTMLButtonElement | null>(null);
  const {
    control,
    formState,
    handleSubmit,
    register,
    reset,
  } = useForm<RuleForm>({
    resolver: zodResolver(ruleFormSchema),
    defaultValues: emptyRule,
  });

  const loadRules = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [databaseStatus, privacyState] = await Promise.all([
        invokeCommand<DatabaseStatus>("get_database_status"),
        invokeCommand<PrivacyState>("privacy_state"),
      ]);
      setDatabase(databaseStatus);
      setPrivacyActive(privacyState !== "Inactive");
      setRules(
        databaseStatus.available ? await invokeCommand<Rule[]>("get_rules") : [],
      );
    } catch (loadError) {
      setRules([]);
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadRules();
  }, [loadRules]);

  const writesDisabled =
    loading || database?.available !== true || privacyActive;
  const ruleTypeLabels = useMemo<Record<RuleForm["rule_type"], string>>(
    () => ({
      whitelist: t("whitelist"),
      blacklist: t("blacklist"),
    }),
    [t],
  );
  const ruleScopeLabels = useMemo<Record<RuleForm["scope"], string>>(
    () => ({
      all: t("ruleScopeAll"),
      files: t("ruleScopeFiles"),
      folders: t("ruleScopeFolders"),
    }),
    [t],
  );
  const conflictingKeywords = useMemo(() => {
    const rulesByKeyword = new Map<
      string,
      { keyword: string; types: Set<Rule["rule_type"]> }
    >();
    for (const rule of rules) {
      const normalized = rule.keyword.trim().toLowerCase();
      const entry = rulesByKeyword.get(normalized) ?? {
        keyword: rule.keyword,
        types: new Set<Rule["rule_type"]>(),
      };
      entry.types.add(rule.rule_type);
      rulesByKeyword.set(normalized, entry);
    }
    return Array.from(rulesByKeyword.values())
      .filter(({ types }) => types.size === 2)
      .map(({ keyword }) => keyword);
  }, [rules]);

  const openCreate = () => {
    setEditingRule(null);
    reset(emptyRule);
    setFormOpen(true);
  };

  const openEdit = useCallback(
    (rule: Rule, trigger: HTMLButtonElement) => {
      formTriggerRef.current = trigger;
      setEditingRule(rule);
      reset({
        keyword: rule.keyword,
        rule_type: rule.rule_type,
        scope: rule.scope,
        enabled: rule.enabled,
      });
      setFormOpen(true);
    },
    [reset],
  );

  const saveRule = handleSubmit(async (values) => {
    try {
      const saved = editingRule
        ? await invokeCommand<Rule>("update_rule", {
            id: editingRule.id,
            rule: values,
          })
        : await invokeCommand<Rule>("add_rule", { rule: values });
      setRules((current) =>
        editingRule
          ? current.map((rule) => (rule.id === saved.id ? saved : rule))
          : [...current, saved].sort((left, right) => left.id - right.id),
      );
      setFormOpen(false);
      setEditingRule(null);
      toast.success(editingRule ? t("ruleSaved") : t("ruleAdded"));
    } catch (saveError) {
      toast.error(errorMessage(saveError));
    }
  });

  const toggleRule = useCallback(
    async (rule: Rule, enabled: boolean) => {
      setMutatingId(rule.id);
      try {
        const saved = await invokeCommand<Rule>("toggle_rule", {
          id: rule.id,
          enabled,
        });
        setRules((current) =>
          current.map((item) => (item.id === saved.id ? saved : item)),
        );
        toast.success(t("ruleSaved"));
      } catch (toggleError) {
        toast.error(errorMessage(toggleError));
      } finally {
        setMutatingId(null);
      }
    },
    [t],
  );

  const deleteRule = async () => {
    if (!pendingDelete) {
      return;
    }

    const rule = pendingDelete;
    setMutatingId(rule.id);
    try {
      await invokeCommand("remove_rule", { id: rule.id });
      setRules((current) => current.filter((item) => item.id !== rule.id));
      setPendingDelete(null);
      toast.success(t("ruleDeleted"));
    } catch (deleteError) {
      toast.error(errorMessage(deleteError));
    } finally {
      setMutatingId(null);
    }
  };

  const columns = useMemo<ColumnDef<Rule>[]>(
    () => [
      {
        accessorKey: "keyword",
        header: ({ column }) => (
          <KeywordFilterHeader
            clearLabel={t("clearRuleKeywordFilter")}
            column={column}
            filterLabel={t("filterRuleKeyword")}
            label={t("ruleKeyword")}
          />
        ),
        filterFn: "includesString",
        cell: ({ row }) => (
          <span
            className="block max-w-md truncate font-medium"
            title={row.original.keyword}
          >
            {row.original.keyword}
          </span>
        ),
      },
      {
        accessorKey: "rule_type",
        header: ({ column }) => (
          <EnumFilterHeader
            column={column}
            emptyValue="all"
            label={t("ruleType")}
            options={[
              { label: t("allRuleTypes"), value: "all" },
              { label: t("whitelist"), value: "whitelist" },
              { label: t("blacklist"), value: "blacklist" },
            ]}
          />
        ),
        filterFn: "equalsString",
        cell: ({ row }) => (
          <RuleTypeLabel ruleType={row.original.rule_type} />
        ),
      },
      {
        accessorKey: "scope",
        header: ({ column }) => (
          <EnumFilterHeader
            column={column}
            emptyValue="any"
            label={t("ruleScope")}
            options={[
              { label: t("allRuleScopes"), value: "any" },
              { label: t("ruleScopeAll"), value: "all" },
              { label: t("ruleScopeFiles"), value: "files" },
              { label: t("ruleScopeFolders"), value: "folders" },
            ]}
          />
        ),
        filterFn: "equalsString",
        cell: ({ row }) => <RuleScopeLabel scope={row.original.scope} />,
      },
      {
        id: "status",
        accessorFn: (rule) => (rule.enabled ? "enabled" : "disabled"),
        header: ({ column }) => (
          <EnumFilterHeader
            column={column}
            emptyValue="all"
            label={t("ruleStatus")}
            options={[
              { label: t("allRuleStatuses"), value: "all" },
              { label: t("enabled"), value: "enabled" },
              { label: t("disabled"), value: "disabled" },
            ]}
          />
        ),
        filterFn: "equalsString",
        cell: ({ row }) => (
          <div className="flex items-center gap-2">
            <Switch
              aria-label={t("ruleToggleLabel", {
                keyword: row.original.keyword,
              })}
              checked={row.original.enabled}
              disabled={
                writesDisabled || mutatingId === row.original.id
              }
              onCheckedChange={(enabled) =>
                void toggleRule(row.original, enabled)
              }
              size="sm"
            />
            <span className="text-sm text-muted-foreground">
              {row.original.enabled ? t("enabled") : t("disabled")}
            </span>
          </div>
        ),
      },
      {
        id: "actions",
        enableColumnFilter: false,
        header: t("actions"),
        cell: ({ row }) => (
          <div className="flex justify-end gap-1">
            <Button
              aria-label={t("editRule")}
              disabled={writesDisabled}
              onClick={(event) => openEdit(row.original, event.currentTarget)}
              size="icon-sm"
              title={t("editRule")}
              type="button"
              variant="ghost"
            >
              <Pencil />
            </Button>
            <Button
              aria-label={t("deleteRule")}
              disabled={writesDisabled}
              onClick={(event) => {
                deleteTriggerRef.current = event.currentTarget;
                setPendingDelete(row.original);
              }}
              size="icon-sm"
              title={t("deleteRule")}
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
      mutatingId,
      openEdit,
      ruleScopeLabels,
      ruleTypeLabels,
      t,
      toggleRule,
      writesDisabled,
    ],
  );

  const table = useReactTable({
    data: rules,
    columns,
    state: { columnFilters },
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getRowId: (rule) => String(rule.id),
    onColumnFiltersChange: setColumnFilters,
  });

  return (
    <>
      <div className="mx-auto grid max-w-6xl gap-4 p-6 pb-24">
        {database && !database.available ? (
          <DatabaseRecoveryPanel
            onRecovered={loadRules}
            onStatusChange={setDatabase}
            status={database}
          />
        ) : null}

        {privacyActive ? (
          <section className="border-l-2 border-border px-4 py-2">
            <h2 className="text-sm font-semibold">{t("privacyActive")}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("privacyWriteDisabled")}
            </p>
          </section>
        ) : null}

        {error ? (
          <section className="flex items-center justify-between gap-4 border-l-2 border-destructive px-4 py-2">
            <p className="min-w-0 break-words text-sm text-destructive">
              {error}
            </p>
            <Button
              onClick={() => void loadRules()}
              size="sm"
              type="button"
              variant="outline"
            >
              {t("refreshRules")}
            </Button>
          </section>
        ) : null}

        {conflictingKeywords.length > 0 ? (
          <section className="border-l-2 border-amber-500 px-4 py-2">
            <p className="break-words text-sm text-muted-foreground">
              {t("ruleConflictWarning", {
                keywords: conflictingKeywords.join(", "),
              })}
            </p>
          </section>
        ) : null}

        <section aria-label={t("rules")}>
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                {table.getHeaderGroups().map((headerGroup) => (
                  <TableRow key={headerGroup.id}>
                    {headerGroup.headers.map((header) => (
                      <TableHead
                        className={ruleColumnClassName(header.column.id)}
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
                  <RuleTableMessage message={t("loadingRules")} />
                ) : rules.length === 0 ? (
                  <RuleTableMessage message={t("noRules")} />
                ) : table.getRowModel().rows.length === 0 ? (
                  <RuleTableMessage message={t("noMatches")} />
                ) : (
                  table.getRowModel().rows.map((row) => (
                    <TableRow key={row.id}>
                      {row.getVisibleCells().map((cell) => (
                        <TableCell
                          className={ruleColumnClassName(cell.column.id)}
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
        </section>
      </div>

      <Button
        aria-label={t("addRule")}
        className="fixed bottom-6 right-6 z-20 size-12 rounded-full shadow-lg"
        disabled={writesDisabled}
        onClick={(event) => {
          formTriggerRef.current = event.currentTarget;
          openCreate();
        }}
        ref={addTriggerRef}
        size="icon-lg"
        title={t("addRule")}
        type="button"
      >
        <Plus className="size-5" />
      </Button>

      <Dialog
        open={formOpen}
        onOpenChange={(open) => {
          setFormOpen(open);
          if (!open) {
            setEditingRule(null);
          }
        }}
      >
        <DialogContent
          closeLabel={t("close")}
          finalFocus={() =>
            formTriggerRef.current?.isConnected &&
            !formTriggerRef.current.disabled
              ? formTriggerRef.current
              : addTriggerRef.current
          }
        >
          <form onSubmit={(event) => void saveRule(event)}>
            <DialogHeader>
              <DialogTitle>
                {editingRule ? t("editRule") : t("addRule")}
              </DialogTitle>
              <DialogDescription>{t("rulesSubtitle")}</DialogDescription>
            </DialogHeader>
            <div className="grid gap-4 py-5">
              <label className="grid gap-2">
                <span className="text-sm font-medium">{t("ruleKeyword")}</span>
                <Input
                  aria-invalid={Boolean(formState.errors.keyword)}
                  autoFocus
                  placeholder={t("ruleKeywordPlaceholder")}
                  {...register("keyword")}
                />
                {formState.errors.keyword ? (
                  <span className="text-xs text-destructive">
                    {t("ruleKeywordRequired")}
                  </span>
                ) : null}
              </label>
              <Controller
                control={control}
                name="rule_type"
                render={({ field }) => (
                  <label className="grid gap-2">
                    <span className="text-sm font-medium">{t("ruleType")}</span>
                    <Select onValueChange={field.onChange} value={field.value}>
                      <SelectTrigger className="w-full">
                        <SelectValue>{ruleTypeLabels[field.value]}</SelectValue>
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="whitelist">{t("whitelist")}</SelectItem>
                        <SelectItem value="blacklist">{t("blacklist")}</SelectItem>
                      </SelectContent>
                    </Select>
                  </label>
                )}
              />
              <Controller
                control={control}
                name="scope"
                render={({ field }) => (
                  <label className="grid gap-2">
                    <span className="text-sm font-medium">{t("ruleScope")}</span>
                    <Select onValueChange={field.onChange} value={field.value}>
                      <SelectTrigger className="w-full">
                        <SelectValue>{ruleScopeLabels[field.value]}</SelectValue>
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="all">{t("ruleScopeAll")}</SelectItem>
                        <SelectItem value="files">{t("ruleScopeFiles")}</SelectItem>
                        <SelectItem value="folders">{t("ruleScopeFolders")}</SelectItem>
                      </SelectContent>
                    </Select>
                  </label>
                )}
              />
              <Controller
                control={control}
                name="enabled"
                render={({ field }) => (
                  <label className="flex items-center justify-between gap-4 py-1">
                    <span className="text-sm font-medium">{t("enabled")}</span>
                    <Switch
                      checked={field.value}
                      inputRef={field.ref}
                      name={field.name}
                      onBlur={field.onBlur}
                      onCheckedChange={field.onChange}
                    />
                  </label>
                )}
              />
            </div>
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                {t("cancel")}
              </DialogClose>
              <Button disabled={formState.isSubmitting} type="submit">
                {t("save")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent
          finalFocus={() =>
            deleteTriggerRef.current?.isConnected &&
            !deleteTriggerRef.current.disabled
              ? deleteTriggerRef.current
              : addTriggerRef.current
          }
        >
          <AlertDialogHeader>
            <AlertDialogTitle>{t("deleteRule")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("deleteRuleDescription", {
                keyword: pendingDelete?.keyword ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={mutatingId !== null}
              onClick={() => void deleteRule()}
              type="button"
              variant="destructive"
            >
              {t("deleteRule")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );

  function RuleTypeLabel({ ruleType }: { ruleType: Rule["rule_type"] }) {
    const whitelist = ruleType === "whitelist";
    return (
      <span
        className={
          whitelist
            ? "inline-flex items-center gap-1 rounded-sm bg-emerald-100 px-2 py-0.5 text-xs font-medium text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200"
            : "inline-flex items-center gap-1 rounded-sm bg-amber-100 px-2 py-0.5 text-xs font-medium text-amber-900 dark:bg-amber-950 dark:text-amber-200"
        }
      >
        <ShieldCheck className="size-3" />
        {ruleTypeLabels[ruleType]}
      </span>
    );
  }

  function RuleScopeLabel({ scope }: { scope: Rule["scope"] }) {
    return (
      <span className="text-sm text-muted-foreground">
        {ruleScopeLabels[scope]}
      </span>
    );
  }
}

interface FilterOption {
  label: string;
  value: string;
}

function KeywordFilterHeader({
  clearLabel,
  column,
  filterLabel,
  label,
}: {
  clearLabel: string;
  column: Column<Rule, unknown>;
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
            onSelect={() => column.setFilterValue(undefined)}
          >
            <FilterX />
            {clearLabel}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function EnumFilterHeader({
  column,
  emptyValue,
  label,
  options,
}: {
  column: Column<Rule, unknown>;
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
        <DropdownMenuContent align="end" className="w-48">
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

function ruleColumnClassName(columnId: string): string {
  switch (columnId) {
    case "keyword":
      return "min-w-52";
    case "rule_type":
      return "min-w-40";
    case "scope":
      return "min-w-44";
    case "status":
      return "min-w-40";
    case "actions":
      return "w-28 text-right";
    default:
      return "";
  }
}

function RuleTableMessage({ message }: { message: string }) {
  return (
    <TableRow>
      <TableCell
        className="h-40 text-center text-muted-foreground"
        colSpan={5}
      >
        {message}
      </TableCell>
    </TableRow>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
