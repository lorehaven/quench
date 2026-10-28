use super::support::*;
use quench_tool::linux::{app_id, build, deb, init, launch_dev, package, run, title};
use quench_tool::project::Project;

/// A project with a workspace `Cargo.toml` as `quench new` writes it.
fn workspace_project(label: &str) -> Project {
    let p = project_named(label, "my-app", "web");
    std::fs::write(
        p.root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"core\", \"app\", \"server\"]\nresolver = \"2\"\n\n[workspace.package]\nversion = \"0.3.2\"\nedition = \"2024\"\n\n[workspace.dependencies]\nquench-ui = { path = \"/q/quench-ui\" }\nquench-core = { path = \"/q/quench-core\" }\n",
    )
    .unwrap();
    p
}

fn read(p: &Project, path: &str) -> String {
    std::fs::read_to_string(p.root.join(path)).unwrap()
}

/// Puts the binary where cargo would have.
fn built(p: &Project, release: bool) {
    let dir = p
        .target_dir()
        .join(if release { "release" } else { "debug" });
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(package(p)), b"ELF-binary").unwrap();
}

#[quench_test::test]
fn init_joins_the_workspace_and_writes_the_host() {
    let p = workspace_project("linux-init");
    let mut out = Vec::new();
    init(&mut out, &p, Some("dev.lorehaven.myapp"), None).unwrap();
    assert!(text(out).contains("dev.lorehaven.myapp"));
    for file in ["Cargo.toml", "build.rs", "src/main.rs"] {
        assert!(p.root.join("desktop").join(file).is_file(), "{file}");
    }
    let cargo = read(&p, "desktop/Cargo.toml");
    assert!(
        cargo.contains("name = \"my-app-desktop\"") && cargo.contains("version.workspace = true")
    );
    assert!(
        cargo.contains("quench-desktop = { workspace = true, features = [\"window\"] }")
            && !cargo.contains("[workspace]")
    );
    let main = read(&p, "desktop/src/main.rs");
    assert!(
        main.contains("App::new(\"my-app\", bundle())")
            && main.contains(".standard_plugins(\"dev.lorehaven.myapp\")")
    );
    assert!(!main.contains("{{") && !main.contains("{%"), "{main}");
    let root = read(&p, "Cargo.toml");
    assert!(root.contains("members = [\"desktop\", \"core\""), "{root}");
    assert!(
        root.contains("quench-desktop = { path = \"/q/quench-desktop\" }"),
        "from the same checkout: {root}"
    );
    assert!(read(&p, "quench.toml").contains("[linux]\nid = \"dev.lorehaven.myapp\""));
    assert!(
        !p.root.join(".gitignore").exists(),
        "a workspace's own target/ is the project's already"
    );
    // The project now knows its id.
    let again = Project::find(&p.root).unwrap();
    assert_eq!(app_id(&again), "dev.lorehaven.myapp");
}

#[quench_test::test]
fn init_makes_a_crate_of_its_own_when_there_is_no_workspace() {
    let p = project_named("linux-alone", "gallery", "web");
    init(&mut Vec::new(), &p, None, Some("/checkout/")).unwrap();
    let cargo = read(&p, "desktop/Cargo.toml");
    assert!(
        cargo.contains("version = \"0.1.0\"")
            && cargo.contains(
                "quench-desktop = { path = \"/checkout/quench-desktop\", features = [\"window\"] }"
            ),
        "{cargo}"
    );
    assert!(
        cargo.contains("\n[workspace]"),
        "its own workspace: {cargo}"
    );
    assert!(!p.root.join("Cargo.toml").exists());
    // Its own `target/` is ignored, once, whatever the file held before.
    assert_eq!(read(&p, ".gitignore"), "/desktop/target\n");
    let r = project_named("linux-alone-ignore", "gallery", "web");
    std::fs::write(r.root.join(".gitignore"), "/target\n/dist").unwrap();
    init(&mut Vec::new(), &r, None, Some("/q")).unwrap();
    assert_eq!(read(&r, ".gitignore"), "/target\n/dist\n/desktop/target\n");
    assert_eq!(
        app_id(&Project::find(&p.root).unwrap()),
        "com.example.gallery"
    );
    // From the registry when no checkout is named.
    let q = project_named("linux-alone-registry", "gallery", "web");
    init(&mut Vec::new(), &q, None, None).unwrap();
    let registry = read(&q, "desktop/Cargo.toml");
    assert!(
        registry.contains("registry = \"ennor\"") && registry.contains("features = [\"window\"]"),
        "{registry}"
    );
}

#[quench_test::test]
fn init_refuses_a_bad_id_and_an_existing_host() {
    let p = workspace_project("linux-bad");
    for bad in ["", "-x", ".x", "a b", "a/b", &"x".repeat(101)] {
        let e = init(&mut Vec::new(), &p, Some(bad), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("not an application id"), "{bad:?}: {e}");
    }
    assert!(
        !p.root.join("desktop").exists(),
        "nothing is written for a bad id"
    );
    init(&mut Vec::new(), &p, None, None).unwrap();
    let e = init(&mut Vec::new(), &p, None, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("exists already"), "{e}");
    // Initialising twice must not have changed the workspace.
    assert_eq!(read(&p, "Cargo.toml").matches("\"desktop\"").count(), 1);
}

#[quench_test::test]
fn the_build_makes_the_bundle_then_the_host_and_copies_it_to_dist() {
    let p = workspace_project("linux-build");
    init(&mut Vec::new(), &p, None, None).unwrap();
    built(&p, false);
    let sys = Fake::default();
    let mut out = Vec::new();
    let target = build(&sys, &mut out, &p, false).unwrap();
    let lines = sys.lines();
    let bundle = lines
        .iter()
        .position(|l| {
            l.contains("QUENCH_EXPORT") || l.contains("demo-server") || l.contains("my-app-server")
        })
        .expect("the bundle is made");
    let host = lines
        .iter()
        .position(|l| l == "cargo build -p my-app-desktop")
        .expect("the host is built");
    assert!(bundle < host, "{lines:?}");
    assert_eq!(target, p.dist().join("my-app-linux"));
    assert_eq!(std::fs::read(&target).unwrap(), b"ELF-binary");
    assert!(text(out).contains("my-app-linux"));
    // Release adds the flag and takes the release binary.
    built(&p, true);
    let sys = Fake::default();
    build(&sys, &mut Vec::new(), &p, true).unwrap();
    assert!(
        sys.lines()
            .iter()
            .any(|l| l == "cargo build -p my-app-desktop --release"),
        "{:?}",
        sys.lines()
    );
}

#[quench_test::test]
fn a_crate_of_its_own_is_built_from_its_directory() {
    let p = project_named("linux-build-alone", "gallery", "web");
    init(&mut Vec::new(), &p, None, Some("/q")).unwrap();
    let dir = p.root.join("desktop/target/debug");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("gallery-desktop"), b"ELF").unwrap();
    let sys = Fake::default();
    build(&sys, &mut Vec::new(), &p, false).unwrap();
    let cargo = sys
        .ran
        .borrow()
        .iter()
        .find(|c| c.line() == "cargo build")
        .cloned()
        .expect("a plain cargo build");
    assert_eq!(cargo.cwd, p.root.join("desktop"));
    assert!(p.dist().join("gallery-linux").is_file());
}

#[quench_test::test]
fn building_needs_the_host_and_its_binary() {
    let p = workspace_project("linux-missing");
    let e = build(&Fake::default(), &mut Vec::new(), &p, false)
        .unwrap_err()
        .to_string();
    assert!(e.contains("quench linux init"), "{e}");
    init(&mut Vec::new(), &p, None, None).unwrap();
    let e = build(&Fake::default(), &mut Vec::new(), &p, false)
        .unwrap_err()
        .to_string();
    assert!(e.contains("my-app-desktop"), "cargo built nothing: {e}");
    let sys = Fake::default();
    *sys.fail_on.borrow_mut() = Some("cargo build -p".into());
    assert!(build(&sys, &mut Vec::new(), &p, false).is_err());
}

#[quench_test::test]
fn run_builds_then_runs_the_binary_and_reports_its_failure() {
    let p = workspace_project("linux-run");
    init(&mut Vec::new(), &p, None, None).unwrap();
    built(&p, false);
    let sys = Fake::default();
    run(&sys, &mut Vec::new(), &p).unwrap();
    assert_eq!(
        sys.ran.borrow().last().unwrap().program,
        p.dist().join("my-app-linux").to_string_lossy()
    );
    let sys = Fake::default();
    *sys.fail_on.borrow_mut() = Some("my-app-linux".into());
    let e = run(&sys, &mut Vec::new(), &p).unwrap_err().to_string();
    assert!(e.contains("exit code 101"), "{e}");
}

#[quench_test::test]
fn the_dev_window_waits_for_the_server_and_opens_on_it() {
    let p = workspace_project("linux-dev");
    init(&mut Vec::new(), &p, None, None).unwrap();
    built(&p, false);
    let sys = Fake::default();
    sys.listening.set(true);
    let mut out = Vec::new();
    let window = launch_dev(&sys, &mut out, &p, 3123).unwrap();
    assert!(window.is_some());
    let spawned = sys.spawned.borrow();
    assert_eq!(
        spawned[0].0.env,
        [(
            "QUENCH_DEV_URL".to_string(),
            "http://127.0.0.1:3123".to_string()
        )]
    );
    drop(spawned);
    // A server that never listens, and a project with no host, are reported; the loop goes on.
    let sys = Fake::default();
    let mut out = Vec::new();
    assert!(launch_dev(&sys, &mut out, &p, 3123).unwrap().is_none());
    assert!(text(out).contains("did not start listening"));
    let none = workspace_project("linux-dev-none");
    let mut out = Vec::new();
    assert!(launch_dev(&sys, &mut out, &none, 1).unwrap().is_none());
    assert!(text(out).contains("quench linux init"));
}

/// The members of an `ar` archive: (name, bytes).
fn ar_members(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    assert_eq!(&bytes[..8], b"!<arch>\n");
    let mut at = 8;
    let mut out = Vec::new();
    while at < bytes.len() {
        let header = &bytes[at..at + 60];
        assert_eq!(&header[58..], b"`\n");
        let name = String::from_utf8_lossy(&header[..16]).trim().to_string();
        let size: usize = String::from_utf8_lossy(&header[48..58])
            .trim()
            .parse()
            .unwrap();
        out.push((name, bytes[at + 60..at + 60 + size].to_vec()));
        at += 60 + size + size % 2;
    }
    assert_eq!(at, bytes.len(), "no stray bytes");
    out
}

#[quench_test::test]
fn the_deb_has_the_binary_the_menu_entry_and_the_control_file() {
    let p = workspace_project("linux-deb");
    init(&mut Vec::new(), &p, Some("dev.lorehaven.myapp"), None).unwrap();
    let p = Project::find(&p.root).unwrap(); // as the next command would
    built(&p, true);
    let sys = Fake::default();
    let path = deb(&sys, &mut Vec::new(), &p).unwrap();
    assert_eq!(path, p.dist().join("my-app_0.3.2_amd64.deb"));
    assert!(sys.lines().iter().any(|l| l.ends_with("--release")));
    let members = ar_members(&std::fs::read(&path).unwrap());
    let names: Vec<_> = members.iter().map(|m| m.0.as_str()).collect();
    assert_eq!(names, ["debian-binary", "control.tar.gz", "data.tar.gz"]);
    assert_eq!(members[0].1, b"2.0\n");

    let control = quench_archive::gzip::decompress(&members[1].1, 1 << 20).unwrap();
    let control: Vec<_> = quench_archive::tar::Reader::new(&control)
        .map(Result::unwrap)
        .collect();
    assert_eq!(control[0].path, "./control");
    let text = String::from_utf8(control[0].data.to_vec()).unwrap();
    for want in [
        "Package: my-app\n",
        "Version: 0.3.2\n",
        "Architecture: amd64\n",
        "Depends: libwebkit2gtk-4.1-0, libgtk-3-0\n",
        "Installed-Size: 1\n",
    ] {
        assert!(text.contains(want), "{want:?} in {text}");
    }

    let data = quench_archive::gzip::decompress(&members[2].1, 1 << 20).unwrap();
    let entries: Vec<_> = quench_archive::tar::Reader::new(&data)
        .map(Result::unwrap)
        .collect();
    let exe = entries
        .iter()
        .find(|e| e.path == "./usr/bin/my-app")
        .expect("the binary");
    assert_eq!((exe.data, exe.mode), (&b"ELF-binary"[..], 0o755));
    let entry = entries
        .iter()
        .find(|e| e.path == "./usr/share/applications/dev.lorehaven.myapp.desktop")
        .expect("the menu entry");
    let entry = String::from_utf8(entry.data.to_vec()).unwrap();
    assert!(
        entry.contains("Exec=/usr/bin/my-app\n") && entry.contains("Name=my-app\n"),
        "{entry}"
    );
}

#[quench_test::test]
fn odd_sized_members_are_padded_and_the_version_falls_back() {
    // A one-byte binary makes odd-sized members somewhere; the archive must still parse.
    let p = project_named("linux-deb-odd", "gallery", "web");
    init(&mut Vec::new(), &p, None, Some("/q")).unwrap();
    let dir = p.root.join("desktop/target/release");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("gallery-desktop"), b"x").unwrap();
    let path = deb(&Fake::default(), &mut Vec::new(), &p).unwrap();
    assert_eq!(
        path.file_name().unwrap(),
        "gallery_0.1.0_amd64.deb",
        "the host crate's version"
    );
    assert_eq!(ar_members(&std::fs::read(&path).unwrap()).len(), 3);
}

#[quench_test::test]
fn the_names_come_from_the_project_and_its_linux_table() {
    let p = project_named("linux-names", "my-app", "web");
    assert_eq!(
        (
            app_id(&p).as_str(),
            title(&p).as_str(),
            package(&p).as_str()
        ),
        ("com.example.my_app", "my-app", "my-app-desktop")
    );
    std::fs::write(p.root.join("quench.toml"), "name = \"my-app\"\ntemplate = \"web\"\n\n[linux]\nid = \"org.x.app\"\ntitle = \"My App\"\n").unwrap();
    let p = Project::find(&p.root).unwrap();
    assert_eq!(
        (app_id(&p).as_str(), title(&p).as_str()),
        ("org.x.app", "My App")
    );
}
