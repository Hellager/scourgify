use serde::Serialize;

pub(crate) const LANGUAGES: &[(&str, &str)] = &[
    ("en-US", "English"),
    ("zh-CN", "简体中文"),
    ("zh-TW", "繁體中文"),
    ("fr-FR", "Français"),
    ("ru-RU", "Русский"),
];

#[derive(Clone, Copy, Serialize)]
pub(crate) struct LanguageChanged {
    pub(crate) language: &'static str,
}

pub(crate) struct TrayText {
    pub(crate) open_dashboard: &'static str,
    pub(crate) auto_start: &'static str,
    pub(crate) privacy_mode: &'static str,
    pub(crate) privacy_mode_partial: &'static str,
    pub(crate) mode: &'static str,
    pub(crate) mode_dashboard: &'static str,
    pub(crate) mode_grid: &'static str,
    pub(crate) mode_tray: &'static str,
    pub(crate) language: &'static str,
    pub(crate) quit: &'static str,
}

pub(crate) fn tray_text(language: &str) -> TrayText {
    match language {
        "zh-CN" => TrayText {
            open_dashboard: "打开主界面",
            auto_start: "开机自启",
            privacy_mode: "隐私模式",
            privacy_mode_partial: "隐私模式（部分）",
            mode: "模式",
            mode_dashboard: "标准",
            mode_grid: "精简",
            mode_tray: "极简",
            language: "语言",
            quit: "退出",
        },
        "zh-TW" => TrayText {
            open_dashboard: "開啟主介面",
            auto_start: "開機自啟",
            privacy_mode: "隱私模式",
            privacy_mode_partial: "隱私模式（部分）",
            mode: "模式",
            mode_dashboard: "標準",
            mode_grid: "精簡",
            mode_tray: "極簡",
            language: "語言",
            quit: "退出",
        },
        "fr-FR" => TrayText {
            open_dashboard: "Ouvrir le tableau de bord",
            auto_start: "Lancer au démarrage",
            privacy_mode: "Mode privé",
            privacy_mode_partial: "Mode privé (partiel)",
            mode: "Mode",
            mode_dashboard: "Standard",
            mode_grid: "Compact",
            mode_tray: "Minimal",
            language: "Langue",
            quit: "Quitter",
        },
        "ru-RU" => TrayText {
            open_dashboard: "Открыть панель",
            auto_start: "Автозапуск",
            privacy_mode: "Приватный режим",
            privacy_mode_partial: "Приватный режим (частично)",
            mode: "Режим",
            mode_dashboard: "Стандартный",
            mode_grid: "Компактный",
            mode_tray: "Минимальный",
            language: "Язык",
            quit: "Выход",
        },
        _ => TrayText {
            open_dashboard: "Open Dashboard",
            auto_start: "Auto Start",
            privacy_mode: "Privacy Mode",
            privacy_mode_partial: "Privacy Mode (Partial)",
            mode: "Mode",
            mode_dashboard: "Standard",
            mode_grid: "Compact",
            mode_tray: "Minimal",
            language: "Language",
            quit: "Quit",
        },
    }
}

pub(crate) fn language_event(language: &str) -> LanguageChanged {
    LanguageChanged {
        language: canonical(language),
    }
}

fn canonical(language: &str) -> &'static str {
    LANGUAGES
        .iter()
        .find_map(|(code, _)| (*code == language).then_some(*code))
        .unwrap_or("en-US")
}
