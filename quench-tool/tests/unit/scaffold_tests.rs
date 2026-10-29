use super::support::*;
use quench_tool::cli::Template;
use quench_tool::{scaffold, templates, versions};

fn files(template: Template, path: Option<&str>) -> Vec<(String, String)> {
    templates::render("my-app", template, path).unwrap()
}

fn get<'a>(files: &'a [(String, String)], path: &str) -> &'a str {
    files
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, t)| t.as_str())
        .unwrap_or_else(|| panic!("no {path}"))
}

#[quench_test::test]
fn a_fullstack_project_has_all_four_crates() {
    let f = files(Template::Fullstack, Some("/q"));
    let paths: Vec<&str> = f.iter().map(|(p, _)| p.as_str()).collect();
    for want in [
        ".quench.toml",
        "Cargo.toml",
        ".gitignore",
        "README.md",
        "core/Cargo.toml",
        "core/src/lib.rs",
        "core/tests/unit.rs",
        "app/Cargo.toml",
        "app/src/lib.rs",
        "server/Cargo.toml",
        "server/src/main.rs",
        "shared/Cargo.toml",
        "shared/src/lib.rs",
    ] {
        assert!(paths.contains(&want), "{want} missing from {paths:?}");
    }
    assert!(
        get(&f, "Cargo.toml").contains("members = [\"shared\", \"core\", \"app\", \"server\"]")
    );
    assert!(get(&f, "app/src/lib.rs").contains("greet"));
    assert!(get(&f, ".quench.toml").contains("template = \"fullstack\""));
}

#[quench_test::test]
fn a_web_project_has_no_shared_crate() {
    let f = files(Template::Web, Some("/q"));
    assert!(f.iter().all(|(p, _)| !p.starts_with("shared/")));
    assert!(get(&f, "Cargo.toml").contains("members = [\"core\", \"app\", \"server\"]"));
    assert!(!get(&f, "app/src/lib.rs").contains("greet"));
    assert!(!get(&f, "app/Cargo.toml").contains("shared"));
    assert!(!get(&f, "server/Cargo.toml").contains("shared"));
}

#[quench_test::test]
fn the_server_can_export_the_bundle_for_the_android_host() {
    for template in [Template::Web, Template::Fullstack] {
        let f = files(template, Some("/q"));
        let main = get(&f, "server/src/main.rs");
        assert!(
            main.contains("QUENCH_EXPORT") && main.contains(".export("),
            "{main}"
        );
    }
}

#[quench_test::test]
fn nothing_of_the_template_syntax_is_left_over() {
    for template in [Template::Web, Template::Fullstack] {
        for (path, body) in files(template, None) {
            assert!(
                !body.contains("{{") && !body.contains("{%"),
                "{path}:\n{body}"
            );
            assert!(
                !body.contains("my-app-core{")
                    && body.contains(if path == ".quench.toml" { "my-app" } else { "" })
            );
        }
    }
}

#[quench_test::test]
fn names_reach_the_crates_and_the_code() {
    let f = files(Template::Fullstack, Some("/q"));
    assert!(get(&f, "app/Cargo.toml").contains("name = \"my-app-app\""));
    assert!(get(&f, "app/Cargo.toml").contains("name = \"my_app_app\""));
    assert!(get(&f, "app/src/lib.rs").contains("use my_app_core::step;"));
    assert!(get(&f, "server/src/main.rs").contains("my_app_app::app()"));
    assert!(get(&f, "server/src/main.rs").contains("wasm-dev/my_app_app.wasm"));
}

#[quench_test::test]
fn quench_comes_from_the_registry_or_from_a_checkout() {
    let registry = files(Template::Web, None);
    assert!(
        get(&registry, "Cargo.toml")
            .contains("quench-ui = { registry = \"ennor\", version = \"0.1.")
    );
    assert!(get(&registry, ".cargo/config.toml").contains("[registries.ennor]"));
    let local = files(Template::Web, Some("/home/me/quench/"));
    assert!(
        get(&local, "Cargo.toml").contains("quench-ui = { path = \"/home/me/quench/quench-ui\" }")
    );
    assert!(local.iter().all(|(p, _)| p != ".cargo/config.toml"));
}

#[quench_test::test]
fn the_version_table_follows_the_workspace() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for (name, minimum) in versions::CRATES {
        let manifest = std::fs::read_to_string(root.join(name).join("Cargo.toml")).unwrap();
        let version = manifest
            .lines()
            .find_map(|l| l.strip_prefix("version = \""))
            .and_then(|l| l.split('"').next())
            .unwrap_or_else(|| panic!("{name} has no version"));
        let minor = |v: &str| v.split('.').take(2).collect::<Vec<_>>().join(".");
        assert_eq!(
            minor(version),
            minor(minimum),
            "{name}: workspace {version}, table {minimum}"
        );
    }
}

#[quench_test::test]
fn create_writes_the_files_and_refuses_to_overwrite() {
    let cwd = tmp("create");
    let mut out = Vec::new();
    let q = env!("CARGO_MANIFEST_DIR");
    let written =
        scaffold::create(&mut out, &cwd, "demo", Template::Fullstack, None, Some(q)).unwrap();
    assert_eq!(written.len(), 14);
    assert!(cwd.join("demo/shared/src/lib.rs").is_file());
    let e = scaffold::create(&mut out, &cwd, "demo", Template::Web, None, Some(q)).unwrap_err();
    assert!(e.to_string().contains("not empty"), "{e}");
    // An empty directory is fine, and `--dir` chooses it.
    std::fs::create_dir_all(cwd.join("elsewhere")).unwrap();
    scaffold::create(
        &mut out,
        &cwd,
        "other",
        Template::Web,
        Some("elsewhere"),
        Some(q),
    )
    .unwrap();
    assert!(cwd.join("elsewhere/.quench.toml").is_file());
}

#[quench_test::test]
fn create_rejects_a_bad_name_and_a_missing_checkout() {
    let cwd = tmp("create-bad");
    let mut out = Vec::new();
    assert!(scaffold::create(&mut out, &cwd, "Bad", Template::Web, None, None).is_err());
    let e = scaffold::create(
        &mut out,
        &cwd,
        "ok",
        Template::Web,
        None,
        Some("/no/such/dir"),
    )
    .unwrap_err();
    assert!(e.to_string().contains("--quench-path"), "{e}");
}
