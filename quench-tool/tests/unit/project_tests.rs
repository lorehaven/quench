use super::support::*;
use quench_tool::cli::Template;
use quench_tool::project::{Project, check_name};

#[quench_test::test]
fn good_names_pass() {
    for name in ["demo", "my-app", "my_app", "a1", "x-2-y"] {
        assert!(check_name(name).is_ok(), "{name}");
    }
}

#[quench_test::test]
fn bad_names_say_why() {
    let long = "a".repeat(41);
    for (name, why) in [
        ("", "needs a name"),
        ("1app", "lowercase letter"),
        ("App", "lowercase letter"),
        ("my app", "`' '`"),
        ("my.app", "`.`"),
        ("quench-demo", "framework"),
        (long.as_str(), "40"),
    ] {
        let e = check_name(name).unwrap_err().to_string();
        assert!(
            e.contains(why) || why == "`' '`" && e.contains("` `"),
            "{name}: {e}"
        );
    }
}

#[quench_test::test]
fn a_project_is_found_from_a_directory_below_it() {
    let p = project("find", "fullstack");
    let below = p.root.join("a/b");
    std::fs::create_dir_all(&below).unwrap();
    let found = Project::find(&below).unwrap();
    assert_eq!(found, p);
    assert_eq!(found.template, Template::Fullstack);
}

#[quench_test::test]
fn no_manifest_is_explained() {
    let e = Project::find(&tmp("nothing")).unwrap_err().to_string();
    assert!(
        e.contains(".quench.toml") && e.contains("quench new"),
        "{e}"
    );
}

#[quench_test::test]
fn a_broken_manifest_is_explained() {
    let dir = tmp("broken");
    std::fs::write(
        dir.join(".quench.toml"),
        "name = \"demo\"\ntemplate = \"mobile\"\n",
    )
    .unwrap();
    let e = Project::find(&dir).unwrap_err().to_string();
    assert!(e.contains("unknown template `mobile`"), "{e}");
    std::fs::write(dir.join(".quench.toml"), "name = [").unwrap();
    assert!(
        Project::find(&dir)
            .unwrap_err()
            .to_string()
            .starts_with(".quench.toml")
    );
    std::fs::write(
        dir.join(".quench.toml"),
        "name = \"Bad Name\"\ntemplate = \"web\"\n",
    )
    .unwrap();
    assert!(
        Project::find(&dir)
            .unwrap_err()
            .to_string()
            .contains(".quench.toml")
    );
}

#[quench_test::test]
fn names_derive_from_the_project_name() {
    let p = project_named("derive", "my-app", "web");
    assert_eq!(p.snake(), "my_app");
    assert_eq!(p.app_package(), "my-app-app");
    assert_eq!(p.server_package(), "my-app-server");
    let wasm = p.wasm_path("wasm");
    assert!(
        wasm.ends_with("wasm32-unknown-unknown/wasm/my_app_app.wasm"),
        "{wasm:?}"
    );
    assert!(p.server_path("release").ends_with("release/my-app-server"));
    assert_eq!(p.dist(), p.root.join("dist"));
}

/// A project at `<repo>/services/demo`, inside a workspace of the repository's.
fn flat_project(label: &str) -> Project {
    let repo = tmp(label);
    std::fs::write(
        repo.join("Cargo.toml"),
        "[workspace]\nmembers = [\"services/demo/app\"]\n",
    )
    .unwrap();
    let root = repo.join("services/demo");
    for (dir, name) in [("app", "demo-app"), ("core", "demo-core")] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
        std::fs::write(
            root.join(dir).join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
        )
        .unwrap();
    }
    std::fs::create_dir_all(root.join("android")).unwrap();
    std::fs::write(
        root.join(".quench.toml"),
        "name = \"demo\"\ntemplate = \"web\"\n",
    )
    .unwrap();
    Project::find(&root).unwrap()
}

#[quench_test::test]
fn a_project_with_its_own_workspace_is_not_flat() {
    let p = project("own-workspace", "web");
    std::fs::write(p.root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    assert!(!p.flat());
    assert_eq!(p.workspace_root(), p.root);
    assert_eq!(p.member_path("desktop"), "desktop");
    assert_eq!(p.target_dir(), p.root.join("target"));
}

#[quench_test::test]
fn a_project_inside_a_workspace_is_flat_and_builds_into_its_target() {
    let p = flat_project("flat");
    let repo = p.root.parent().unwrap().parent().unwrap().to_path_buf();
    assert!(p.flat());
    assert_eq!(p.workspace_root(), repo);
    assert_eq!(p.workspace_manifest(), repo.join("Cargo.toml"));
    assert_eq!(p.member_path("desktop"), "services/demo/desktop");
    assert_eq!(p.target_dir(), repo.join("target"));
    // Only its own crates, by name: `--workspace` would take in the repository's.
    assert_eq!(p.packages(), ["demo-app", "demo-core"]);
}
