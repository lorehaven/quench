use super::support::*;
use quench_tool::project::Project;
use quench_tool::publish::{Options, run};

fn ready(label: &str) -> (Fake, Project) {
    let p = project_named(label, "my-app", "web");
    quench_tool::android::init(&mut Vec::new(), &p, Some("dev.lorehaven.myapp")).unwrap();
    let manifest = p.root.join("quench.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text.replace(
            "version_code = 1",
            "version_code = 7\nregistry = \"https://warehouse.example/\"",
        ),
    )
    .unwrap();
    let p = Project::find(&p.root).unwrap();
    std::fs::create_dir_all(p.root.join("out")).unwrap();
    std::fs::write(p.root.join("out/app.apk"), vec![7_u8; 4096]).unwrap();
    let mut sys = Fake::default();
    sys.vars.insert("ANDROID_HOME".into(), "/sdk".into());
    sys.vars.insert("WAREHOUSE_TOKEN".into(), "tok".into());
    sys.dirs
        .insert("/sdk/build-tools".into(), vec!["36.0.0".into()]);
    sys.existing
        .insert("/sdk/build-tools/36.0.0/apksigner".into());
    sys.captures.insert(
        format!(
            "/sdk/build-tools/36.0.0/apksigner verify --print-certs {}",
            p.root.join("out/app.apk").display()
        ),
        "Signer #1 certificate DN: CN=me\n".into(),
    );
    (sys, p)
}

fn options() -> Options {
    Options {
        apk: Some("out/app.apk".into()),
        ..Options::default()
    }
}

#[quench_test::test]
fn the_apk_is_put_to_the_registry_under_its_package_and_version_code() {
    let (sys, p) = ready("publish-ok");
    let mut out = Vec::new();
    run(&sys, &mut out, &p, &options()).unwrap();
    let requests = sys.requests.borrow();
    assert_eq!(requests.len(), 1);
    let r = &requests[0];
    assert_eq!(r.method, "PUT");
    assert_eq!(
        r.url,
        "https://warehouse.example/warehouse/api/v1/artifacts/dev.lorehaven.myapp/android/7"
    );
    assert!(
        r.headers
            .contains(&("authorization".into(), "Bearer tok".into()))
    );
    assert!(r.headers.contains(&(
        "content-type".into(),
        "application/vnd.android.package-archive".into()
    )));
    assert_eq!(r.body.len(), 4096);
    let said = text(out);
    assert!(
        said.contains("publishing dev.lorehaven.myapp 0.1.0 (version_code 7, 4 KiB")
            && said.contains("published dev.lorehaven.myapp android 7"),
        "{said}"
    );
}

#[quench_test::test]
fn a_dry_run_checks_everything_and_sends_nothing() {
    let (sys, p) = ready("publish-dry");
    let mut out = Vec::new();
    run(
        &sys,
        &mut out,
        &p,
        &Options {
            dry_run: true,
            ..options()
        },
    )
    .unwrap();
    assert!(sys.requests.borrow().is_empty());
    assert!(text(out).contains("would publish dev.lorehaven.myapp"));
    let (mut sys, p) = ready("publish-dry-unsigned");
    sys.captures.clear();
    assert!(
        run(
            &sys,
            &mut Vec::new(),
            &p,
            &Options {
                dry_run: true,
                ..options()
            }
        )
        .is_err(),
        "an unsigned APK is refused even in a dry run"
    );
}

#[quench_test::test]
fn the_registry_answers_are_explained() {
    let (sys, p) = ready("publish-409");
    sys.replies.borrow_mut().push((409, String::new()));
    let e = run(&sys, &mut Vec::new(), &p, &options())
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("version_code 7 is already published") && e.contains("raise version_code"),
        "{e}"
    );
    sys.replies.borrow_mut().push((500, "boom".into()));
    let e = run(&sys, &mut Vec::new(), &p, &options())
        .unwrap_err()
        .to_string();
    assert!(e.contains("HTTP 500 boom"), "{e}");
}

#[quench_test::test]
fn credentials_are_a_token_or_a_login() {
    let (mut sys, p) = ready("publish-login");
    sys.vars.remove("WAREHOUSE_TOKEN");
    let e = run(&sys, &mut Vec::new(), &p, &options())
        .unwrap_err()
        .to_string();
    assert!(e.contains("WAREHOUSE_TOKEN"), "{e}");
    sys.vars.insert("WAREHOUSE_USER".into(), "ola".into());
    sys.vars.insert("WAREHOUSE_PASS".into(), "pw".into());
    sys.replies
        .borrow_mut()
        .push((200, "{\"access_token\":\"fresh\"}".into()));
    run(&sys, &mut Vec::new(), &p, &options()).unwrap();
    let requests = sys.requests.borrow();
    assert_eq!(
        requests[0].url,
        "https://warehouse.example/gatehouse/api/v1/auth/login"
    );
    assert!(String::from_utf8_lossy(&requests[0].body).contains("\"username\":\"ola\""));
    assert!(
        requests[1]
            .headers
            .contains(&("authorization".into(), "Bearer fresh".into()))
    );
    drop(requests);
    sys.replies.borrow_mut().push((401, String::new()));
    assert!(
        run(&sys, &mut Vec::new(), &p, &options())
            .unwrap_err()
            .to_string()
            .contains("login was refused")
    );
    sys.replies.borrow_mut().push((200, "not json".into()));
    assert!(run(&sys, &mut Vec::new(), &p, &options()).is_err());
}

#[quench_test::test]
fn the_registry_is_named_by_the_environment_or_the_project() {
    let (mut sys, p) = ready("publish-server");
    sys.vars
        .insert("WAREHOUSE_URL".into(), "https://other.example".into());
    run(&sys, &mut Vec::new(), &p, &options()).unwrap();
    assert!(
        sys.requests.borrow()[0]
            .url
            .starts_with("https://other.example/warehouse/")
    );
    let bare = project("publish-noregistry", "web");
    let e = run(
        &Fake::default(),
        &mut Vec::new(),
        &bare,
        &Options::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("WAREHOUSE_URL"), "{e}");
}

#[quench_test::test]
fn an_unreachable_registry_is_reported() {
    let (mut sys, p) = ready("publish-down");
    sys.vars
        .insert("WAREHOUSE_URL".into(), "https://unreachable.example".into());
    let e = run(&sys, &mut Vec::new(), &p, &options())
        .unwrap_err()
        .to_string();
    assert!(e.contains("publish: connection refused"), "{e}");
}

#[quench_test::test]
fn a_dirty_tree_is_refused_unless_allowed_and_a_commit_is_named_when_clean() {
    let (mut sys, p) = ready("publish-git");
    let root = p.root.display();
    sys.captures.insert(
        format!("git -C {root} rev-parse --is-inside-work-tree"),
        "true\n".into(),
    );
    sys.captures.insert(
        format!("git -C {root} status --porcelain"),
        " M app/src/lib.rs\n?? notes.txt\n".into(),
    );
    sys.captures.insert(
        format!("git -C {root} rev-parse --short HEAD"),
        "abc1234\n".into(),
    );
    let e = run(&sys, &mut Vec::new(), &p, &options())
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("uncommitted changes")
            && e.contains("app/src/lib.rs")
            && e.contains("--allow-dirty"),
        "{e}"
    );
    assert!(sys.requests.borrow().is_empty());
    let mut out = Vec::new();
    run(
        &sys,
        &mut out,
        &p,
        &Options {
            allow_dirty: true,
            ..options()
        },
    )
    .unwrap();
    assert!(text(out).contains("commit abc1234-dirty"));
    sys.captures
        .insert(format!("git -C {root} status --porcelain"), String::new());
    let mut out = Vec::new();
    run(&sys, &mut out, &p, &options()).unwrap();
    let said = text(out);
    assert!(
        said.contains("commit abc1234)"),
        "a clean tree names its commit: {said}"
    );
}

#[quench_test::test]
fn without_an_apk_given_the_release_is_built_first() {
    let (mut sys, p) = ready("publish-build");
    sys.vars
        .insert("QUENCH_KEYSTORE_PASSWORD".into(), "secret".into());
    let manifest = p.root.join("quench.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text + "keystore = \"release.keystore\"\nkey_alias = \"release\"\n",
    )
    .unwrap();
    let p = Project::find(&p.root).unwrap();
    sys.existing.insert(p.root.join("release.keystore"));
    let release = p.root.join("android/app/build/outputs/apk/release");
    std::fs::create_dir_all(&release).unwrap();
    std::fs::write(release.join("app-release.apk"), vec![1_u8; 2048]).unwrap();
    std::fs::create_dir_all(p.wasm_path("wasm").parent().unwrap()).unwrap();
    sys.captures.insert(
        format!(
            "/sdk/build-tools/36.0.0/apksigner verify --print-certs {}",
            p.dist().join("my-app-0.1.0-7.apk").display()
        ),
        "Signer #1 certificate DN: CN=me\n".into(),
    );
    run(&sys, &mut Vec::new(), &p, &Options::default()).unwrap();
    assert!(sys.lines().iter().any(|l| l.contains("assembleRelease")));
    assert_eq!(sys.requests.borrow()[0].body.len(), 2048);
}
