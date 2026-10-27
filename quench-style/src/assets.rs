use crate::i18n::{available_locales, generate_translations_js};
use crate::theme::{Theme, theme_shared};
use std::path::Path;

/// The favicon every generated asset set contains.
pub const FAVICON: &[u8] = include_bytes!("../favicon.png");

/// Where the generated assets go, relative to the working directory.
const ASSETS_DIR: &str = "dist/assets";

pub fn create_asset_files(default_theme: Theme, resources_prefix: &str) {
    let supported_themes = Theme::iter().collect::<Vec<_>>();
    let supported_locales = available_locales().unwrap_or_default();
    create_asset_files_with_options(
        default_theme,
        &supported_themes,
        &supported_locales,
        resources_prefix,
    );
}

pub fn create_asset_files_with_options(
    _default_theme: Theme,
    supported_themes: &[Theme],
    supported_locales: &[String],
    _resources_prefix: &str,
) {
    write_assets(Path::new(ASSETS_DIR), supported_themes, supported_locales);
}

/// Writes `css/style.css`, `css/themes/<theme>.css`, `js/translations.js` and `favicon.png`
/// under `root`. Failures are reported on stderr and do not stop the remaining files.
pub fn write_assets(root: &Path, supported_themes: &[Theme], supported_locales: &[String]) {
    let themes_dir = root.join("css/themes");
    if let Err(err) = std::fs::create_dir_all(&themes_dir) {
        eprintln!("ERROR: failed to create css themes directory: {err}");
    }

    let js_dir = root.join("js");
    if let Err(err) = std::fs::create_dir_all(&js_dir) {
        eprintln!("ERROR: failed to create js directory: {err}");
    }

    if let Err(err) = std::fs::write(root.join("css/style.css"), theme_shared()) {
        eprintln!("ERROR: failed to write style.css: {err}");
    }

    if let Err(err) = std::fs::write(root.join("favicon.png"), FAVICON) {
        eprintln!("ERROR: failed to write favicon.png: {err}");
    }

    for theme in supported_themes {
        let path = themes_dir.join(format!("{theme}.css"));

        if let Err(err) = std::fs::write(&path, Theme::theme(*theme)) {
            eprintln!(
                "ERROR: failed to write theme file {}: {err}",
                path.display()
            );
        }
    }

    if let Err(err) = std::fs::write(
        js_dir.join("translations.js"),
        generate_translations_js(supported_locales),
    ) {
        eprintln!("ERROR: failed to write translations.js: {err}");
    }
}
