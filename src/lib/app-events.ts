export const OPEN_CONFIG_DRAWER_EVENT = "scourgify:open-config-drawer";
export const REFRESH_DASHBOARD_EVENT = "scourgify:refresh-dashboard";
export const REFRESH_HISTORY_EVENT = "scourgify:refresh-history";
export const AUTO_CLEAN_UPDATED_EVENT = "scourgify:auto-clean-updated";
export const QUICK_ACCESS_CHANGED_EVENT = "quick-access-changed";

export interface AutoCleanFinished {
  completed_at: string;
  total: number;
  succeeded: number;
  failed: number;
  warnings: number;
  section_errors: number;
  history_errors: number;
}

export function dispatchAppEvent<T = undefined>(name: string, detail?: T) {
  window.dispatchEvent(new CustomEvent(name, { detail }));
}
