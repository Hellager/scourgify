import * as React from "react";

import { cn } from "@/lib/utils";

const toneClasses = {
  error:
    "border-l-status-error bg-status-error-muted/45 text-status-error-foreground",
  neutral: "border-l-border bg-surface-subtle text-muted-foreground",
  running:
    "border-l-status-running bg-status-running-muted/45 text-status-running-foreground",
  success:
    "border-l-status-success bg-status-success-muted/45 text-status-success-foreground",
  warning:
    "border-l-status-warning bg-status-warning-muted/45 text-status-warning-foreground",
};

function StatusNotice({
  actions,
  children,
  className,
  title,
  tone = "neutral",
  ...props
}: React.ComponentProps<"section"> & {
  actions?: React.ReactNode;
  title?: React.ReactNode;
  tone?: keyof typeof toneClasses;
}) {
  return (
    <section
      data-slot="status-notice"
      className={cn(
        "surface-panel flex min-w-0 flex-col items-start justify-between gap-3 border-l-2 px-3 py-2.5 text-sm sm:flex-row sm:gap-4",
        toneClasses[tone],
        className,
      )}
      {...props}
    >
      <div className="min-w-0 flex-1">
        {title ? (
          <h2 className="font-semibold text-foreground">{title}</h2>
        ) : null}
        <div className={cn("min-w-0 break-words", title && "mt-1")}>
          {children}
        </div>
      </div>
      {actions ? (
        <div className="w-full shrink-0 sm:w-auto">{actions}</div>
      ) : null}
    </section>
  );
}

export { StatusNotice };
