import {
  createContext,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { Link, Outlet, useLocation } from "react-router-dom";
import {
  Gauge,
  History,
  Info,
  Paintbrush,
  Settings,
  ShieldCheck,
} from "lucide-react";
import { toast } from "sonner";
import { AppCommandPalette } from "@/components/AppCommandPalette";
import { ConfigDrawer } from "@/components/ConfigDrawer";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarSeparator,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import {
  type AutoCleanFinished,
  AUTO_CLEAN_UPDATED_EVENT,
  dispatchAppEvent,
  OPEN_CONFIG_DRAWER_EVENT,
  REFRESH_DASHBOARD_EVENT,
  REFRESH_HISTORY_EVENT,
} from "@/lib/app-events";
import { configSchema, defaultConfig, type ConfigForm } from "@/lib/config";
import { useI18n } from "@/lib/i18n";
import { invokeCommand } from "@/lib/commands";

interface DashboardSummary {
  recent: number;
  frequent: number;
  selected: number;
}

interface AppShellContextValue {
  config: ConfigForm;
  setConfig: Dispatch<SetStateAction<ConfigForm>>;
  updateDashboardSummary: (summary: DashboardSummary) => void;
}

type PrivacyState =
  | "Inactive"
  | "ActiveFull"
  | { ActivePartial: { recent: boolean; frequent: boolean } };

const AUTO_CLEAN_FINISHED_EVENT = "auto-clean-finished";

const AppShellContext = createContext<AppShellContextValue | null>(null);

export function AppShell({ dashboard }: { dashboard: ReactNode }) {
  const { t } = useI18n();
  const location = useLocation();
  const [config, setConfig] = useState<ConfigForm>(defaultConfig);
  const [configDrawerOpen, setConfigDrawerOpen] = useState(false);
  const [privacyActive, setPrivacyActive] = useState(false);
  const [summary, setSummary] = useState<DashboardSummary>({
    recent: 0,
    frequent: 0,
    selected: 0,
  });
  const onDashboard = location.pathname === "/";

  useEffect(() => {
    invokeCommand<ConfigForm>("get_config")
      .then((value) => setConfig(configSchema.parse(value)))
      .catch((error) =>
        toast.error(error instanceof Error ? error.message : String(error)),
      );
  }, []);

  useEffect(() => {
    const unlisten = listen<AutoCleanFinished>(
      AUTO_CLEAN_FINISHED_EVENT,
      ({ payload }) => {
        setConfig((current) => ({
          ...current,
          auto_clean_last_run: payload.completed_at,
        }));
        dispatchAppEvent(AUTO_CLEAN_UPDATED_EVENT, payload);
        dispatchAppEvent(REFRESH_DASHBOARD_EVENT, { fresh: false });
        dispatchAppEvent(REFRESH_HISTORY_EVENT);
      },
    );

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, []);

  useEffect(() => {
    const openConfigDrawer = () => {
      setConfigDrawerOpen(true);
      void invokeCommand<PrivacyState>("privacy_state")
        .then((state) => setPrivacyActive(state !== "Inactive"))
        .catch(() => setPrivacyActive(false));
    };

    window.addEventListener(OPEN_CONFIG_DRAWER_EVENT, openConfigDrawer);
    return () =>
      window.removeEventListener(OPEN_CONFIG_DRAWER_EVENT, openConfigDrawer);
  }, []);

  const updateDashboardSummary = useCallback(
    (next: DashboardSummary) => setSummary(next),
    [],
  );
  const context = useMemo(
    () => ({ config, setConfig, updateDashboardSummary }),
    [config, updateDashboardSummary],
  );
  const navigation = [
    { icon: Gauge, label: t("dashboard"), path: "/" },
    { icon: ShieldCheck, label: t("rules"), path: "/rules" },
    { icon: History, label: t("history"), path: "/history" },
    { icon: Settings, label: t("settings"), path: "/settings" },
  ];

  return (
    <AppShellContext.Provider value={context}>
      <SidebarProvider>
        <Sidebar collapsible="icon" variant="sidebar">
          <SidebarHeader>
            <div className="px-2 py-1">
              <div className="text-sm font-semibold">Scourgify</div>
              <div className="text-xs text-muted-foreground">
                {t("quickAccess")}
              </div>
            </div>
          </SidebarHeader>
          <SidebarSeparator />
          <SidebarContent>
            <SidebarGroup>
              <SidebarGroupLabel>{t("commandNavigation")}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {navigation.map(({ icon: Icon, label, path }) => (
                    <SidebarMenuItem key={path}>
                      <SidebarMenuButton
                        isActive={location.pathname === path}
                        render={<Link to={path} />}
                        tooltip={label}
                      >
                        <Icon />
                        <span>{label}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      render={<Link to="/about" />}
                      tooltip={t("about")}
                    >
                      <Info />
                      <span>{t("about")}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      onClick={() => dispatchAppEvent(OPEN_CONFIG_DRAWER_EVENT)}
                      tooltip={t("appearance")}
                      type="button"
                    >
                      <Paintbrush />
                      <span>{t("appearance")}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
            <SidebarGroup>
              <SidebarGroupLabel>{t("counts")}</SidebarGroupLabel>
              <SidebarGroupContent>
                <div className="grid gap-1 px-2 text-xs text-muted-foreground">
                  <span>
                    {t("recent")}: {summary.recent}
                  </span>
                  <span>
                    {t("frequent")}: {summary.frequent}
                  </span>
                  <span>
                    {t("selected")}: {summary.selected}
                  </span>
                </div>
              </SidebarGroupContent>
            </SidebarGroup>
          </SidebarContent>
        </Sidebar>
        <SidebarInset className="min-h-screen bg-background text-foreground">
          <div className={onDashboard ? "min-w-0" : "hidden"}>
            {dashboard}
          </div>
          {onDashboard ? null : <Outlet />}
        </SidebarInset>
        <ConfigDrawer
          config={config}
          onConfigSaved={setConfig}
          onOpenChange={setConfigDrawerOpen}
          open={configDrawerOpen}
          privacyActive={privacyActive}
        />
        <AppCommandPalette />
      </SidebarProvider>
    </AppShellContext.Provider>
  );
}

export function PageHeader({
  actions,
  subtitle,
  title,
}: {
  actions?: ReactNode;
  subtitle?: ReactNode;
  title: ReactNode;
}) {
  return (
    <header className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-b px-6 py-2">
      <div className="flex min-w-0 items-center gap-3">
        <SidebarTrigger />
        <div className="min-w-0">
          <h1 className="truncate text-base font-semibold">{title}</h1>
          {subtitle ? (
            <p className="truncate text-xs text-muted-foreground">{subtitle}</p>
          ) : null}
        </div>
      </div>
      {actions ? (
        <div className="flex flex-wrap items-center justify-end gap-2">
          {actions}
        </div>
      ) : null}
    </header>
  );
}

export function useAppShell() {
  const context = useContext(AppShellContext);
  if (!context) {
    throw new Error("useAppShell must be used within AppShell");
  }
  return context;
}
