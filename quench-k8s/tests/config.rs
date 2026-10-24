//! Kubeconfig and service-account configuration, and the small value helpers.

use std::path::Path;

use quench_core::base64;
use quench_core::stream::{StreamExt, iter};
use quench_k8s::{ApiResource, Auth, Config, Error, Object, guess_plural, lines};
use quench_serde::json;

fn kubeconfig(user: &str, cluster_extra: &str) -> String {
    format!(
        r"
apiVersion: v1
kind: Config
current-context: prod
contexts:
- name: prod
  context: {{cluster: main, user: me, namespace: forge}}
- name: other
  context: {{cluster: second, user: me}}
clusters:
- name: main
  cluster:
    server: https://k8s.example:6443/
    certificate-authority-data: {ca}
{cluster_extra}
- name: second
  cluster: {{server: 'https://second:6443', insecure-skip-tls-verify: true}}
users:
- name: me
  user:
{user}
",
        ca = base64::STANDARD.encode("CA PEM")
    )
}

#[test]
fn a_kubeconfig_with_a_token_and_embedded_ca() {
    let config =
        Config::from_kubeconfig_str(&kubeconfig("    token: abc123", ""), None, None).unwrap();
    assert_eq!(config.server, "https://k8s.example:6443");
    assert_eq!(config.namespace, "forge");
    assert_eq!(config.ca_pem.as_deref(), Some(&b"CA PEM"[..]));
    assert!(!config.insecure);
    assert!(matches!(&config.auth, Auth::Bearer(t) if t == "abc123"));
    assert!(
        format!("{:?}", config.auth).contains("Bearer(..)")
            && !format!("{:?}", config.auth).contains("abc123")
    );

    let other =
        Config::from_kubeconfig_str(&kubeconfig("    token: abc123", ""), Some("other"), None)
            .unwrap();
    assert_eq!(other.server, "https://second:6443");
    assert!(other.insecure);
    assert_eq!(other.namespace, "default");
    assert!(other.ca_pem.is_none());
}

#[test]
fn client_certificates_basic_auth_and_token_files() {
    let cert = base64::STANDARD.encode("CERT");
    let key = base64::STANDARD.encode("KEY");
    let with_cert = kubeconfig(
        &format!("    client-certificate-data: {cert}\n    client-key-data: {key}"),
        "",
    );
    let config = Config::from_kubeconfig_str(&with_cert, None, None).unwrap();
    assert!(
        matches!(&config.auth, Auth::ClientCertificate { certificate, key } if certificate == b"CERT" && key == b"KEY")
    );

    let basic = Config::from_kubeconfig_str(
        &kubeconfig("    username: u\n    password: p", ""),
        None,
        None,
    )
    .unwrap();
    assert!(
        matches!(&basic.auth, Auth::Basic { username, password } if username == "u" && password == "p")
    );

    let file = Config::from_kubeconfig_str(
        &kubeconfig("    tokenFile: tokens/t", ""),
        None,
        Some(Path::new("/etc/k")),
    )
    .unwrap();
    assert!(matches!(&file.auth, Auth::BearerFile(p) if p == Path::new("/etc/k/tokens/t")));
    let absolute = Config::from_kubeconfig_str(
        &kubeconfig("    tokenFile: /var/t", ""),
        None,
        Some(Path::new("/etc/k")),
    )
    .unwrap();
    assert!(matches!(&absolute.auth, Auth::BearerFile(p) if p == Path::new("/var/t")));

    let anonymous = Config::from_kubeconfig_str(&kubeconfig("    {}", ""), None, None).unwrap();
    assert_eq!(anonymous.auth, Auth::None);
}

#[test]
fn certificate_files_are_read_relative_to_the_kubeconfig() {
    let dir = std::env::temp_dir().join(format!("quench-k8s-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("client.crt"), "FILE CERT").unwrap();
    std::fs::write(dir.join("client.key"), "FILE KEY").unwrap();
    std::fs::write(dir.join("ca.crt"), "FILE CA").unwrap();
    let text = r"
current-context: c
contexts: [{name: c, context: {cluster: k, user: u}}]
clusters: [{name: k, cluster: {server: 'https://h:1', certificate-authority: ca.crt}}]
users: [{name: u, user: {client-certificate: client.crt, client-key: client.key}}]
";
    let path = dir.join("config");
    std::fs::write(&path, text).unwrap();
    let config = Config::from_kubeconfig(&path, None).unwrap();
    assert_eq!(config.ca_pem.as_deref(), Some(&b"FILE CA"[..]));
    assert!(
        matches!(&config.auth, Auth::ClientCertificate { certificate, key } if certificate == b"FILE CERT" && key == b"FILE KEY")
    );

    std::fs::remove_file(dir.join("client.key")).unwrap();
    assert!(
        matches!(Config::from_kubeconfig(&path, None), Err(Error::Config(m)) if m.contains("client.key"))
    );
    assert!(
        matches!(Config::from_kubeconfig(&dir.join("absent"), None), Err(Error::Config(m)) if m.contains("cannot read"))
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unusable_kubeconfigs_are_described() {
    let check = |text: &str, context: Option<&str>, wanted: &str| {
        let error = Config::from_kubeconfig_str(text, context, None).unwrap_err();
        assert!(error.to_string().contains(wanted), "{text}\n{error}");
    };
    check("not: [valid", None, "not a kubeconfig");
    check("clusters: []", None, "no current-context");
    check(
        &kubeconfig("    token: t", ""),
        Some("missing"),
        "no context `missing`",
    );
    let base = kubeconfig("    token: t", "");
    check(
        &base.replace("cluster: main, user", "cluster: gone, user"),
        None,
        "no cluster `gone`",
    );
    check(
        &base.replace("user: me, namespace", "user: gone, namespace"),
        None,
        "no user `gone`",
    );
    check(
        &base.replace("server: https://k8s.example:6443/", "other: x"),
        None,
        "has no server",
    );
    check(
        &base.replace(
            "certificate-authority-data: ",
            "certificate-authority-data: $$",
        ),
        None,
        "not valid base64",
    );
    check(
        &kubeconfig("    client-certificate-data: QQ==", ""),
        None,
        "without its key",
    );
    check(
        &kubeconfig("    exec: {command: aws}", ""),
        None,
        "credential plugin",
    );
    check(
        &kubeconfig("    auth-provider: {name: gcp}", ""),
        None,
        "credential plugin",
    );
    assert!(matches!(
        Config::from_kubeconfig_str(&kubeconfig("    exec: {command: aws}", ""), None, None),
        Err(Error::Unsupported(_))
    ));
    let no_user = "current-context: c\ncontexts: [{name: c, context: {cluster: k}}]\nclusters: [{name: k, cluster: {server: 'http://h'}}]\n";
    assert_eq!(
        Config::from_kubeconfig_str(no_user, None, None)
            .unwrap()
            .auth,
        Auth::None
    );
    let no_cluster = "current-context: c\ncontexts: [{name: c, context: {}}]\n";
    check(no_cluster, None, "names no cluster");
}

#[test]
fn the_service_account_directory_makes_a_config() {
    let dir = std::env::temp_dir().join(format!("quench-k8s-sa-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ca.crt"), "SA CA").unwrap();
    std::fs::write(dir.join("token"), "tok").unwrap();
    std::fs::write(dir.join("namespace"), "forge\n").unwrap();
    let config = Config::in_cluster_at(&dir, "10.43.0.1", "443").unwrap();
    assert_eq!(config.server, "https://10.43.0.1:443");
    assert_eq!(config.namespace, "forge");
    assert_eq!(config.ca_pem.as_deref(), Some(&b"SA CA"[..]));
    assert!(matches!(&config.auth, Auth::BearerFile(p) if p == &dir.join("token")));
    assert_eq!(
        Config::in_cluster_at(&dir, "fd00::1", "6443")
            .unwrap()
            .server,
        "https://[fd00::1]:6443"
    );

    std::fs::remove_file(dir.join("namespace")).unwrap();
    assert_eq!(
        Config::in_cluster_at(&dir, "h", "1").unwrap().namespace,
        "default"
    );
    std::fs::remove_file(dir.join("ca.crt")).unwrap();
    assert!(
        matches!(Config::in_cluster_at(&dir, "h", "1"), Err(Error::Config(m)) if m.contains("CA"))
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn inference_uses_the_environment() {
    // KUBECONFIG, HOME and the service host are process-wide, so all of it is in one test.
    let saved: Vec<_> = [
        "KUBECONFIG",
        "HOME",
        "KUBERNETES_SERVICE_HOST",
        "KUBERNETES_SERVICE_PORT",
    ]
    .iter()
    .map(|k| (*k, std::env::var_os(k)))
    .collect();
    let dir = std::env::temp_dir().join(format!("quench-k8s-env-{}", std::process::id()));
    std::fs::create_dir_all(dir.join(".kube")).unwrap();
    let text = "current-context: c\ncontexts: [{name: c, context: {cluster: k, user: u}}]\nclusters: [{name: k, cluster: {server: 'https://from-kubeconfig:1'}}]\nusers: [{name: u, user: {token: t}}]\n";

    quench_core::env::remove("KUBECONFIG");
    quench_core::env::remove("KUBERNETES_SERVICE_HOST");
    quench_core::env::set("HOME", &dir);
    std::fs::write(dir.join(".kube/config"), text).unwrap();
    assert_eq!(Config::infer().unwrap().server, "https://from-kubeconfig:1");

    let two = text.replace(
        "contexts: [",
        "contexts: [{name: d, context: {cluster: k2, user: u}}, ",
    );
    let two = two.replace(
        "clusters: [",
        "clusters: [{name: k2, cluster: {server: 'https://other:2'}}, ",
    );
    std::fs::write(dir.join(".kube/config"), &two).unwrap();
    assert_eq!(
        Config::infer_with_context(Some("d")).unwrap().server,
        "https://other:2"
    );
    assert!(Config::infer_with_context(Some("nope")).is_err());
    std::fs::write(dir.join(".kube/config"), text).unwrap();

    let explicit = dir.join("explicit");
    std::fs::write(&explicit, text.replace("from-kubeconfig", "from-env")).unwrap();
    let list = std::env::join_paths([dir.join("missing"), explicit]).unwrap();
    quench_core::env::set("KUBECONFIG", list);
    assert_eq!(Config::infer().unwrap().server, "https://from-env:1");

    quench_core::env::remove("KUBECONFIG");
    std::fs::remove_file(dir.join(".kube/config")).unwrap();
    assert!(
        Config::infer()
            .unwrap_err()
            .to_string()
            .contains("not running inside a cluster")
    );
    assert!(
        Config::in_cluster()
            .unwrap_err()
            .to_string()
            .contains("KUBERNETES_SERVICE_HOST")
    );

    for (key, value) in saved {
        match value {
            Some(v) => quench_core::env::set(key, v),
            None => quench_core::env::remove(key),
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn plurals_are_guessed_like_kubernetes_names_them() {
    for (kind, plural) in [
        ("Pod", "pods"),
        ("Ingress", "ingresses"),
        ("NetworkPolicy", "networkpolicies"),
        ("Gateway", "gateways"),
        ("Endpoints", "endpointses"),
        ("Batch", "batches"),
        ("Y", "ys"),
        ("Box", "boxes"),
        ("Wish", "wishes"),
    ] {
        assert_eq!(guess_plural(kind), plural, "{kind}");
    }
    let manifest =
        json::parse(r#"{"apiVersion":"networking.k8s.io/v1","kind":"Ingress"}"#).unwrap();
    let resource = ApiResource::from_manifest(&manifest, true).unwrap();
    assert_eq!(
        (resource.group.as_str(), resource.plural.as_str()),
        ("networking.k8s.io", "ingresses")
    );
    let core = ApiResource::from_manifest(
        &json::parse(r#"{"apiVersion":"v1","kind":"Secret"}"#).unwrap(),
        true,
    )
    .unwrap();
    assert_eq!(core, ApiResource::secret());
    assert!(
        ApiResource::from_manifest(&json::parse(r#"{"kind":"Secret"}"#).unwrap(), true).is_none()
    );
    for resource in [
        ApiResource::pod(),
        ApiResource::service(),
        ApiResource::secret(),
        ApiResource::config_map(),
        ApiResource::namespace(),
        ApiResource::node(),
        ApiResource::deployment(),
        ApiResource::stateful_set(),
        ApiResource::daemon_set(),
        ApiResource::job(),
    ] {
        assert_eq!(
            guess_plural(&resource.kind),
            resource.plural,
            "{}",
            resource.kind
        );
    }
}

#[test]
fn objects_expose_their_metadata() {
    let pod = json::parse(
        r#"{"kind":"Pod","metadata":{"name":"web","namespace":"forge","uid":"u-1","resourceVersion":"12",
            "labels":{"app":"web","tier":"fe","bad":5},"annotations":{"note":"hi"},
            "creationTimestamp":"2026-10-23T16:08:29Z","deletionTimestamp":"2026-10-24T00:00:00Z"}}"#,
    )
    .unwrap();
    let object = Object(&pod);
    assert_eq!(object.name(), Some("web"));
    assert_eq!(
        (
            object.namespace(),
            object.uid(),
            object.resource_version(),
            object.kind()
        ),
        (Some("forge"), Some("u-1"), Some("12"), Some("Pod"))
    );
    assert_eq!(object.label("app"), Some("web"));
    assert_eq!(object.label("missing"), None);
    assert_eq!(object.annotation("note"), Some("hi"));
    assert_eq!(
        object.labels().collect::<Vec<_>>(),
        [("app", "web"), ("tier", "fe")],
        "non-string values are skipped"
    );
    assert_eq!(object.annotations().count(), 1);
    assert_eq!(
        object.creation_timestamp().unwrap().to_rfc3339(),
        "2026-10-23T16:08:29Z"
    );
    assert!(object.deletion_timestamp().is_some());

    let bare = json::parse("{}").unwrap();
    let bare = Object(&bare);
    assert!(
        bare.name().is_none()
            && bare.creation_timestamp().is_none()
            && bare.labels().next().is_none()
    );
}

#[tokio::test]
async fn lines_handle_errors_and_missing_final_breaks() {
    let chunk = |s: &'static [u8]| Ok::<_, Error>(quench_core::bytes::Bytes::from_static(s));
    let collected = lines(Box::pin(iter(vec![chunk(b"a\n\nb"), chunk(b"c\r\n")])))
        .collect_vec()
        .await;
    assert_eq!(
        collected
            .into_iter()
            .map(Result::unwrap)
            .collect::<Vec<_>>(),
        ["a", "", "bc"]
    );

    let failing = lines(Box::pin(iter(vec![
        chunk(b"ok\npartial"),
        Err(Error::Transport("cut".into())),
    ])))
    .collect_vec()
    .await;
    assert_eq!(failing[0].as_ref().unwrap(), "ok");
    assert!(matches!(&failing[1], Err(Error::Transport(m)) if m == "cut"));
    assert_eq!(
        failing.len(),
        2,
        "the partial line is dropped with the failure"
    );
    assert!(
        lines(Box::pin(iter(Vec::<
            Result<quench_core::bytes::Bytes, Error>,
        >::new())))
        .collect_vec()
        .await
        .is_empty()
    );
}
