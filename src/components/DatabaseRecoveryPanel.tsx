import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  dispatchAppEvent,
  REFRESH_DASHBOARD_EVENT,
} from "@/lib/app-events";
import { useI18n } from "@/lib/i18n";

export interface DatabaseStatus {
  available: boolean;
  path: string | null;
  schema_version: number | null;
  error: string | null;
}

export function DatabaseRecoveryPanel({
  onRecovered,
  onStatusChange,
  status,
}: {
  onRecovered?: () => Promise<void> | void;
  onStatusChange: (status: DatabaseStatus) => void;
  status: DatabaseStatus;
}) {
  const { t } = useI18n();
  const [retrying, setRetrying] = useState(false);

  const retry = async () => {
    setRetrying(true);
    try {
      const next = await invoke<DatabaseStatus>("retry_database");
      onStatusChange(next);
      if (next.available) {
        toast.success(t("databaseRecovered"));
        dispatchAppEvent(REFRESH_DASHBOARD_EVENT);
        await onRecovered?.();
      } else {
        toast.error(next.error ?? t("databaseUnavailable"));
      }
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setRetrying(false);
    }
  };

  const openDirectory = async () => {
    try {
      await invoke("open_database_directory");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  return (
    <section
      aria-live="polite"
      className="grid gap-3 border-l-2 border-destructive px-4 py-2"
    >
      <div>
        <h2 className="text-sm font-semibold">{t("databaseUnavailable")}</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          {t("databaseUnavailableDescription")}
        </p>
      </div>
      <dl className="grid gap-2 text-xs text-muted-foreground">
        {status.path ? (
          <div className="grid gap-1">
            <dt className="font-medium text-foreground">{t("databasePath")}</dt>
            <dd className="break-all font-mono">{status.path}</dd>
          </div>
        ) : null}
        {status.error ? (
          <div className="grid gap-1">
            <dt className="font-medium text-foreground">{t("databaseError")}</dt>
            <dd className="break-words">{status.error}</dd>
          </div>
        ) : null}
      </dl>
      <div className="flex flex-wrap gap-2">
        <Button
          disabled={retrying}
          onClick={() => void retry()}
          size="sm"
          type="button"
          variant="outline"
        >
          <RefreshCw className={retrying ? "animate-spin" : ""} />
          {t("retryDatabase")}
        </Button>
        <Button
          disabled={!status.path}
          onClick={() => void openDirectory()}
          size="sm"
          type="button"
          variant="outline"
        >
          <FolderOpen />
          {t("openDatabaseDirectory")}
        </Button>
      </div>
    </section>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
