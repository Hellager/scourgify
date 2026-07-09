import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

interface NotificationConfig {
  notifications_enabled: boolean;
  notify_operation_complete: boolean;
  notify_partial_failure: boolean;
}

type NotificationKind = "operation_complete" | "partial_failure";

export async function requestNotificationPermission(): Promise<boolean> {
  if (await isPermissionGranted()) {
    return true;
  }

  return (await requestPermission()) === "granted";
}

export async function notifyOperationComplete(
  title: string,
  body: string,
): Promise<void> {
  await notifySystem({
    body,
    kind: "operation_complete",
    requireInactiveWindow: true,
    title,
  });
}

export async function notifyPartialFailure(
  title: string,
  body: string,
): Promise<void> {
  await notifySystem({ body, kind: "partial_failure", title });
}

async function notifySystem({
  body,
  kind,
  requireInactiveWindow = false,
  title,
}: {
  body: string;
  kind: NotificationKind;
  requireInactiveWindow?: boolean;
  title: string;
}) {
  try {
    const config = await invoke<NotificationConfig>("get_config");
    if (!config.notifications_enabled) {
      return;
    }
    if (kind === "operation_complete" && !config.notify_operation_complete) {
      return;
    }
    if (kind === "partial_failure" && !config.notify_partial_failure) {
      return;
    }
    if (requireInactiveWindow && !(await isCurrentWindowInactive())) {
      return;
    }
    if (!(await requestNotificationPermission())) {
      return;
    }

    sendNotification({ body, title });
  } catch {
    // Notification failures should not change the foreground Toast workflow.
  }
}

async function isCurrentWindowInactive() {
  const window = getCurrentWindow();
  const [visible, focused] = await Promise.all([
    window.isVisible(),
    window.isFocused(),
  ]);
  return !visible || !focused;
}
