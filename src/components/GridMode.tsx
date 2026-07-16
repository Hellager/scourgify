import { useEffect, useState } from "react";
import {
  Clock3,
  FileClock,
  FolderClock,
  Gauge,
  ListChecks,
  PanelTopClose,
  Play,
  Settings,
  Shield,
  ShieldCheck,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { invokeCommand } from "@/lib/commands";
import { useI18n } from "@/lib/i18n";

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

export function GridMode() {
  const { t } = useI18n();
  const [busy, setBusy] = useState<string | null>(null);
  const [privacyActive, setPrivacyActive] = useState(false);

  useEffect(() => {
    void invokeCommand<PrivacyState>("privacy_state")
      .then((state) => setPrivacyActive(state !== "Inactive"))
      .catch(() => setPrivacyActive(false));
  }, []);

  const openDashboard = async (path = "/") => {
    await invokeCommand("set_app_mode", { mode: "dashboard" });
    window.location.hash = `#${path}`;
  };

  const run = async (key: string, action: () => Promise<unknown>) => {
    setBusy(key);
    try {
      await action();
      toast.success(t("complete"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(null);
    }
  };

  const togglePrivacy = async () => {
    await invokeCommand(privacyActive ? "privacy_exit" : "privacy_enter");
    setPrivacyActive(!privacyActive);
  };

  const actions = [
    { key: "dashboard", icon: Gauge, label: t("dashboard"), action: () => openDashboard() },
    {
      key: "recent",
      icon: FileClock,
      label: `${t("actionSmartClean")} · ${t("recent")}`,
      action: () => invokeCommand("smart_clean", { qaType: "recent" }),
    },
    {
      key: "frequent",
      icon: FolderClock,
      label: `${t("actionSmartClean")} · ${t("frequent")}`,
      action: () => invokeCommand("smart_clean", { qaType: "frequent" }),
    },
    {
      key: "auto",
      icon: Play,
      label: t("autoCleanRunNow"),
      action: () => invokeCommand("run_auto_clean_now"),
    },
    {
      key: "privacy",
      icon: privacyActive ? ShieldCheck : Shield,
      label: t("privacy"),
      action: togglePrivacy,
    },
    { key: "rules", icon: ListChecks, label: t("rules"), action: () => openDashboard("/rules") },
    { key: "history", icon: Clock3, label: t("history"), action: () => openDashboard("/history") },
    { key: "settings", icon: Settings, label: t("settings"), action: () => openDashboard("/settings") },
    {
      key: "tray",
      icon: PanelTopClose,
      label: t("appModeTray"),
      action: () => invokeCommand("set_app_mode", { mode: "tray" }),
    },
  ];

  return (
    <main className="grid min-h-screen place-items-center bg-background p-5 text-foreground">
      <div className="grid w-full max-w-lg grid-cols-3 gap-3">
        {actions.map(({ action, icon: Icon, key, label }) => (
          <Button
            className="h-36 min-w-0 flex-col gap-3 whitespace-normal rounded-md px-3 text-center"
            disabled={busy !== null}
            key={key}
            onClick={() => void run(key, action)}
            type="button"
            variant="outline"
          >
            <Icon className="size-6 shrink-0" />
            <span className="line-clamp-2 text-sm">{label}</span>
          </Button>
        ))}
      </div>
    </main>
  );
}
