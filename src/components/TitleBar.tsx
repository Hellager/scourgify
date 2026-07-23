import { getCurrentWindow } from "@tauri-apps/api/window";
import { Gauge, LayoutGrid, Minus, Square, X } from "lucide-react";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { commandErrorMessage, invokeCommand } from "@/lib/commands";
import { cn } from "@/lib/utils";
import darkIcon from "../../src-tauri/icons/dark/incognito-svgrepo-com.svg";
import lightIcon from "../../src-tauri/icons/light/incognito-svgrepo-com.svg";

interface TitleBarProps {
  closeLabel: string;
  maximizeLabel: string;
  minimizeLabel: string;
  mode: "dashboard" | "grid";
  switchModeLabel: string;
  title: string;
}

export function TitleBar({
  closeLabel,
  maximizeLabel,
  minimizeLabel,
  mode,
  switchModeLabel,
  title,
}: TitleBarProps) {
  const { resolvedTheme } = useTheme();
  const dashboard = mode === "dashboard";
  const titleBarIcon = resolvedTheme === "dark" ? darkIcon : lightIcon;
  const switchMode = async () => {
    try {
      await invokeCommand("set_app_mode", {
        mode: dashboard ? "grid" : "dashboard",
      });
    } catch (error) {
      toast.error(commandErrorMessage(error));
    }
  };

  return (
    <header
      className={cn(
        "group grid h-8 shrink-0 items-center border-b bg-secondary select-none",
        dashboard ? "grid-cols-[1fr_auto]" : "grid-cols-[1fr_auto_1fr]",
      )}
      data-tauri-drag-region
    >
      {dashboard ? (
        <div
          className="flex min-w-0 items-center gap-2 px-3"
          data-tauri-drag-region
        >
          <img
            alt=""
            className="pointer-events-none size-4 shrink-0"
            src={titleBarIcon}
          />
          <h1
            className="truncate text-left text-xs font-medium text-secondary-foreground"
            data-tauri-drag-region
          >
            {title}
          </h1>
        </div>
      ) : (
        <>
          <div data-tauri-drag-region />
          <h1
            className="px-3 text-center text-xs font-medium text-secondary-foreground"
            data-tauri-drag-region
          >
            {title}
          </h1>
        </>
      )}
      <div
        className={cn(
          "flex h-full items-center justify-end",
          !dashboard &&
            "pointer-events-none opacity-0 transition-opacity duration-300 ease-in-out group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100",
        )}
        data-tauri-drag-region
      >
        <Button
          aria-label={switchModeLabel}
          className="h-8 w-10 rounded-none border-0 bg-clip-border hover:bg-foreground/10"
          onClick={() => void switchMode()}
          size="icon-sm"
          title={switchModeLabel}
          type="button"
          variant="ghost"
        >
          {dashboard ? (
            <LayoutGrid className="size-3.5" />
          ) : (
            <Gauge className="size-3.5" />
          )}
        </Button>
        <Separator className="mx-1 my-2" orientation="vertical" />
        <Button
          aria-label={minimizeLabel}
          className="h-8 w-10 rounded-none border-0 bg-clip-border hover:bg-foreground/10"
          onClick={() => void getCurrentWindow().minimize()}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Minus className="size-3.5" />
        </Button>
        <Button
          aria-label={maximizeLabel}
          className="h-8 w-10 rounded-none border-0 bg-clip-border hover:bg-foreground/10"
          onClick={() => void getCurrentWindow().toggleMaximize()}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Square className="size-3" />
        </Button>
        <Button
          aria-label={closeLabel}
          className="h-8 w-10 rounded-none border-0 bg-clip-border hover:bg-destructive hover:text-white"
          onClick={() => void getCurrentWindow().close()}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <X className="size-3.5" />
        </Button>
      </div>
    </header>
  );
}
