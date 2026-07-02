import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import packageJson from "../../package.json";

const GITHUB_URL = "https://github.com/hellager/scourgify";
const LANGUAGE_CHANGED_EVENT = "language-changed";

type Language = "en-US" | "zh-CN" | "zh-TW" | "fr-FR" | "ru-RU";

interface LanguageChanged {
  language: Language;
}

const copy: Record<
  Language,
  {
    close: string;
    version: string;
    author: string;
    license: string;
    github: string;
  }
> = {
  "en-US": {
    close: "Close",
    version: "Version",
    author: "Author",
    license: "License",
    github: "GitHub",
  },
  "zh-CN": {
    close: "关闭",
    version: "版本",
    author: "作者",
    license: "许可",
    github: "GitHub",
  },
  "zh-TW": {
    close: "關閉",
    version: "版本",
    author: "作者",
    license: "授權",
    github: "GitHub",
  },
  "fr-FR": {
    close: "Fermer",
    version: "Version",
    author: "Auteur",
    license: "Licence",
    github: "GitHub",
  },
  "ru-RU": {
    close: "Закрыть",
    version: "Версия",
    author: "Автор",
    license: "Лицензия",
    github: "GitHub",
  },
};

export function AboutDialog() {
  const [language, setLanguage] = useState<Language>("en-US");
  const text = copy[language];

  useEffect(() => {
    invoke<string>("current_language").then((language) => {
      setLanguage(toLanguage(language));
    });

    const unlisten = listen<LanguageChanged>(LANGUAGE_CHANGED_EVENT, (event) => {
      setLanguage(toLanguage(event.payload.language));
    });

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, []);

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
          aria-label={text.close}
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
            <dt>{text.version}</dt>
            <dd>{packageJson.version}</dd>
          </div>
          <div>
            <dt>{text.author}</dt>
            <dd>Stein Gu</dd>
          </div>
          <div>
            <dt>{text.license}</dt>
            <dd>MIT</dd>
          </div>
        </dl>
        <button className="github-link" type="button" onClick={openGitHub}>
          {text.github}
        </button>
      </section>
    </main>
  );
}

function toLanguage(language: string): Language {
  return language in copy ? (language as Language) : "en-US";
}
