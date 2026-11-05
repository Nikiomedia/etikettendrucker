//! Sprache der Oberfläche: Deutsch oder Englisch.
//!
//! Texte stehen direkt im Code als Paar `tr("Deutsch", "English")`. So bleiben beide
//! Fassungen nebeneinander und keine geht beim Ändern verloren.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Lang {
    De,
    En,
}

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn set(lang: Lang) {
    ENGLISH.store(lang == Lang::En, Ordering::Relaxed);
}

pub fn is_en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// Sprache von Windows: Deutsch, wenn die Oberfläche deutsch ist, sonst Englisch.
pub fn system() -> Lang {
    match sys_locale::get_locale() {
        Some(l) if l.to_lowercase().starts_with("de") => Lang::De,
        Some(_) => Lang::En,
        None => Lang::De,
    }
}

/// Text in der eingestellten Sprache.
pub fn tr(de: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        de
    }
}

/// Wie `format!`, nur mit deutscher und englischer Vorlage.
#[macro_export]
macro_rules! trf {
    ($de:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() { format!($en $(, $arg)*) } else { format!($de $(, $arg)*) }
    };
}

/// Dezimaltrennzeichen: Komma im Deutschen, Punkt im Englischen.
pub fn decimal(s: String) -> String {
    if is_en() {
        s
    } else {
        s.replace('.', ",")
    }
}
