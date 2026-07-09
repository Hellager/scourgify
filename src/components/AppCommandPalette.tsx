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

export function AppCommandPalette() {
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
        <CommandInput placeholder="Search commands" />
        <CommandList>
          <CommandEmpty>No commands found.</CommandEmpty>
          <CommandGroup heading="Navigation">
            <CommandItem onSelect={() => run(() => navigate("/"))}>
              <Gauge />
              Go to Dashboard
            </CommandItem>
            <CommandItem onSelect={() => run(() => navigate("/settings"))}>
              <Settings />
              Go to Settings
            </CommandItem>
            <CommandItem onSelect={() => run(() => navigate("/about"))}>
              <Info />
              Open About
            </CommandItem>
          </CommandGroup>
          <CommandSeparator />
          <CommandGroup heading="Appearance">
            <CommandItem
              onSelect={() => run(openConfigDrawer)}
            >
              <Paintbrush />
              Open Config Drawer
              <CommandShortcut>Drawer</CommandShortcut>
            </CommandItem>
          </CommandGroup>
          <CommandSeparator />
          <CommandGroup heading="Dashboard">
            <CommandItem
              disabled={!onDashboard}
              onSelect={() =>
                run(() => dispatchAppEvent(REFRESH_DASHBOARD_EVENT))
              }
            >
              <RefreshCw />
              Refresh Quick Access
              <CommandShortcut>Dashboard</CommandShortcut>
            </CommandItem>
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
