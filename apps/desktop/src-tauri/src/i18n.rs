use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn english() -> bool { ENGLISH.load(Ordering::Relaxed) }

pub fn text<'a>(japanese: &'a str, english_text: &'a str) -> &'a str {
    if english() { english_text } else { japanese }
}

#[tauri::command]
pub fn cmd_set_locale(locale: String) -> Result<(), String> {
    match locale.as_str() {
        "ja" => ENGLISH.store(false, Ordering::Relaxed),
        "en" => ENGLISH.store(true, Ordering::Relaxed),
        _ => return Err("Unsupported language".into()),
    }
    Ok(())
}

#[macro_export]
macro_rules! localized {
    ($ja:literal, $en:literal $(, $argument:expr)* $(,)?) => {
        if $crate::i18n::english() { format!($en $(, $argument)*) }
        else { format!($ja $(, $argument)*) }
    };
}
