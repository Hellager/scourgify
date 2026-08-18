import * as React from "react";

import { cn } from "@/lib/utils";

const widthClasses = {
  settings: "[--page-frame-width:var(--content-width-settings)]",
  table: "[--page-frame-width:var(--content-width-table)]",
  wide: "[--page-frame-width:var(--content-width-wide)]",
};

function PageFrame({
  as: Component = "div",
  className,
  comfortable = false,
  reserveFabSpace = false,
  width = "table",
  ...props
}: React.ComponentProps<"div"> & {
  as?: "div" | "main";
  comfortable?: boolean;
  reserveFabSpace?: boolean;
  width?: keyof typeof widthClasses;
}) {
  return (
    <Component
      data-slot="page-frame"
      className={cn(
        "page-frame",
        widthClasses[width],
        comfortable &&
          "[--page-frame-gap:var(--content-gap-comfortable)] [--page-frame-gutter:var(--content-gutter-comfortable)]",
        reserveFabSpace && "[--page-frame-fab-space:4rem]",
        className,
      )}
      {...props}
    />
  );
}

export { PageFrame };
