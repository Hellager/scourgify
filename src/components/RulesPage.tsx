import { useCallback, useEffect, useState } from "react";
import { zodResolver } from "@hookform/resolvers/zod";
import { invoke } from "@tauri-apps/api/core";
import { Controller, useForm } from "react-hook-form";
import { Pencil, Plus, RefreshCw, ShieldCheck, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";
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
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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

const ruleFormSchema = z.object({
  keyword: z.string().trim().min(1),
  rule_type: z.enum(["whitelist", "blacklist"]),
  enabled: z.boolean(),
});

type RuleForm = z.infer<typeof ruleFormSchema>;

interface Rule extends RuleForm {
  id: number;
  created_at: string;
}

interface DatabaseStatus {
  available: boolean;
  path: string | null;
  schema_version: number | null;
  error: string | null;
}

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

const emptyRule: RuleForm = {
  keyword: "",
  rule_type: "whitelist",
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
        invoke<DatabaseStatus>("get_database_status"),
        invoke<PrivacyState>("privacy_state"),
      ]);
      setDatabase(databaseStatus);
      setPrivacyActive(privacyState !== "Inactive");
      setRules(
        databaseStatus.available ? await invoke<Rule[]>("get_rules") : [],
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

  const openCreate = () => {
    setEditingRule(null);
    reset(emptyRule);
    setFormOpen(true);
  };

  const openEdit = (rule: Rule) => {
    setEditingRule(rule);
    reset({
      keyword: rule.keyword,
      rule_type: rule.rule_type,
      enabled: rule.enabled,
    });
    setFormOpen(true);
  };

  const saveRule = handleSubmit(async (values) => {
    try {
      const saved = editingRule
        ? await invoke<Rule>("update_rule", {
            id: editingRule.id,
            rule: values,
          })
        : await invoke<Rule>("add_rule", { rule: values });
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

  const toggleRule = async (rule: Rule, enabled: boolean) => {
    setMutatingId(rule.id);
    try {
      const saved = await invoke<Rule>("toggle_rule", {
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
  };

  const deleteRule = async () => {
    if (!pendingDelete) {
      return;
    }

    const rule = pendingDelete;
    setMutatingId(rule.id);
    try {
      await invoke("remove_rule", { id: rule.id });
      setRules((current) => current.filter((item) => item.id !== rule.id));
      setPendingDelete(null);
      toast.success(t("ruleDeleted"));
    } catch (deleteError) {
      toast.error(errorMessage(deleteError));
    } finally {
      setMutatingId(null);
    }
  };

  return (
    <>
      <PageHeader
        actions={
          <>
          <Button
            aria-label={t("refreshRules")}
            disabled={loading}
            onClick={() => void loadRules()}
            size="icon-sm"
            title={t("refreshRules")}
            type="button"
            variant="outline"
          >
            <RefreshCw className={loading ? "animate-spin" : ""} />
          </Button>
          <Button disabled={writesDisabled} onClick={openCreate} type="button">
            <Plus />
            {t("addRule")}
          </Button>
          </>
        }
        subtitle={t("rulesSubtitle")}
        title={t("rules")}
      />

      <div className="mx-auto grid max-w-6xl gap-4 p-6">
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

        <section aria-labelledby="rules-list-title">
          <div className="mb-3 flex items-center justify-between gap-4">
            <h2 className="text-sm font-semibold" id="rules-list-title">
              {t("rules")}
            </h2>
            <span className="text-sm tabular-nums text-muted-foreground">
              {t("ruleCount", { count: rules.length })}
            </span>
          </div>
          <div className="overflow-x-auto rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("ruleKeyword")}</TableHead>
                  <TableHead>{t("ruleType")}</TableHead>
                  <TableHead>{t("ruleStatus")}</TableHead>
                  <TableHead className="w-28 text-right">{t("actions")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {loading ? (
                  <RuleTableMessage message={t("loadingRules")} />
                ) : rules.length === 0 ? (
                  <RuleTableMessage message={t("noRules")} />
                ) : (
                  rules.map((rule) => (
                    <TableRow key={rule.id}>
                      <TableCell className="max-w-md font-medium">
                        <span className="block truncate" title={rule.keyword}>
                          {rule.keyword}
                        </span>
                      </TableCell>
                      <TableCell>
                        <RuleTypeLabel ruleType={rule.rule_type} />
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <Switch
                            aria-label={t("ruleToggleLabel", {
                              keyword: rule.keyword,
                            })}
                            checked={rule.enabled}
                            disabled={
                              writesDisabled || mutatingId === rule.id
                            }
                            onCheckedChange={(enabled) =>
                              void toggleRule(rule, enabled)
                            }
                            size="sm"
                          />
                          <span className="text-sm text-muted-foreground">
                            {rule.enabled ? t("enabled") : t("disabled")}
                          </span>
                        </div>
                      </TableCell>
                      <TableCell>
                        <div className="flex justify-end gap-1">
                          <Button
                            aria-label={t("editRule")}
                            disabled={writesDisabled}
                            onClick={() => openEdit(rule)}
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
                            onClick={() => setPendingDelete(rule)}
                            size="icon-sm"
                            title={t("deleteRule")}
                            type="button"
                            variant="ghost"
                          >
                            <Trash2 />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
        </section>
      </div>

      <Dialog
        open={formOpen}
        onOpenChange={(open) => {
          setFormOpen(open);
          if (!open) {
            setEditingRule(null);
          }
        }}
      >
        <DialogContent>
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
                        <SelectValue />
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
        <AlertDialogContent>
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
        {whitelist ? t("whitelist") : t("blacklist")}
      </span>
    );
  }
}

function RuleTableMessage({ message }: { message: string }) {
  return (
    <TableRow>
      <TableCell
        className="h-40 text-center text-muted-foreground"
        colSpan={4}
      >
        {message}
      </TableCell>
    </TableRow>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
