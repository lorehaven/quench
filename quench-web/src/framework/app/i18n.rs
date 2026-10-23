use quench_core::{Context as _, Result};
use quench_serde::json;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use super::ftl;

const I18N_DIR: &str = "i18n";

/// The locales with a `.ftl` file in `./i18n`, sorted.
pub fn available_locales() -> Result<Vec<String>> {
    available_locales_in(Path::new(I18N_DIR))
}

/// The locales with a `.ftl` file in `dir`, sorted.
pub fn available_locales_in(dir: &Path) -> Result<Vec<String>> {
    let mut locales = Vec::new();

    if !dir.exists() {
        return Ok(Vec::new());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|s| s == "ftl")
            && let Some(locale) = path.file_stem().map(|s| s.to_string_lossy().to_string())
        {
            locales.push(locale);
        }
    }

    locales.sort();
    locales.dedup();
    Ok(locales)
}

pub fn validate_locales_exist(locales: &[String]) -> Result<()> {
    validate_locales_exist_in(Path::new(I18N_DIR), locales)
}

pub fn validate_locales_exist_in(dir: &Path, locales: &[String]) -> Result<()> {
    for locale in locales {
        let path = dir.join(format!("{locale}.ftl"));
        if !path.exists() {
            quench_core::bail!("missing locale file: {}", path.display());
        }
    }
    Ok(())
}

/// The translations as pretty JSON: `{"en-US": {"id": "text with {$var}"}}`.
/// Only the `supported_locales` are included when given.
pub fn parse_ftl_with_options(supported_locales: Option<&[String]>) -> Result<String> {
    parse_ftl_in(Path::new(I18N_DIR), supported_locales)
}

pub fn parse_ftl_in(dir: &Path, supported_locales: Option<&[String]>) -> Result<String> {
    if !dir.exists() {
        return Ok("{}".to_string());
    }

    let allowed: Option<HashSet<&str>> =
        supported_locales.map(|v| v.iter().map(String::as_str).collect());
    // Sorted maps keep the generated file stable from run to run.
    let mut all_locales: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|s| s == "ftl") {
            let locale = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if let Some(allowed) = &allowed
                && !allowed.contains(locale.as_str())
            {
                continue;
            }

            let ftl_string = std::fs::read_to_string(&path)?;
            let messages = ftl::parse(&ftl_string)
                .with_context(|| format!("Failed to parse FTL: {}", path.display()))?;
            let map = messages.into_iter().map(|m| (m.id, m.value)).collect();
            all_locales.insert(locale, map);
        }
    }

    Ok(json::to_string_pretty(&all_locales))
}

#[must_use]
pub fn generate_translations_js(supported_locales: &[String]) -> String {
    let translations = parse_ftl_with_options(Some(supported_locales)).unwrap_or_default();
    format!("window.qTranslations = {translations};")
}
