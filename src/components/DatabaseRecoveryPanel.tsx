import { useState } from "react";
import { FolderOpen, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { StatusNotice } from "@/components/ui/status-notice";
import {
  dispatchAppEvent,
  REFRESH_DASHBOARD_EVENT,
} from "@/lib/app-events";
import { useI18n } from "@/lib/i18n";
import { invokeCommand } from "@/lib/commands";

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
      const next = await invokeCommand<DatabaseStatus>("retry_database");
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
      await invokeCommand("open_database_directory");
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  return (
    <StatusNotice
      actions={
        <div className="flex flex-wrap justify-end gap-2">
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
      }
      aria-live="polite"
      title={t("databaseUnavailable")}
      tone="error"
    >
      <p>{t("databaseUnavailableDescription")}</p>
      <dl className="mt-3 grid gap-2 text-xs text-muted-foreground">
        {status.path ? (
          <div className="grid gap-1">
            <dt className="font-medium text-foreground">{t("databasePath")}</dt>
            <dd className="data-code break-all">{status.path}</dd>
          </div>
        ) : null}
        {status.error ? (
          <div className="grid gap-1">
            <dt className="font-medium text-foreground">{t("databaseError")}</dt>
            <dd className="break-words">{status.error}</dd>
          </div>
        ) : null}
      </dl>
    </StatusNotice>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
