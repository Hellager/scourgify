import { getCurrentWindow } from "@tauri-apps/api/window";
import { Gauge, Minus, Square, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { commandErrorMessage, invokeCommand } from "@/lib/commands";

interface TitleBarProps {
  closeLabel: string;
  dashboardLabel: string;
  maximizeLabel: string;
  minimizeLabel: string;
}

export function TitleBar({
  closeLabel,
  dashboardLabel,
  maximizeLabel,
  minimizeLabel,
}: TitleBarProps) {
  const showDashboard = async () => {
    try {
      await invokeCommand("set_app_mode", { mode: "dashboard" });
    } catch (error) {
      toast.error(commandErrorMessage(error));
    }
  };

  return (
    <header
      className="group grid h-8 shrink-0 grid-cols-[1fr_auto_1fr] items-center border-b bg-secondary select-none"
      data-tauri-drag-region
    >
      <div data-tauri-drag-region />
      <h1
        className="px-3 text-center text-xs font-medium text-secondary-foreground"
        data-tauri-drag-region
      >
        Scourgify
      </h1>
      <div className="pointer-events-none flex h-full items-center justify-end opacity-0 transition-opacity duration-300 ease-in-out group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100">
        <Button
          aria-label={dashboardLabel}
          className="h-8 w-10 rounded-none border-0 bg-clip-border hover:bg-foreground/10"
          onClick={() => void showDashboard()}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Gauge className="size-3.5" />
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
