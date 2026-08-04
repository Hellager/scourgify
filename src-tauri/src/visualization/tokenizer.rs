use std::sync::OnceLock;

use jieba_rs::Jieba;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub key: String,
    pub display: String,
}

static JIEBA: OnceLock<Jieba> = OnceLock::new();

const STOP_WORDS: &[&str] = &[
    "appdata",
    "backups",
    "bureau",
    "cache",
    "desktop",
    "documents",
    "downloads",
    "images",
    "music",
    "oneDrive",
    "programdata",
    "program files",
    "pictures",
    "temp",
    "temporary",
    "users",
    "videos",
    "windows",
    "архив",
    "видео",
    "данные",
    "документы",
    "загрузки",
    "изображения",
    "кэш",
    "музыка",
    "пользователи",
    "резервные копии",
    "рабочий стол",
    "使用者",
    "下載",
    "備份",
    "圖片",
    "影片",
    "快取",
    "桌面",
    "檔案",
    "資料",
    "暫存",
    "文档",
    "下载",
    "备份",
    "图片",
    "临时文件",
    "数据",
    "桌面",
    "文档",
    "视频",
    "缓存",
    "用户",
];

pub(crate) fn tokenize_path(path: &str, recent_file: bool) -> Vec<Token> {
    let components = path
        .split(['\\', '/'])
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    let Some(last) = components.last().copied() else {
        return Vec::new();
    };

    let last = if recent_file {
        strip_extension(last)
    } else {
        last
    };
    let parent = components
        .iter()
        .rev()
        .skip(1)
        .enumerate()
        .find(|(reverse_index, component)| {
            let index = components.len().saturating_sub(*reverse_index + 2);
            !is_path_noise(component)
                && !is_stop_word(&component.to_lowercase())
                && !is_user_component(&components, index)
        })
        .map(|(_, component)| *component);

    let mut tokens = tokenize_text(last);
    if let Some(parent) = parent {
        tokens.extend(tokenize_text(parent));
    }
    tokens
}

pub(crate) fn tokenize_text(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut run = String::new();
    let mut han_run = false;

    for character in text.chars() {
        let character_is_han = is_han(character);
        if !run.is_empty() && character_is_han != han_run {
            extend_run(&mut tokens, &run, han_run);
            run.clear();
        }
        han_run = character_is_han;
        run.push(character);
    }
    if !run.is_empty() {
        extend_run(&mut tokens, &run, han_run);
    }
    tokens
}

pub(crate) fn tokenize_name(name: &str) -> Vec<Token> {
    tokenize_text(strip_extension(name))
}

fn extend_run(tokens: &mut Vec<Token>, text: &str, han: bool) {
    if han {
        let jieba = JIEBA.get_or_init(Jieba::new);
        for token in jieba.cut(text, false) {
            push_candidate(tokens, token.word);
        }
        return;
    }

    for word in UnicodeSegmentation::unicode_words(text) {
        for part in split_identifier(word) {
            push_candidate(tokens, part);
        }
    }
}

fn split_identifier(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let characters = value.char_indices().collect::<Vec<_>>();

    for index in 0..characters.len() {
        let (offset, character) = characters[index];
        if character == '_' || character == '-' {
            if start < offset {
                parts.push(&value[start..offset]);
            }
            start = offset + character.len_utf8();
            continue;
        }

        let Some((next_offset, next_character)) = characters.get(index + 1).copied() else {
            continue;
        };
        if character.is_lowercase() && next_character.is_uppercase() {
            if start < next_offset {
                parts.push(&value[start..next_offset]);
            }
            start = next_offset;
        }
    }

    if start < value.len() {
        parts.push(&value[start..]);
    }
    parts
}

fn push_candidate(tokens: &mut Vec<Token>, candidate: &str) {
    let display = candidate
        .trim_matches(|character: char| !character.is_alphanumeric() && !is_han(character));
    if !is_valid_candidate(display) {
        return;
    }

    let key = display.to_lowercase();
    if is_stop_word(&key) {
        return;
    }
    tokens.push(Token {
        key,
        display: display.to_string(),
    });
}

fn is_valid_candidate(value: &str) -> bool {
    if value.is_empty() || value.chars().all(|character| character.is_numeric()) {
        return false;
    }
    if value.chars().count() == 1
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    value
        .chars()
        .any(|character| character.is_alphabetic() || is_han(character))
}

fn is_stop_word(value: &str) -> bool {
    STOP_WORDS
        .iter()
        .any(|stop_word| value == stop_word.to_lowercase())
}

fn is_path_noise(value: &str) -> bool {
    value.len() <= 3
        || value.chars().all(|character| character.is_numeric())
        || value.ends_with(':')
}

fn is_user_component(components: &[&str], index: usize) -> bool {
    index > 0
        && matches!(
            components[index - 1].to_lowercase().as_str(),
            "users" | "使用者" | "用户" | "пользователи"
        )
}

fn strip_extension(value: &str) -> &str {
    let Some((stem, extension)) = value.rsplit_once('.') else {
        return value;
    };
    if stem.is_empty() || extension.is_empty() || stem.ends_with('.') {
        value
    } else {
        stem
    }
}

fn is_han(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2FA1F
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(tokens: Vec<Token>) -> Vec<String> {
        tokens.into_iter().map(|token| token.key).collect()
    }

    #[test]
    fn tokenizes_chinese_with_jieba() {
        let keys = keys(tokenize_text("项目计划报告"));

        assert!(keys.contains(&"项目".to_string()));
        assert!(keys.contains(&"计划".to_string()));
    }

    #[test]
    fn tokenizes_mixed_scripts_and_identifiers() {
        let keys = keys(tokenize_text("Scourgify项目-v2_report"));

        assert!(keys.contains(&"scourgify".to_string()));
        assert!(keys.contains(&"项目".to_string()));
        assert!(keys.contains(&"v2".to_string()));
        assert!(keys.contains(&"report".to_string()));
    }

    #[test]
    fn tokenizes_unicode_words() {
        let french = tokenize_text("présentation-projet");
        let russian = tokenize_text("отчет_проекта");

        assert!(french.iter().any(|token| token.key == "présentation"));
        assert!(russian.iter().any(|token| token.key == "отчет"));
        assert!(russian.iter().any(|token| token.key == "проекта"));
    }

    #[test]
    fn tokenizes_han_french_and_russian_segments_in_one_value() {
        assert_eq!(
            keys(tokenize_text("发票Résumé_Анализ")),
            ["发票", "résumé", "анализ"]
        );
    }

    #[test]
    fn removes_extensions_noise_and_path_stop_words() {
        let keys = keys(tokenize_path(
            r"C:\Users\alice\Documents\project_report.docx",
            true,
        ));

        assert!(keys.contains(&"project".to_string()));
        assert!(keys.contains(&"report".to_string()));
        assert!(!keys.contains(&"docx".to_string()));
        assert!(!keys.contains(&"users".to_string()));
        assert!(!keys.contains(&"documents".to_string()));
    }

    #[test]
    fn tokenizes_metadata_names_without_extensions() {
        assert_eq!(
            keys(tokenize_name("QuarterlyReport.docx")),
            ["quarterly", "report"]
        );
    }

    #[test]
    fn preserves_most_useful_folder_parent() {
        let keys = keys(tokenize_path(r"C:\Users\alice\Projects\Scourgify", false));

        assert!(keys.contains(&"scourgify".to_string()));
        assert!(keys.contains(&"projects".to_string()));
        assert!(!keys.contains(&"alice".to_string()));
    }

    #[test]
    fn filters_user_numeric_single_letter_extension_and_stop_word_components() {
        assert_eq!(
            keys(tokenize_path(
                r"C:\Users\alice\Documents\2026\a\Résumé-final_v2.PDF",
                true,
            )),
            ["résumé", "final", "v2"]
        );
    }
}
