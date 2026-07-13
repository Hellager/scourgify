import { z } from "zod";

export const autoCleanScheduleSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("disabled") }),
  z.object({ kind: z.literal("on_startup") }),
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
  app_mode: z.enum(["minimal", "dashboard"]),
  language: z.enum(["en-US", "zh-CN", "zh-TW", "fr-FR", "ru-RU"]),
  auto_start: z.boolean(),
  privacy_mode: z.boolean(),
  privacy_mode_cleanup_links: z.boolean(),
  close_behavior: z.enum(["hide", "quit"]),
  theme: z.enum(["system", "light", "dark"]),
  sidebar_variant: z.enum(["sidebar", "inset", "floating"]),
  show_recent_files: z.boolean(),
  show_frequent_folders: z.boolean(),
  notifications_enabled: z.boolean(),
  notify_operation_complete: z.boolean(),
  notify_inactive_operation_complete: z.boolean(),
  notify_active_operation_complete: z.boolean(),
  notify_partial_failure: z.boolean(),
  confirm_destructive_actions: z.boolean(),
  smart_clean_confirm: z.boolean(),
  history_retention: z.number().int().nonnegative(),
  auto_clean: autoCleanScheduleSchema,
  auto_clean_last_run: z.string().datetime({ offset: true }).nullable(),
});

export type ConfigForm = z.infer<typeof configSchema>;
export type AutoCleanSchedule = z.infer<typeof autoCleanScheduleSchema>;
export type SidebarVariant = ConfigForm["sidebar_variant"];

export const defaultConfig: ConfigForm = {
  app_mode: "dashboard",
  language: "en-US",
  auto_start: false,
  privacy_mode: false,
  privacy_mode_cleanup_links: true,
  close_behavior: "hide",
  theme: "system",
  sidebar_variant: "sidebar",
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
