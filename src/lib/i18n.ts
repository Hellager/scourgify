import { createElement, type ReactNode, useEffect, useMemo } from "react";
import { listen } from "@tauri-apps/api/event";
import i18n from "i18next";
import {
  I18nextProvider,
  initReactI18next,
  useTranslation,
} from "react-i18next";
import { invokeCommand, setCommandErrorLanguage } from "@/lib/commands";
import enUS from "./i18n/en-US.json";
import frFR from "./i18n/fr-FR.json";
import ruRU from "./i18n/ru-RU.json";
import zhCN from "./i18n/zh-CN.json";
import zhTW from "./i18n/zh-TW.json";

export type Language = "en-US" | "zh-CN" | "zh-TW" | "fr-FR" | "ru-RU";

type Dictionary = typeof enUS;
export type I18nKey = keyof Dictionary;

const LANGUAGE_CHANGED_EVENT = "language-changed";

interface LanguageChanged {
  language: Language;
}

const resources = {
  "en-US": { translation: enUS },
  "zh-CN": { translation: zhCN },
  "zh-TW": { translation: zhTW },
  "fr-FR": { translation: frFR },
  "ru-RU": { translation: ruRU },
};

void i18n.use(initReactI18next).init({
  resources,
  lng: "en-US",
  fallbackLng: "en-US",
  interpolation: {
    escapeValue: false,
    prefix: "{",
    suffix: "}",
  },
  initImmediate: false,
});

export function I18nProvider({ children }: { children: ReactNode }) {
  useEffect(() => {
    let active = true;

    const applyLanguage = (value: string) => {
      const language = toLanguage(value);
      setCommandErrorLanguage(language);
      void i18n.changeLanguage(language);
    };

    invokeCommand<string>("current_language")
      .then((value) => {
        if (active) {
          applyLanguage(value);
        }
      })
      .catch(() => {
        if (active) {
          applyLanguage("en-US");
        }
      });

    const unlisten = Promise.resolve()
      .then(() =>
        listen<LanguageChanged>(LANGUAGE_CHANGED_EVENT, (event) => {
          if (active) {
            applyLanguage(event.payload.language);
          }
        }),
      )
      .catch(() => undefined);

    return () => {
      active = false;
      void unlisten.then((cleanup) => cleanup?.());
    };
  }, []);

  return createElement(I18nextProvider, { i18n }, children);
}

export function useI18n() {
  const { i18n: instance, t } = useTranslation();
  const language = toLanguage(instance.resolvedLanguage ?? instance.language);
  const translate = useMemo(
    () => (key: I18nKey, values?: Record<string, string | number>) =>
      t(key, values),
    [t],
  );

  return useMemo(
    () => ({ language, t: translate }),
    [language, translate],
  );
}

export function toLanguage(language: string): Language {
  return language in resources ? (language as Language) : "en-US";
}
