import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import packageJson from "../../package.json";

const GITHUB_URL = "https://github.com/hellager/scourgify";

export function AboutDialog() {
  const openGitHub = async () => {
    await openUrl(GITHUB_URL);
  };

  const close = async () => {
    await invoke("hide_about");
  };

  return (
    <main className="about-shell">
      <section className="about-panel" aria-labelledby="about-title">
        <button
          className="close-button"
          type="button"
          aria-label="Close"
          onClick={close}
        >
          x
        </button>
        <div className="app-mark" aria-hidden="true">
          S
        </div>
        <h1 id="about-title">Scourgify</h1>
        <dl className="about-details">
          <div>
            <dt>Version</dt>
            <dd>{packageJson.version}</dd>
          </div>
          <div>
            <dt>Author</dt>
            <dd>Stein Gu</dd>
          </div>
          <div>
            <dt>License</dt>
            <dd>MIT</dd>
          </div>
        </dl>
        <button className="github-link" type="button" onClick={openGitHub}>
          GitHub
        </button>
      </section>
    </main>
  );
}
