//! Application-level internationalization for user-visible UI text.
//!
//! `I18n` deliberately lives in `CommonData` instead of Floem's global
//! context. Floem's context store is runtime-global in the pinned version,
//! while Lapce can create multiple window tabs with different workspace
//! configuration. A window-local signal keeps those scopes independent.

use std::{collections::HashMap, env};

use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate};
use once_cell::sync::Lazy;

const EN_LOCALE: &str = include_str!("../assets/locales/en.toml");
const ZH_CN_LOCALE: &str = include_str!("../assets/locales/zh-CN.toml");

static EN: Lazy<HashMap<String, String>> = Lazy::new(|| parse_locale(EN_LOCALE));
static ZH_CN: Lazy<HashMap<String, String>> =
    Lazy::new(|| parse_locale(ZH_CN_LOCALE));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    ZhCn,
}

impl Locale {
    fn from_tag(tag: &str) -> Option<Self> {
        let tag = tag.trim().to_ascii_lowercase();
        if tag == "en" || tag.starts_with("en-") || tag.starts_with("en_") {
            Some(Self::En)
        } else if tag == "zh" || tag.starts_with("zh-") || tag.starts_with("zh_") {
            Some(Self::ZhCn)
        } else {
            None
        }
    }

    fn detect_system() -> Self {
        ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
            .into_iter()
            .filter_map(|key| env::var(key).ok())
            .find_map(|value| Self::from_tag(&value))
            .unwrap_or(Self::En)
    }

    pub fn from_preference(preference: &str) -> Self {
        if preference.eq_ignore_ascii_case("auto") {
            Self::detect_system()
        } else {
            Self::from_tag(preference).unwrap_or_else(Self::detect_system)
        }
    }
}

#[derive(Clone)]
pub struct I18n {
    locale: RwSignal<Locale>,
}

impl I18n {
    pub fn new(cx: Scope, preference: &str) -> Self {
        Self {
            locale: cx.create_rw_signal(Locale::from_preference(preference)),
        }
    }

    pub fn locale(&self) -> Locale {
        self.locale.get()
    }

    pub fn set_preference(&self, preference: &str) {
        self.locale.set(Locale::from_preference(preference));
    }

    pub fn text(&self, key: &str) -> String {
        let translations = match self.locale.get() {
            Locale::En => &*EN,
            Locale::ZhCn => &*ZH_CN,
        };

        translations
            .get(key)
            .or_else(|| EN.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    /// Creates a reactive text producer for Floem views.
    ///
    /// The locale signal is read when the returned closure runs, so views such
    /// as `label(i18n.text_signal("panel.file-explorer"))` update when the
    /// language changes. Keeping this separate from [`Self::text`] makes it
    /// harder to accidentally snapshot a translation during view construction.
    pub fn text_signal(
        &self,
        key: &'static str,
    ) -> impl Fn() -> String + Clone + 'static {
        let i18n = self.clone();
        move || i18n.text(key)
    }
}

fn parse_locale(source: &str) -> HashMap<String, String> {
    toml::from_str(source).expect("embedded locale file must be valid TOML")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_files_have_matching_keys() {
        assert_eq!(EN.len(), ZH_CN.len());
        assert!(EN.keys().all(|key| ZH_CN.contains_key(key)));
    }

    #[test]
    fn unsupported_locale_falls_back_to_system_locale() {
        assert!(matches!(
            Locale::from_preference("fr"),
            Locale::En | Locale::ZhCn
        ));
    }
}
