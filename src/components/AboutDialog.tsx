import { openUrl } from "@tauri-apps/plugin-opener";
import { X } from "lucide-react";
import { useTheme } from "next-themes";
import packageJson from "../../package.json";
import darkIcon from "../../src-tauri/icons/dark/incognito-svgrepo-com.svg";
import lightIcon from "../../src-tauri/icons/light/incognito-svgrepo-com.svg";
import { Button } from "@/components/ui/button";
import { invokeCommand } from "@/lib/commands";
import { useI18n } from "@/lib/i18n";

const GITHUB_URL = "https://github.com/hellager/scourgify";

export function AboutDialog() {
  const { t } = useI18n();
  const { resolvedTheme } = useTheme();
  const appIcon = resolvedTheme === "dark" ? darkIcon : lightIcon;

  const openGitHub = async () => {
    await openUrl(GITHUB_URL);
  };

  const close = async () => {
    await invokeCommand("hide_about");
  };

  return (
    <main className="relative min-h-svh overflow-hidden bg-background text-foreground">
      <Button
        aria-label={t("close")}
        className="absolute right-1 top-1 text-muted-foreground hover:text-foreground"
        onClick={() => void close()}
        size="icon-lg"
        title={t("close")}
        type="button"
        variant="ghost"
      >
        <X />
      </Button>

      <section
        className="flex min-h-svh w-full flex-col items-center pt-[26px]"
        aria-labelledby="about-title"
      >
        <img alt="" className="size-12" src={appIcon} />
        <h1 className="mt-3 text-xl font-semibold" id="about-title">
          Scourgify
        </h1>

        <div className="mt-4 flex flex-col items-stretch gap-5">
          <p className="flex items-center justify-center gap-2 whitespace-nowrap text-sm font-medium text-muted-foreground">
            <span className="tabular-nums">v{packageJson.version}</span>
            <span aria-hidden="true">·</span>
            <span>Stein Gu</span>
            <span aria-hidden="true">·</span>
            <span>MIT</span>
          </p>

          <Button
            className="w-full"
            onClick={() => void openGitHub()}
            size="sm"
            type="button"
            variant="outline"
          >
            {t("github")}
          </Button>
        </div>
      </section>
    </main>
  );
}
