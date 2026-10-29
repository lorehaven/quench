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
