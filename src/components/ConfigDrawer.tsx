import { useEffect, useRef, useState } from "react";
import { Save } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Sheet,
  SheetContent,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Switch } from "@/components/ui/switch";
import { configSchema, type ConfigForm } from "@/lib/config";
import { useI18n } from "@/lib/i18n";
import { invokeCommand } from "@/lib/commands";

type VisibilityQaType = "recent" | "frequent";

interface QaVisibility {
  recent: boolean;
  frequent: boolean;
}

interface ConfigDrawerProps {
  config: ConfigForm;
  onConfigSaved: (config: ConfigForm) => void;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  privacyActive: boolean;
}

export function ConfigDrawer({
  config,
  onConfigSaved,
  onOpenChange,
  open,
  privacyActive,
}: ConfigDrawerProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState(config);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const originalVisibility = useRef<QaVisibility | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    let active = true;
    setLoading(true);
    invokeCommand<QaVisibility>("get_qa_visibility")
      .then((visibility) => {
        if (!active) {
          return;
        }
        originalVisibility.current = visibility;
        setDraft(
          configSchema.parse({
            ...config,
            show_recent_files: visibility.recent,
            show_frequent_folders: visibility.frequent,
          }),
        );
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
  }, [config, open]);

  const updateDraft = <K extends keyof ConfigForm>(
    key: K,
    value: ConfigForm[K],
  ) => {
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const updateVisibility = async (qaType: VisibilityQaType, visible: boolean) => {
    const fieldName =
      qaType === "recent" ? "show_recent_files" : "show_frequent_folders";

    if (privacyActive) {
      updateDraft(fieldName, originalVisibility.current?.[qaType] ?? !visible);
      toast.warning(t("privacyWriteDisabled"));
      return;
    }

    updateDraft(fieldName, visible);
    try {
      await invokeCommand("set_qa_visibility", { qaType, visible });
      originalVisibility.current = {
        recent: originalVisibility.current?.recent ?? true,
        frequent: originalVisibility.current?.frequent ?? true,
        [qaType]: visible,
      };
      toast.success(t("quickAccessVisibilityUpdated"));
    } catch (error) {
      updateDraft(fieldName, originalVisibility.current?.[qaType] ?? !visible);
      toast.error(errorMessage(error));
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      const saved = configSchema.parse(
        await invokeCommand<ConfigForm>("update_config", {
          nextConfig: draft,
        }),
      );
      setDraft(saved);
      onConfigSaved(saved);
      toast.success(t("appearanceSaved"));
      onOpenChange(false);
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full sm:max-w-md">
        <SheetHeader>
          <SheetTitle>{t("appearance")}</SheetTitle>
        </SheetHeader>
        <div className="grid gap-1 px-4">
          <label className="flex items-center justify-between gap-4 py-3">
            <span className="text-sm">{t("theme")}</span>
            <Select
              disabled={loading || saving}
              onValueChange={(value) =>
                updateDraft("theme", value as ConfigForm["theme"])
              }
              value={draft.theme}
            >
              <SelectTrigger className="w-44">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="system">{t("system")}</SelectItem>
                <SelectItem value="light">{t("light")}</SelectItem>
                <SelectItem value="dark">{t("dark")}</SelectItem>
              </SelectContent>
            </Select>
          </label>
          <label className="flex items-center justify-between gap-4 border-t py-3">
            <span className="text-sm">{t("sidebarStyle")}</span>
            <Select
              disabled={loading || saving}
              onValueChange={(value) =>
                updateDraft(
                  "sidebar_variant",
                  value as ConfigForm["sidebar_variant"],
                )
              }
              value={draft.sidebar_variant}
            >
              <SelectTrigger className="w-44">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="sidebar">{t("sidebar")}</SelectItem>
                <SelectItem value="inset">{t("inset")}</SelectItem>
                <SelectItem value="floating">{t("floating")}</SelectItem>
              </SelectContent>
            </Select>
          </label>
          <SwitchRow
            checked={draft.show_recent_files}
            disabled={loading || saving || privacyActive}
            label={t("showRecentFiles")}
            onCheckedChange={(checked) =>
              void updateVisibility("recent", checked)
            }
          />
          <SwitchRow
            checked={draft.show_frequent_folders}
            disabled={loading || saving || privacyActive}
            label={t("showFrequentFolders")}
            onCheckedChange={(checked) =>
              void updateVisibility("frequent", checked)
            }
          />
        </div>
        <SheetFooter>
          <Button disabled={loading || saving} onClick={() => void save()}>
            <Save />
            {t("save")}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}

function SwitchRow({
  checked,
  disabled,
  label,
  onCheckedChange,
}: {
  checked: boolean;
  disabled?: boolean;
  label: string;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-4 border-t py-3">
      <span className="text-sm">{label}</span>
      <Switch
        checked={checked}
        disabled={disabled}
        onCheckedChange={onCheckedChange}
      />
    </label>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
