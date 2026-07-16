use serde::Serialize;

pub const LANGUAGES: &[(&str, &str)] = &[
    ("en-US", "English"),
    ("zh-CN", "简体中文"),
    ("zh-TW", "繁體中文"),
    ("fr-FR", "Français"),
    ("ru-RU", "Русский"),
];

#[derive(Clone, Copy, Serialize)]
pub struct LanguageChanged {
    pub language: &'static str,
}

pub struct TrayText {
    pub open_dashboard: &'static str,
    pub auto_start: &'static str,
    pub privacy_mode: &'static str,
    pub privacy_mode_partial: &'static str,
    pub mode: &'static str,
    pub mode_dashboard: &'static str,
    pub mode_grid: &'static str,
    pub mode_tray: &'static str,
    pub language: &'static str,
    pub quit: &'static str,
}

pub fn tray_text(language: &str) -> TrayText {
    match language {
        "zh-CN" => TrayText {
            open_dashboard: "打开主界面",
            auto_start: "开机自启",
            privacy_mode: "隐私模式",
            privacy_mode_partial: "隐私模式（部分）",
            mode: "模式",
            mode_dashboard: "Dashboard",
            mode_grid: "九宫格",
            mode_tray: "托盘",
            language: "语言",
            quit: "退出",
        },
        "zh-TW" => TrayText {
            open_dashboard: "開啟主介面",
            auto_start: "開機自啟",
            privacy_mode: "隱私模式",
            privacy_mode_partial: "隱私模式（部分）",
            mode: "模式",
            mode_dashboard: "Dashboard",
            mode_grid: "九宮格",
            mode_tray: "系統匣",
            language: "語言",
            quit: "退出",
        },
        "fr-FR" => TrayText {
            open_dashboard: "Ouvrir le tableau de bord",
            auto_start: "Lancer au démarrage",
            privacy_mode: "Mode privé",
            privacy_mode_partial: "Mode privé (partiel)",
            mode: "Mode",
            mode_dashboard: "Dashboard",
            mode_grid: "Grille",
            mode_tray: "Zone de notification",
            language: "Langue",
            quit: "Quitter",
        },
        "ru-RU" => TrayText {
            open_dashboard: "Открыть панель",
            auto_start: "Автозапуск",
            privacy_mode: "Приватный режим",
            privacy_mode_partial: "Приватный режим (частично)",
            mode: "Режим",
            mode_dashboard: "Dashboard",
            mode_grid: "Сетка",
            mode_tray: "Трей",
            language: "Язык",
            quit: "Выход",
        },
        _ => TrayText {
            open_dashboard: "Open Dashboard",
            auto_start: "Auto Start",
            privacy_mode: "Privacy Mode",
            privacy_mode_partial: "Privacy Mode (Partial)",
            mode: "Mode",
            mode_dashboard: "Dashboard",
            mode_grid: "Grid",
            mode_tray: "Tray",
            language: "Language",
            quit: "Quit",
        },
    }
}

pub fn language_event(language: &str) -> LanguageChanged {
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
