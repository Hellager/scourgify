import type { CommandWarningPayload } from "@/lib/commands";

export const OPEN_CONFIG_DRAWER_EVENT = "scourgify:open-config-drawer";
export const REFRESH_DASHBOARD_EVENT = "scourgify:refresh-dashboard";
export const REFRESH_HISTORY_EVENT = "scourgify:refresh-history";
export const AUTO_CLEAN_UPDATED_EVENT = "scourgify:auto-clean-updated";
export const QUICK_ACCESS_CHANGED_EVENT = "quick-access-changed";
export const PRIVACY_STATE_CHANGED_EVENT = "privacy-state-changed";
export const REFRESH_RUNTIME_STATUS_EVENT = "scourgify:refresh-runtime-status";

export interface AutoCleanFinished {
  completed_at: string;
  total: number;
  succeeded: number;
  failed: number;
  warnings: number;
  section_errors: number;
  history_errors: number;
  post_process_warnings: CommandWarningPayload[];
  last_run_persisted: boolean;
}

export function dispatchAppEvent<T = undefined>(name: string, detail?: T) {
  window.dispatchEvent(new CustomEvent(name, { detail }));
}
