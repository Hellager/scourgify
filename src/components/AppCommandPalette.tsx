import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  Gauge,
  Info,
  Paintbrush,
  RefreshCw,
  Settings,
} from "lucide-react";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "@/components/ui/command";
import {
  dispatchAppEvent,
  OPEN_CONFIG_DRAWER_EVENT,
  REFRESH_DASHBOARD_EVENT,
} from "@/lib/app-events";
import { useI18n } from "@/lib/i18n";

export function AppCommandPalette() {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();
  const onDashboard = location.pathname === "/";

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setOpen((current) => !current);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const run = (action: () => void) => {
    setOpen(false);
    action();
  };

  const openConfigDrawer = () => {
    if (!onDashboard) {
      navigate("/");
    }
    window.setTimeout(
      () => dispatchAppEvent(OPEN_CONFIG_DRAWER_EVENT),
      onDashboard ? 0 : 50,
    );
  };

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <Command>
        <CommandInput placeholder={t("searchCommands")} />
        <CommandList>
          <CommandEmpty>{t("noCommands")}</CommandEmpty>
          <CommandGroup heading={t("commandNavigation")}>
            <CommandItem onSelect={() => run(() => navigate("/"))}>
              <Gauge />
              {t("goToDashboard")}
            </CommandItem>
            <CommandItem onSelect={() => run(() => navigate("/settings"))}>
              <Settings />
              {t("goToSettings")}
            </CommandItem>
            <CommandItem onSelect={() => run(() => navigate("/about"))}>
              <Info />
              {t("openAbout")}
            </CommandItem>
          </CommandGroup>
          <CommandSeparator />
          <CommandGroup heading={t("commandAppearance")}>
            <CommandItem
              onSelect={() => run(openConfigDrawer)}
            >
              <Paintbrush />
              {t("openConfigDrawer")}
              <CommandShortcut>{t("drawerShortcut")}</CommandShortcut>
            </CommandItem>
          </CommandGroup>
          <CommandSeparator />
          <CommandGroup heading={t("commandDashboard")}>
            <CommandItem
              disabled={!onDashboard}
              onSelect={() =>
                run(() => dispatchAppEvent(REFRESH_DASHBOARD_EVENT))
              }
            >
              <RefreshCw />
              {t("refreshDashboard")}
              <CommandShortcut>{t("dashboard")}</CommandShortcut>
            </CommandItem>
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
