import { z } from "zod";

export const autoCleanPolicySchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("disabled") }),
  z.object({ kind: z.literal("monitor") }),
  z.object({
    kind: z.literal("every_hours"),
    hours: z.number().int().min(1).max(168),
  }),
  z.object({
    kind: z.literal("daily_at"),
    hour: z.number().int().min(0).max(23),
    minute: z.number().int().min(0).max(59),
  }),
]);

export const configSchema = z.object({
  app_mode: z.enum(["dashboard", "grid", "tray"]),
  language: z.enum(["en-US", "zh-CN", "zh-TW", "fr-FR", "ru-RU"]),
  auto_start: z.boolean(),
  privacy_mode: z.boolean(),
  privacy_mode_cleanup_links: z.boolean(),
  close_behavior: z.enum(["hide", "quit"]),
  theme: z.enum(["system", "light", "dark"]),
  show_recent_files: z.boolean(),
  show_frequent_folders: z.boolean(),
  notifications_enabled: z.boolean(),
  notify_operation_complete: z.boolean(),
  notify_inactive_operation_complete: z.boolean(),
  notify_active_operation_complete: z.boolean(),
  notify_partial_failure: z.boolean(),
  confirm_destructive_actions: z.boolean(),
  smart_clean_confirm: z.boolean(),
  history_retention: z.number().int().min(0).max(1_000_000),
  auto_clean: autoCleanPolicySchema,
  auto_clean_last_run: z.string().datetime({ offset: true }).nullable(),
});

export type ConfigForm = z.infer<typeof configSchema>;
export type AutoCleanPolicy = z.infer<typeof autoCleanPolicySchema>;

export const defaultConfig: ConfigForm = {
  app_mode: "dashboard",
  language: "en-US",
  auto_start: false,
  privacy_mode: false,
  privacy_mode_cleanup_links: true,
  close_behavior: "hide",
  theme: "system",
  show_recent_files: true,
  show_frequent_folders: true,
  notifications_enabled: true,
  notify_operation_complete: true,
  notify_inactive_operation_complete: true,
  notify_active_operation_complete: false,
  notify_partial_failure: true,
  confirm_destructive_actions: true,
  smart_clean_confirm: true,
  history_retention: 0,
  auto_clean: { kind: "disabled" },
  auto_clean_last_run: null,
};
