import { openUrl } from "@tauri-apps/plugin-opener";
import packageJson from "../../package.json";
import { invokeCommand } from "@/lib/commands";
import { useI18n } from "@/lib/i18n";

const GITHUB_URL = "https://github.com/hellager/scourgify";

export function AboutDialog() {
  const { t } = useI18n();

  const openGitHub = async () => {
    await openUrl(GITHUB_URL);
  };

  const close = async () => {
    await invokeCommand("hide_about");
  };

  return (
    <main className="about-shell">
      <section className="about-panel" aria-labelledby="about-title">
        <button
          aria-label={t("close")}
          className="close-button"
          onClick={close}
          type="button"
        >
          x
        </button>
        <div className="app-mark" aria-hidden="true">
          S
        </div>
        <h1 id="about-title">Scourgify</h1>
        <dl className="about-details">
          <div>
            <dt>{t("version")}</dt>
            <dd>{packageJson.version}</dd>
          </div>
          <div>
            <dt>{t("author")}</dt>
            <dd>Stein Gu</dd>
          </div>
          <div>
            <dt>{t("license")}</dt>
            <dd>MIT</dd>
          </div>
        </dl>
        <button className="github-link" onClick={openGitHub} type="button">
          {t("github")}
        </button>
      </section>
    </main>
  );
}
