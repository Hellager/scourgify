import { type ReactNode, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Controller, type Control, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Link } from "react-router-dom";
import { ArrowLeft, Save } from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";
import packageJson from "../../package.json";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

const GITHUB_URL = "https://github.com/hellager/scourgify";

const configSchema = z.object({
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
  notify_partial_failure: z.boolean(),
  confirm_destructive_actions: z.boolean(),
});

type ConfigForm = z.infer<typeof configSchema>;
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

const defaultConfig: ConfigForm = {
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
  notify_partial_failure: true,
  confirm_destructive_actions: true,
};

export function SettingsPage() {
  const [loading, setLoading] = useState(true);
  const {
    control,
    formState,
    handleSubmit,
    reset,
  } = useForm<ConfigForm>({
    resolver: zodResolver(configSchema),
    defaultValues: defaultConfig,
  });

  useEffect(() => {
    let active = true;

    invoke<ConfigForm>("get_config")
      .then((config) => {
        if (active) {
          reset(configSchema.parse(config));
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

  const save = handleSubmit(async (values) => {
    try {
      const saved = await invoke<ConfigForm>("update_config", {
        nextConfig: values,
      });
      reset(configSchema.parse(saved));
      toast.success("Settings saved.");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  });

  return (
    <main className="min-h-screen bg-background text-foreground">
      <header className="flex h-14 items-center justify-between border-b px-6">
        <div className="flex items-center gap-3">
          <Button
            render={<Link to="/" />}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <ArrowLeft />
            <span className="sr-only">Back to Dashboard</span>
          </Button>
          <div>
            <h1 className="text-base font-semibold">Settings</h1>
            <p className="text-xs text-muted-foreground">
              Scourgify preferences
            </p>
          </div>
        </div>
        <Button
          disabled={loading || formState.isSubmitting}
          form="settings-form"
          type="submit"
        >
          <Save />
          Save
        </Button>
      </header>

      <form
        className="mx-auto grid max-w-5xl gap-4 p-6"
        id="settings-form"
        onSubmit={(event) => void save(event)}
      >
        <Section title="General">
          <SelectControl
            control={control}
            field={{
              label: "Run mode",
              name: "app_mode",
              options: [
                { label: "Dashboard", value: "dashboard" },
                { label: "Minimal", value: "minimal" },
              ],
            }}
          />
          <SelectControl
            control={control}
            field={{
              label: "Language",
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
              label: "Close window",
              name: "close_behavior",
              options: [
                { label: "Hide to tray", value: "hide" },
                { label: "Quit app", value: "quit" },
              ],
            }}
          />
          <SwitchControl
            control={control}
            field={{ label: "Auto start", name: "auto_start" }}
          />
        </Section>

        <Section title="Privacy">
          <SwitchControl
            control={control}
            field={{
              label: "Restore privacy mode on startup",
              name: "privacy_mode",
              description: "Does not toggle the current lock immediately.",
            }}
          />
          <SwitchControl
            control={control}
            field={{
              label: "Clean new .lnk files on unlock",
              name: "privacy_mode_cleanup_links",
            }}
          />
        </Section>

        <Section title="Appearance">
          <SelectControl
            control={control}
            field={{
              label: "Theme",
              name: "theme",
              options: [
                { label: "System", value: "system" },
                { label: "Light", value: "light" },
                { label: "Dark", value: "dark" },
              ],
            }}
          />
          <SelectControl
            control={control}
            field={{
              label: "Sidebar style",
              name: "sidebar_variant",
              options: [
                { label: "Sidebar", value: "sidebar" },
                { label: "Inset", value: "inset" },
                { label: "Floating", value: "floating" },
              ],
            }}
          />
          <SwitchControl
            control={control}
            field={{ label: "Show recent files", name: "show_recent_files" }}
          />
          <SwitchControl
            control={control}
            field={{
              label: "Show frequent folders",
              name: "show_frequent_folders",
            }}
          />
        </Section>

        <Section title="Notifications">
          <SwitchControl
            control={control}
            field={{
              label: "Enable notifications",
              name: "notifications_enabled",
            }}
          />
          <SwitchControl
            control={control}
            field={{
              label: "Notify operation complete",
              name: "notify_operation_complete",
            }}
          />
          <SwitchControl
            control={control}
            field={{
              label: "Notify partial failures",
              name: "notify_partial_failure",
            }}
          />
          <SwitchControl
            control={control}
            field={{
              label: "Confirm destructive actions",
              name: "confirm_destructive_actions",
            }}
          />
        </Section>

        <Section title="About">
          <InfoRow label="Version" value={packageJson.version} />
          <InfoRow label="Author" value="Stein Gu" />
          <InfoRow label="License" value="MIT" />
          <div className="flex items-center justify-between gap-4 py-2">
            <span className="text-sm text-muted-foreground">GitHub</span>
            <Button
              onClick={() => void openUrl(GITHUB_URL)}
              size="sm"
              type="button"
              variant="outline"
            >
              Open
            </Button>
          </div>
        </Section>
      </form>
    </main>
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
  field,
}: {
  control: Control<ConfigForm>;
  field: SwitchField;
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
            onCheckedChange={formField.onChange}
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
