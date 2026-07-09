export const OPEN_CONFIG_DRAWER_EVENT = "scourgify:open-config-drawer";
export const REFRESH_DASHBOARD_EVENT = "scourgify:refresh-dashboard";

export function dispatchAppEvent(name: string) {
  window.dispatchEvent(new Event(name));
}
