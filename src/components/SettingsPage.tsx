import { type ReactNode, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Controller, type Control, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { LoaderCircle, Play } from "lucide-react";
import { toast } from "sonner";
import packageJson from "../../package.json";
import { useAppShell } from "@/components/AppShell";
import { Button } from "@/components/ui/button";
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
  type AutoCleanPolicy,
  configSchema,
  defaultConfig,
  type ConfigForm,
} from "@/lib/config";
import {
  type AutoCleanFinished,
  AUTO_CLEAN_UPDATED_EVENT,
} from "@/lib/app-events";
import { useI18n } from "@/lib/i18n";
import { requestNotificationPermission } from "@/lib/notifications";
import { invokeCommand } from "@/lib/commands";

const GITHUB_URL = "https://github.com/hellager/scourgify";

type SelectField = {
  label: string;
  name: keyof ConfigForm;
  options: Array<{ label: string; value: string }>;
};
type SwitchField = {
  label: string;
  name: keyof ConfigForm;
  description?: string;
};
type VisibilityQaType = "recent" | "frequent" | "start_recommended";

interface QaVisibility {
  recent: boolean;
  frequent: boolean;
  start_recommended: boolean;
}

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

interface AutoCleanResult {
  total: number;
  succeeded: number;
  failed: number;
  warnings: number;
  section_errors: number;
  history_errors: number;
}

export function SettingsPage() {
  const { language, t } = useI18n();
  const { setConfig: setShellConfig } = useAppShell();
  const [loading, setLoading] = useState(true);
  const [privacyActive, setPrivacyActive] = useState(false);
  const [runningAutoClean, setRunningAutoClean] = useState(false);
  const [startRecommendedVisible, setStartRecommendedVisible] = useState(true);
  const [updatingVisibility, setUpdatingVisibility] =
    useState<VisibilityQaType | null>(null);
  const autoSaveReady = useRef(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saveRef = useRef<() => void>(() => undefined);
  const saveQueue = useRef(Promise.resolve());
  const lastSavedConfig = useRef<ConfigForm | null>(null);
  const originalVisibility = useRef<QaVisibility | null>(null);
  const {
    control,
    formState,
    getValues,
    handleSubmit,
    register,
    reset,
    setValue,
    watch,
  } = useForm<ConfigForm>({
    resolver: zodResolver(configSchema),
    defaultValues: defaultConfig,
  });
  const notificationsEnabled = watch("notifications_enabled");
  const notifyOperationComplete = watch("notify_operation_complete");
  const autoCleanPolicy = watch("auto_clean");
  const autoCleanLastRun = watch("auto_clean_last_run");

  useEffect(() => {
    let active = true;

    Promise.all([
      invokeCommand<ConfigForm>("get_config"),
      invokeCommand<QaVisibility>("get_qa_visibility"),
      invokeCommand<PrivacyState>("privacy_state"),
    ])
      .then(([config, visibility, privacyState]) => {
        if (active) {
          originalVisibility.current = visibility;
          setPrivacyActive(privacyState !== "Inactive");
          setStartRecommendedVisible(visibility.start_recommended);
          const parsed = configSchema.parse({
            ...config,
            show_recent_files: visibility.recent,
            show_frequent_folders: visibility.frequent,
          });
          lastSavedConfig.current = parsed;
          reset(parsed);
          autoSaveReady.current = true;
        }
      })
      .catch((error) => toast.error(errorMessage(error)))
      .finally(() => {
        if (active) {
          setLoading(false);
        }
      });

    return () => {
      active = false;
    };
  }, [reset]);

  useEffect(() => {
    const updateLastRun = (event: Event) => {
      const { completed_at } = (event as CustomEvent<AutoCleanFinished>).detail;
      setValue("auto_clean_last_run", completed_at, { shouldDirty: false });
    };
    window.addEventListener(AUTO_CLEAN_UPDATED_EVENT, updateLastRun);
    return () =>
      window.removeEventListener(AUTO_CLEAN_UPDATED_EVENT, updateLastRun);
  }, [setValue]);

  const updateAutoCleanPolicy = (policy: AutoCleanPolicy) => {
    setValue("auto_clean", policy, {
      shouldDirty: true,
      shouldValidate: true,
    });
  };

  const setAutoCleanEnabled = (enabled: boolean) => {
    updateAutoCleanPolicy(
      enabled ? { kind: "monitor" } : { kind: "disabled" },
    );
  };

  const setAutoCleanMode = (
    kind: Exclude<AutoCleanPolicy["kind"], "disabled">,
  ) => {
    if (kind === "monitor") {
      updateAutoCleanPolicy({ kind });
    } else if (kind === "every_hours") {
      updateAutoCleanPolicy({
        kind,
        hours: autoCleanPolicy.kind === kind ? autoCleanPolicy.hours : 6,
      });
    } else {
      updateAutoCleanPolicy({
        kind,
        hour: autoCleanPolicy.kind === kind ? autoCleanPolicy.hour : 8,
        minute: autoCleanPolicy.kind === kind ? autoCleanPolicy.minute : 0,
      });
    }
  };

  const runAutoClean = async () => {
    setRunningAutoClean(true);
    try {
      const result = await invokeCommand<AutoCleanResult>("run_auto_clean_now");
      if (result.failed || result.warnings || result.section_errors || result.history_errors) {
        toast.warning(
          t("autoCleanCompletedWithIssues", {
            succeeded: result.succeeded,
            failed: result.failed,
            warnings: result.warnings,
            sectionErrors: result.section_errors,
            historyErrors: result.history_errors,
          }),
        );
      } else {
        toast.success(
          t("autoCleanCompleted", {
            succeeded: result.succeeded,
            total: result.total,
          }),
        );
      }
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setRunningAutoClean(false);
    }
  };

  const updateVisibility = async (qaType: VisibilityQaType, visible: boolean) => {
    const fieldName =
      qaType === "recent"
        ? "show_recent_files"
        : qaType === "frequent"
          ? "show_frequent_folders"
          : null;

    if (qaType === "start_recommended") {
      setStartRecommendedVisible(visible);
    }

    if (privacyActive) {
      restoreVisibilityValue(qaType, visible);
      toast.warning(t("privacyWriteDisabled"));
      return;
    }

    setUpdatingVisibility(qaType);
    try {
      const actual = await invokeCommand<QaVisibility>("set_qa_visibility", {
        qaType,
        visible,
      });
      applyActualVisibility(actual);
      toast.success(t("quickAccessVisibilityUpdated"));
    } catch (error) {
      try {
        applyActualVisibility(
          await invokeCommand<QaVisibility>("get_qa_visibility"),
        );
      } catch {
        restoreVisibilityValue(qaType, visible);
      }
      toast.error(errorMessage(error));
    } finally {
      setUpdatingVisibility(null);
    }

    function restoreVisibilityValue(
      target: VisibilityQaType,
      attemptedValue: boolean,
    ) {
      const previous = originalVisibility.current?.[target] ?? !attemptedValue;
      if (fieldName) {
        setValue(fieldName, previous, { shouldValidate: true });
      } else {
        setStartRecommendedVisible(previous);
      }
    }

    function applyActualVisibility(actual: QaVisibility) {
      originalVisibility.current = actual;
      setValue("show_recent_files", actual.recent, { shouldValidate: true });
      setValue("show_frequent_folders", actual.frequent, {
        shouldValidate: true,
      });
      setStartRecommendedVisible(actual.start_recommended);
    }
  };

  const persist = async (values: ConfigForm) => {
    try {
      const previousConfig = lastSavedConfig.current;
      const visibilityChanged =
        originalVisibility.current &&
        (originalVisibility.current.recent !== values.show_recent_files ||
          originalVisibility.current.frequent !== values.show_frequent_folders);
      if (privacyActive && visibilityChanged) {
        toast.warning(t("privacyWriteDisabled"));
        return;
      }
      if (originalVisibility.current?.recent !== values.show_recent_files) {
        await invokeCommand("set_qa_visibility", {
          qaType: "recent",
          visible: values.show_recent_files,
        });
      }
      if (originalVisibility.current?.frequent !== values.show_frequent_folders) {
        await invokeCommand("set_qa_visibility", {
          qaType: "frequent",
          visible: values.show_frequent_folders,
        });
      }
      const saved = await invokeCommand<ConfigForm>("update_config", {
        nextConfig: values,
      });
      const parsed = configSchema.parse(saved);
      lastSavedConfig.current = parsed;
      if (JSON.stringify(getValues()) === JSON.stringify(values)) {
        reset(parsed);
      } else {
        setValue("auto_clean_last_run", parsed.auto_clean_last_run, {
          shouldDirty: false,
        });
      }
      setShellConfig(parsed);
      originalVisibility.current = {
        recent: values.show_recent_files,
        frequent: values.show_frequent_folders,
        start_recommended:
          originalVisibility.current?.start_recommended ??
          startRecommendedVisible,
      };
      if (visibilityChanged) {
        toast.success(t("quickAccessVisibilityUpdated"));
      }
      if (
        values.notifications_enabled &&
        !previousConfig?.notifications_enabled &&
        !(await requestNotificationPermission())
      ) {
        toast.warning(t("notificationPermissionDenied"));
      }
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const save = handleSubmit((values) => {
    saveQueue.current = saveQueue.current.then(() => persist(values));
    return saveQueue.current;
  });
  saveRef.current = () => void save();

  useEffect(() => {
    const subscription = watch((_values, { name }) => {
      if (
        !autoSaveReady.current ||
        !name ||
        name === "auto_clean_last_run"
      ) {
        return;
      }
      if (saveTimer.current) {
        clearTimeout(saveTimer.current);
      }
      saveTimer.current = setTimeout(() => saveRef.current(), 300);
    });

    return () => {
      subscription.unsubscribe();
      if (saveTimer.current) {
        clearTimeout(saveTimer.current);
      }
    };
  }, [watch]);

  return (
    <div className="mx-auto grid max-w-5xl gap-4 p-6">
        <Section title={t("general")}>
          <SelectControl
            control={control}
            field={{
              label: t("appMode"),
              name: "app_mode",
              options: [
                { label: t("appModeDashboard"), value: "dashboard" },
                { label: t("appModeGrid"), value: "grid" },
                { label: t("appModeTray"), value: "tray" },
              ],
            }}
          />
          <SelectControl
            control={control}
            field={{
              label: t("displayLanguage"),
              name: "language",
              options: [
                { label: "English", value: "en-US" },
                { label: "简体中文", value: "zh-CN" },
                { label: "繁體中文", value: "zh-TW" },
                { label: "Français", value: "fr-FR" },
                { label: "Русский", value: "ru-RU" },
              ],
            }}
          />
          <SelectControl
            control={control}
            field={{
              label: t("closeWindow"),
              name: "close_behavior",
              options: [
                { label: t("hideToTray"), value: "hide" },
                { label: t("quitApp"), value: "quit" },
              ],
            }}
          />
          <SelectControl
            control={control}
            field={{
              label: t("theme"),
              name: "theme",
              options: [
                { label: t("system"), value: "system" },
                { label: t("light"), value: "light" },
                { label: t("dark"), value: "dark" },
              ],
            }}
          />
          <SwitchControl
            control={control}
            field={{ label: t("autoStart"), name: "auto_start" }}
          />
        </Section>

        <Section title={t("privacy")}>
          <SwitchControl
            control={control}
            field={{
              label: t("restorePrivacyOnStartup"),
              name: "privacy_mode",
              description: t("restorePrivacyDescription"),
            }}
          />
          <SwitchControl
            control={control}
            field={{
              label: t("cleanLinks"),
              name: "privacy_mode_cleanup_links",
            }}
          />
        </Section>

        <Section title={t("autoClean")}>
          <label className="flex items-center justify-between gap-4 py-3">
            <span className="text-sm">{t("autoCleanEnabled")}</span>
            <Switch
              checked={autoCleanPolicy.kind !== "disabled"}
              disabled={loading || formState.isSubmitting}
              onCheckedChange={setAutoCleanEnabled}
            />
          </label>

          {autoCleanPolicy.kind !== "disabled" ? (
            <>
              <label className="grid gap-2 py-3 sm:grid-cols-[minmax(0,1fr)_12rem] sm:items-center">
                <span className="text-sm">{t("autoCleanMode")}</span>
                <Select
                  onValueChange={(value) =>
                    setAutoCleanMode(
                      value as Exclude<AutoCleanPolicy["kind"], "disabled">,
                    )
                  }
                  value={autoCleanPolicy.kind}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="monitor">
                      {t("autoCleanMonitor")}
                    </SelectItem>
                    <SelectItem value="every_hours">
                      {t("autoCleanEveryHours")}
                    </SelectItem>
                    <SelectItem value="daily_at">
                      {t("autoCleanDailyAt")}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </label>

              {autoCleanPolicy.kind === "every_hours" ? (
                <label className="grid gap-2 py-3 sm:grid-cols-[minmax(0,1fr)_12rem] sm:items-center">
                  <span className="text-sm">{t("autoCleanInterval")}</span>
                  <Input
                    max={168}
                    min={1}
                    onChange={(event) => {
                      const hours = event.currentTarget.valueAsNumber;
                      if (!Number.isNaN(hours)) {
                        updateAutoCleanPolicy({ kind: "every_hours", hours });
                      }
                    }}
                    type="number"
                    value={autoCleanPolicy.hours}
                  />
                </label>
              ) : null}

              {autoCleanPolicy.kind === "daily_at" ? (
                <label className="grid gap-2 py-3 sm:grid-cols-[minmax(0,1fr)_12rem] sm:items-center">
                  <span className="text-sm">{t("autoCleanTime")}</span>
                  <Input
                    onChange={(event) => {
                      const [hour, minute] = event.currentTarget.value
                        .split(":")
                        .map(Number);
                      if (Number.isInteger(hour) && Number.isInteger(minute)) {
                        updateAutoCleanPolicy({ kind: "daily_at", hour, minute });
                      }
                    }}
                    type="time"
                    value={`${String(autoCleanPolicy.hour).padStart(2, "0")}:${String(autoCleanPolicy.minute).padStart(2, "0")}`}
                  />
                </label>
              ) : null}
            </>
          ) : null}

          {formState.errors.auto_clean ? (
            <p className="py-2 text-sm text-destructive">
              {t("autoCleanInvalidSchedule")}
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-between gap-3 py-3">
            <span>
              <span className="block text-sm">{t("autoCleanLastRun")}</span>
              <span className="block text-xs text-muted-foreground">
                {formatLastRun(
                  autoCleanLastRun,
                  language,
                  t("autoCleanNever"),
                )}
              </span>
            </span>
            <Button
              className="min-w-36"
              disabled={loading || runningAutoClean || privacyActive}
              onClick={() => void runAutoClean()}
              title={privacyActive ? t("privacyWriteDisabled") : undefined}
              type="button"
              variant="outline"
            >
              {runningAutoClean ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <Play />
              )}
              {t(runningAutoClean ? "autoCleanRunning" : "autoCleanRunNow")}
            </Button>
          </div>
        </Section>

        <Section title={t("visibility")}>
          <SwitchControl
            control={control}
            disabled={
              loading || privacyActive || updatingVisibility !== null
            }
            field={{ label: t("showRecentFiles"), name: "show_recent_files" }}
            onCheckedChange={(checked) =>
              void updateVisibility("recent", checked)
            }
          />
          <SwitchControl
            control={control}
            disabled={
              loading || privacyActive || updatingVisibility !== null
            }
            field={{
              label: t("showFrequentFolders"),
              name: "show_frequent_folders",
            }}
            onCheckedChange={(checked) =>
              void updateVisibility("frequent", checked)
            }
          />
          <label className="flex items-center justify-between gap-4 py-3">
            <span>
              <span className="block text-sm">{t("showStartRecommended")}</span>
              <span className="block text-xs text-muted-foreground">
                {t("showStartRecommendedDescription")}
              </span>
            </span>
            <Switch
              checked={startRecommendedVisible}
              disabled={
                loading || privacyActive || updatingVisibility !== null
              }
              onCheckedChange={(checked) =>
                void updateVisibility("start_recommended", checked)
              }
            />
          </label>
        </Section>

        <Section title={t("notifications")}>
          <SwitchControl
            control={control}
            field={{
              label: t("enableNotifications"),
              name: "notifications_enabled",
            }}
          />
          <SwitchControl
            control={control}
            disabled={!notificationsEnabled}
            field={{
              label: t("notifyComplete"),
              name: "notify_operation_complete",
            }}
          />
          <SwitchControl
            control={control}
            disabled={!notificationsEnabled || !notifyOperationComplete}
            field={{
              label: t("inactiveNotification"),
              name: "notify_inactive_operation_complete",
            }}
          />
          <SwitchControl
            control={control}
            disabled={!notificationsEnabled || !notifyOperationComplete}
            field={{
              label: t("notifyActive"),
              name: "notify_active_operation_complete",
              description: t("notifyActiveDescription"),
            }}
          />
          <SwitchControl
            control={control}
            disabled={!notificationsEnabled}
            field={{
              label: t("notifyPartialFailure"),
              name: "notify_partial_failure",
            }}
          />
          <SwitchControl
            control={control}
            field={{
              label: t("confirmDestructiveActions"),
              name: "confirm_destructive_actions",
            }}
          />
          <SwitchControl
            control={control}
            field={{
              label: t("confirmSmartClean"),
              name: "smart_clean_confirm",
              description: t("confirmSmartCleanDescription"),
            }}
          />
        </Section>

        <Section title={t("history")}>
          <label className="flex items-center justify-between gap-4 py-3">
            <span>
              <span className="block text-sm">{t("historyRetention")}</span>
              <span className="block text-xs text-muted-foreground">
                {t("historyRetentionDescription")}
              </span>
            </span>
            <Input
              className="w-28"
              max={1_000_000}
              min={0}
              type="number"
              {...register("history_retention", { valueAsNumber: true })}
            />
          </label>
        </Section>

        <Section title={t("about")}>
          <InfoRow label={t("version")} value={packageJson.version} />
          <InfoRow label={t("author")} value="Stein Gu" />
          <InfoRow label={t("license")} value="MIT" />
          <div className="flex items-center justify-between gap-4 py-2">
            <span className="text-sm text-muted-foreground">{t("github")}</span>
            <Button
              onClick={() => void openUrl(GITHUB_URL)}
              size="sm"
              type="button"
              variant="outline"
            >
              {t("open")}
            </Button>
          </div>
          <div className="flex items-center justify-between gap-4 py-2">
            <span className="text-sm text-muted-foreground">
              {t("diagnostics")}
            </span>
            <Button
              onClick={() =>
                void invokeCommand("open_log_directory").catch((error) =>
                  toast.error(errorMessage(error)),
                )
              }
              size="sm"
              type="button"
              variant="outline"
            >
              {t("openLogDirectory")}
            </Button>
          </div>
        </Section>
    </div>
  );
}

function Section({
  children,
  title,
}: {
  children: ReactNode;
  title: string;
}) {
  return (
    <section className="rounded-md border bg-card p-4 text-card-foreground">
      <h2 className="text-sm font-semibold">{title}</h2>
      <div className="mt-3 divide-y">{children}</div>
    </section>
  );
}

function SelectControl({
  control,
  field,
}: {
  control: Control<ConfigForm>;
  field: SelectField;
}) {
  return (
    <Controller
      control={control}
      name={field.name}
      render={({ field: formField }) => (
        <label className="flex items-center justify-between gap-4 py-3">
          <span className="text-sm">{field.label}</span>
          <Select
            onValueChange={formField.onChange}
            value={String(formField.value)}
          >
            <SelectTrigger className="w-48">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {field.options.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
      )}
    />
  );
}

function SwitchControl({
  control,
  disabled,
  field,
  onCheckedChange,
}: {
  control: Control<ConfigForm>;
  disabled?: boolean;
  field: SwitchField;
  onCheckedChange?: (checked: boolean) => void;
}) {
  return (
    <Controller
      control={control}
      name={field.name}
      render={({ field: formField }) => (
        <label className="flex items-center justify-between gap-4 py-3">
          <span>
            <span className="block text-sm">{field.label}</span>
            {field.description ? (
              <span className="block text-xs text-muted-foreground">
                {field.description}
              </span>
            ) : null}
          </span>
          <Switch
            checked={Boolean(formField.value)}
            disabled={disabled}
            inputRef={formField.ref}
            name={formField.name}
            onBlur={formField.onBlur}
            onCheckedChange={(checked) => {
              formField.onChange(checked);
              onCheckedChange?.(checked);
            }}
          />
        </label>
      )}
    />
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4 py-3">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="text-sm font-medium">{value}</span>
    </div>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function formatLastRun(value: string | null, language: string, fallback: string) {
  if (!value) {
    return fallback;
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? fallback
    : new Intl.DateTimeFormat(language, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(date);
}
