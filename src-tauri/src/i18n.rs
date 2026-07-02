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
    pub auto_start: &'static str,
    pub privacy_mode: &'static str,
    pub privacy_mode_partial: &'static str,
    pub language: &'static str,
    pub about: &'static str,
    pub quit: &'static str,
}

pub fn tray_text(language: &str) -> TrayText {
    match language {
        "zh-CN" => TrayText {
            auto_start: "开机自启",
            privacy_mode: "隐私模式",
            privacy_mode_partial: "隐私模式（部分）",
            language: "语言",
            about: "关于",
            quit: "退出",
        },
        "zh-TW" => TrayText {
            auto_start: "開機自啟",
            privacy_mode: "隱私模式",
            privacy_mode_partial: "隱私模式（部分）",
            language: "語言",
            about: "關於",
            quit: "退出",
        },
        "fr-FR" => TrayText {
            auto_start: "Lancer au démarrage",
            privacy_mode: "Mode privé",
            privacy_mode_partial: "Mode privé (partiel)",
            language: "Langue",
            about: "À propos",
            quit: "Quitter",
        },
        "ru-RU" => TrayText {
            auto_start: "Автозапуск",
            privacy_mode: "Приватный режим",
            privacy_mode_partial: "Приватный режим (частично)",
            language: "Язык",
            about: "О программе",
            quit: "Выход",
        },
        _ => TrayText {
            auto_start: "Auto Start",
            privacy_mode: "Privacy Mode",
            privacy_mode_partial: "Privacy Mode (Partial)",
            language: "Language",
            about: "About",
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
