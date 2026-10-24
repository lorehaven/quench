//! The client against a mock API server that records what it was asked.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use std::sync::{Arc, Mutex};

use quench_core::async_trait;
use quench_core::stream::{StreamExt, iter};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::StatusCode;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::serve_http_on;
use quench_k8s::{
    Api, ApiResource, Auth, Client, Config, DeleteParams, ListParams, LogParams, Patch,
    PatchParams, PostParams, Propagation, WatchEvent,
};
use quench_serde::{Value, json};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

type Handler = Arc<dyn Fn(&Seen) -> Response + Send + Sync>;

struct Mock {
    seen: Arc<Mutex<Vec<Seen>>>,
    handler: Handler,
}

#[async_trait]
impl Endpoint for Mock {
    async fn call(&self, mut req: Request) -> Response {
        let body = req
            .take_body()
            .collect_limited(1 << 20)
            .await
            .unwrap_or_default();
        let headers = req
            .headers()
            .iter()
            .map(|(n, v)| {
                (
                    n.as_str().to_string(),
                    v.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let seen = Seen {
            method: req.method().as_str().to_string(),
            path: req.uri().path().to_string(),
            query: req.uri().query().unwrap_or_default().to_string(),
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        };
        let response = (self.handler)(&seen);
        self.seen.lock().unwrap().push(seen);
        response
    }
}

async fn server(
    handler: impl Fn(&Seen) -> Response + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    tokio::spawn(serve_http_on(
        Arc::new(Mock {
            seen: Arc::clone(&seen),
            handler: Arc::new(handler),
        }),
        container,
        listener,
        std::future::pending(),
    ));
    (format!("http://{addr}"), seen)
}

fn ok(value: &Value) -> Response {
    Response::json(StatusCode::OK, value).unwrap()
}

fn status(code: u16, reason: &str, message: &str) -> Response {
    Response::json(
        StatusCode::from_u16(code).unwrap(),
        &json!({"kind": "Status", "apiVersion": "v1", "status": "Failure", "code": code, "reason": reason, "message": message}),
    )
    .unwrap()
}

fn client(base: &str) -> Client {
    Client::new(Config::new(base)).unwrap()
}

fn pods(client: &Client, ns: &str) -> Api {
    client.namespaced(&ApiResource::pod(), ns)
}

#[quench_test::test]
async fn get_and_get_opt_build_the_right_paths_and_map_404() {
    let (base, seen) = server(|s| {
        if s.path.ends_with("/missing") {
            status(404, "NotFound", "pods \"missing\" not found")
        } else {
            ok(&json!({"kind": "Pod", "metadata": {"name": "web-1"}}))
        }
    })
    .await;
    let client = client(&base);
    let pod = pods(&client, "forge").get("web-1").await.unwrap();
    assert_eq!(pod["metadata"]["name"], "web-1");
    assert!(
        pods(&client, "forge")
            .get_opt("web-1")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        pods(&client, "forge")
            .get_opt("missing")
            .await
            .unwrap()
            .is_none()
    );

    let error = pods(&client, "forge").get("missing").await.unwrap_err();
    assert!(error.is_not_found() && !error.is_conflict() && !error.is_forbidden());
    let status = error.status().unwrap();
    assert_eq!((status.code, status.reason.as_str()), (404, "NotFound"));
    assert!(error.to_string().contains("pods \"missing\" not found"));

    let paths: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.path.clone())
        .collect();
    assert_eq!(paths[0], "/api/v1/namespaces/forge/pods/web-1");
    assert_eq!(
        seen.lock().unwrap()[0].header("accept"),
        Some("application/json")
    );
    assert_eq!(seen.lock().unwrap()[0].method, "GET");
}

#[quench_test::test]
async fn paths_for_groups_cluster_scope_and_odd_names() {
    let (base, seen) = server(|_| ok(&json!({"items": []}))).await;
    let client = client(&base);
    client
        .api(&ApiResource::deployment())
        .list(&ListParams::default())
        .await
        .unwrap();
    client
        .namespaced(&ApiResource::deployment(), "a b")
        .list(&ListParams::default())
        .await
        .unwrap();
    client
        .api(&ApiResource::namespace())
        .list(&ListParams::default())
        .await
        .unwrap();
    // A cluster-scoped kind ignores a namespace.
    client
        .namespaced(&ApiResource::node(), "ignored")
        .list(&ListParams::default())
        .await
        .unwrap();
    let custom = ApiResource::new(
        "traefik.io",
        "v1alpha1",
        "IngressRoute",
        "ingressroutes",
        true,
    );
    client
        .namespaced(&custom, "forge")
        .list(&ListParams::default())
        .await
        .unwrap();
    let paths: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.path.clone())
        .collect();
    assert_eq!(
        paths,
        [
            "/apis/apps/v1/deployments",
            "/apis/apps/v1/namespaces/a%20b/deployments",
            "/api/v1/namespaces",
            "/api/v1/nodes",
            "/apis/traefik.io/v1alpha1/namespaces/forge/ingressroutes",
        ]
    );
    assert_eq!(custom.api_version(), "traefik.io/v1alpha1");
    assert_eq!(ApiResource::pod().api_version(), "v1");
}

#[quench_test::test]
async fn a_namespaced_object_needs_a_namespace() {
    let (base, seen) = server(|_| ok(&json!({}))).await;
    let client = client(&base);
    let error = client.api(&ApiResource::pod()).get("x").await.unwrap_err();
    assert!(error.to_string().contains("namespaced"), "{error}");
    assert!(
        client
            .api(&ApiResource::pod())
            .delete("x", &DeleteParams::default())
            .await
            .is_err()
    );
    assert!(seen.lock().unwrap().is_empty(), "nothing was sent");
    // Cluster-scoped objects need none.
    assert!(
        client
            .api(&ApiResource::namespace())
            .get("forge")
            .await
            .is_ok()
    );
}

#[quench_test::test]
async fn lists_follow_pages_and_send_selectors() {
    let (base, seen) = server(|s| {
        if s.query.contains("continue=page2") {
            ok(&json!({"metadata": {"resourceVersion": "42"}, "items": [{"metadata": {"name": "c"}}]}))
        } else if s.query.contains("limit=2") {
            ok(&json!({"metadata": {"resourceVersion": "41", "continue": "page2"}, "items": [{"metadata": {"name": "a"}}, {"metadata": {"name": "b"}}]}))
        } else {
            ok(&json!({"metadata": {"resourceVersion": "7"}, "items": [{"metadata": {"name": "only"}}]}))
        }
    })
    .await;
    let client = client(&base);
    let params = ListParams {
        page_size: Some(2),
        ..ListParams::default()
            .labels("app=web,tier!=db")
            .fields("status.phase=Running")
    };
    let list = client.api(&ApiResource::pod()).list(&params).await.unwrap();
    let names: Vec<&str> = list
        .items
        .iter()
        .map(|i| i["metadata"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["a", "b", "c"]);
    assert_eq!(list.resource_version.as_deref(), Some("42"));
    let first = seen.lock().unwrap()[0].clone();
    assert!(
        first
            .query
            .contains("labelSelector=app%3Dweb%2Ctier%21%3Ddb"),
        "{}",
        first.query
    );
    assert!(
        first.query.contains("fieldSelector=status.phase%3DRunning"),
        "{}",
        first.query
    );
    assert_eq!(seen.lock().unwrap().len(), 2);

    let single = client
        .api(&ApiResource::pod())
        .list(&ListParams::default())
        .await
        .unwrap();
    assert_eq!(single.items.len(), 1);
}

#[quench_test::test]
#[allow(clippy::too_many_lines)] // one server, many calls, checked together
async fn create_replace_patch_and_delete_send_what_they_should() {
    let (base, seen) = server(|s| {
        if s.method == "POST" && s.body.contains("exists") {
            status(409, "AlreadyExists", "pods \"exists\" already exists")
        } else {
            ok(&json!({"kind": "Pod", "metadata": {"name": "web"}}))
        }
    })
    .await;
    let client = client(&base);
    let api = pods(&client, "forge");
    let manifest = json!({"apiVersion": "v1", "kind": "Pod", "metadata": {"name": "web"}});

    api.create(&manifest, &PostParams::default()).await.unwrap();
    api.create(
        &manifest,
        &PostParams {
            dry_run: true,
            field_manager: Some("me".into()),
        },
    )
    .await
    .unwrap();
    let conflict = api
        .create(
            &json!({"metadata": {"name": "exists"}}),
            &PostParams::default(),
        )
        .await
        .unwrap_err();
    assert!(conflict.is_conflict());

    api.replace("web", &manifest).await.unwrap();
    api.patch(
        "web",
        &Patch::Merge(json!({"metadata": {"labels": {"a": "b"}}})),
        &PatchParams::default(),
    )
    .await
    .unwrap();
    api.patch(
        "web",
        &Patch::StrategicMerge(json!({"spec": {}})),
        &PatchParams::default(),
    )
    .await
    .unwrap();
    api.patch(
        "web",
        &Patch::Json(json!([{"op": "remove", "path": "/x"}])),
        &PatchParams::default(),
    )
    .await
    .unwrap();
    api.patch(
        "web",
        &Patch::Apply(manifest.clone()),
        &PatchParams {
            field_manager: Some("quench".into()),
            force: true,
            dry_run: true,
        },
    )
    .await
    .unwrap();
    api.delete("web", &DeleteParams::default()).await.unwrap();
    api.delete("web", &DeleteParams::background())
        .await
        .unwrap();
    api.delete(
        "web",
        &DeleteParams {
            propagation: Some(Propagation::Foreground),
            grace_period_seconds: Some(0),
            dry_run: true,
        },
    )
    .await
    .unwrap();
    api.delete(
        "web",
        &DeleteParams {
            propagation: Some(Propagation::Orphan),
            ..DeleteParams::default()
        },
    )
    .await
    .unwrap();

    let calls = seen.lock().unwrap().clone();
    let summary: Vec<(&str, &str)> = calls
        .iter()
        .map(|s| (s.method.as_str(), s.path.as_str()))
        .collect();
    assert_eq!(summary[0], ("POST", "/api/v1/namespaces/forge/pods"));
    assert_eq!(summary[3], ("PUT", "/api/v1/namespaces/forge/pods/web"));
    assert_eq!(calls[0].header("content-type"), Some("application/json"));
    assert_eq!(json::from_str::<Value>(&calls[0].body).unwrap(), manifest);
    assert!(calls[1].query.contains("dryRun=All") && calls[1].query.contains("fieldManager=me"));

    assert_eq!(calls[4].method, "PATCH");
    assert_eq!(
        calls[4].header("content-type"),
        Some("application/merge-patch+json")
    );
    assert_eq!(
        calls[5].header("content-type"),
        Some("application/strategic-merge-patch+json")
    );
    assert_eq!(
        calls[6].header("content-type"),
        Some("application/json-patch+json")
    );
    assert_eq!(
        calls[7].header("content-type"),
        Some("application/apply-patch+yaml")
    );
    assert!(
        calls[7].query.contains("force=true")
            && calls[7].query.contains("fieldManager=quench")
            && calls[7].query.contains("dryRun=All")
    );

    assert_eq!(calls[8].method, "DELETE");
    assert!(calls[8].body.is_empty(), "plain delete sends no options");
    assert!(
        calls[9]
            .body
            .contains(r#""propagationPolicy":"Background""#),
        "{}",
        calls[9].body
    );
    assert!(
        calls[10].body.contains("Foreground")
            && calls[10].body.contains(r#""gracePeriodSeconds":0"#)
            && calls[10].query.contains("dryRun=All")
    );
    assert!(calls[11].body.contains("Orphan"));
}

#[quench_test::test]
async fn logs_are_text_and_can_stream_with_the_right_query() {
    let (base, seen) = server(|s| {
        if s.path.contains("/gone/") {
            status(404, "NotFound", "pod gone not found")
        } else if s.query.contains("follow=true") {
            Response::streaming(
                StatusCode::OK,
                iter(vec![
                    Ok::<_, std::io::Error>(quench_core::bytes::Bytes::from_static(
                        b"line one\nlin",
                    )),
                    Ok(quench_core::bytes::Bytes::from_static(
                        b"e two\r\nlast, no break",
                    )),
                ]),
            )
        } else {
            Response::text(StatusCode::OK, "plain log\n")
        }
    })
    .await;
    let client = client(&base);
    let api = pods(&client, "forge");

    let text = api
        .logs(
            "web",
            &LogParams {
                container: Some("steps".into()),
                tail_lines: Some(50),
                previous: true,
                timestamps: true,
                since_seconds: Some(60),
                ..LogParams::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(text, "plain log\n");
    let query = seen.lock().unwrap()[0].query.clone();
    for part in [
        "container=steps",
        "tailLines=50",
        "previous=true",
        "timestamps=true",
        "sinceSeconds=60",
    ] {
        assert!(query.contains(part), "{query}");
    }
    assert_eq!(
        seen.lock().unwrap()[0].path,
        "/api/v1/namespaces/forge/pods/web/log"
    );

    let stream = api
        .log_stream(
            "web",
            &LogParams {
                follow: true,
                ..LogParams::default()
            },
        )
        .await
        .unwrap();
    let lines: Vec<String> = quench_k8s::lines(stream)
        .map(|l| l.unwrap())
        .collect_vec()
        .await;
    assert_eq!(lines, ["line one", "line two", "last, no break"]);

    assert!(
        api.logs("gone", &LogParams::default())
            .await
            .unwrap_err()
            .is_not_found()
    );
    assert!(api.log_stream("gone", &LogParams::default()).await.is_err());
}

#[quench_test::test]
async fn watches_decode_events() {
    let (base, seen) = server(|_| {
        let body = [
            json!({"type": "ADDED", "object": {"metadata": {"name": "a"}}}),
            json!({"type": "MODIFIED", "object": {"metadata": {"name": "a"}}}),
            json!({"type": "BOOKMARK", "object": {"metadata": {"resourceVersion": "9"}}}),
            json!({"type": "DELETED", "object": {"metadata": {"name": "a"}}}),
            json!({"type": "ERROR", "object": {"kind": "Status", "code": 410, "reason": "Expired", "message": "too old"}}),
        ]
        .iter()
        .fold(String::new(), |mut text, v| {
            text.push_str(&json::to_string(v));
            text.push('\n');
            text
        });
        Response::text(StatusCode::OK, body)
    })
    .await;
    let client = client(&base);
    let events: Vec<WatchEvent> = client
        .namespaced(&ApiResource::pod(), "forge")
        .watch(&ListParams::default().labels("app=web"), "8")
        .await
        .unwrap()
        .map(|e| e.unwrap())
        .collect_vec()
        .await;
    assert!(matches!(&events[0], WatchEvent::Added(o) if o["metadata"]["name"] == "a"));
    assert!(matches!(&events[1], WatchEvent::Modified(_)));
    assert!(
        matches!(&events[2], WatchEvent::Bookmark(o) if o["metadata"]["resourceVersion"] == "9")
    );
    assert!(matches!(&events[3], WatchEvent::Deleted(_)));
    assert!(matches!(&events[4], WatchEvent::Error(s) if s.code == 410 && s.reason == "Expired"));
    let query = seen.lock().unwrap()[0].query.clone();
    for part in [
        "watch=true",
        "resourceVersion=8",
        "allowWatchBookmarks=true",
        "labelSelector=app%3Dweb",
    ] {
        assert!(query.contains(part), "{query}");
    }
}

#[quench_test::test]
async fn bad_watch_lines_are_decode_errors() {
    let (base, _) = server(|_| {
        Response::text(
            StatusCode::OK,
            "not json\n{\"type\":\"WEIRD\",\"object\":{}}\n{\"type\":\"ADDED\"}\n",
        )
    })
    .await;
    let client = client(&base);
    let results: Vec<_> = client
        .api(&ApiResource::pod())
        .watch(&ListParams::default(), "")
        .await
        .unwrap()
        .collect_vec()
        .await;
    assert_eq!(results.len(), 3);
    assert!(
        results
            .iter()
            .all(|r| matches!(r, Err(quench_k8s::Error::Decode(_))))
    );
}

#[quench_test::test]
async fn authentication_is_sent_every_time_and_files_are_reread() {
    let (base, seen) = server(|_| ok(&json!({}))).await;
    let dir = std::env::temp_dir().join(format!("quench-k8s-auth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let token_file = dir.join("token");
    std::fs::write(&token_file, "first\n").unwrap();

    let mut config = Config::new(&base);
    config.auth = Auth::BearerFile(token_file.clone());
    let from_file = Client::new(config.clone()).unwrap();
    from_file.get_json("/version").await.unwrap();
    std::fs::write(&token_file, "rotated").unwrap();
    from_file.get_json("/version").await.unwrap();

    config.auth = Auth::Bearer("static".into());
    Client::new(config.clone())
        .unwrap()
        .get_json("/version")
        .await
        .unwrap();
    config.auth = Auth::Basic {
        username: "u".into(),
        password: "p".into(),
    };
    Client::new(config.clone())
        .unwrap()
        .get_json("/version")
        .await
        .unwrap();
    config.auth = Auth::None;
    Client::new(config)
        .unwrap()
        .get_json("/version")
        .await
        .unwrap();

    let auth: Vec<Option<String>> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.header("authorization").map(str::to_string))
        .collect();
    assert_eq!(
        auth,
        [
            Some("Bearer first".to_string()),
            Some("Bearer rotated".to_string()),
            Some("Bearer static".to_string()),
            Some("Basic dTpw".to_string()),
            None,
        ]
    );

    let mut missing = Config::new(&base);
    missing.auth = Auth::BearerFile(dir.join("nope"));
    let error = Client::new(missing)
        .unwrap()
        .get_json("/version")
        .await
        .unwrap_err();
    assert!(matches!(error, quench_k8s::Error::Config(m) if m.contains("token file")));
    std::fs::remove_dir_all(&dir).ok();
}

#[quench_test::test]
async fn failures_that_are_not_status_objects_still_become_errors() {
    let (base, _) = server(|s| match s.path.as_str() {
        "/plain" => Response::text(StatusCode::INTERNAL_SERVER_ERROR, "boom\n"),
        "/forbidden" => status(403, "Forbidden", "nope"),
        "/garbage" => Response::text(StatusCode::OK, "<html>"),
        _ => ok(&json!({})),
    })
    .await;
    let client = client(&base);
    let plain = client.get_json("/plain").await.unwrap_err();
    let status = plain.status().unwrap();
    assert_eq!(
        (status.code, status.reason.as_str(), status.message.as_str()),
        (500, "", "boom")
    );
    assert!(
        client
            .get_json("/forbidden")
            .await
            .unwrap_err()
            .is_forbidden()
    );
    assert!(matches!(
        client.get_json("/garbage").await.unwrap_err(),
        quench_k8s::Error::Decode(_)
    ));

    let unreachable = Client::new(Config::new("http://127.0.0.1:1")).unwrap();
    assert!(matches!(
        unreachable.get_json("/version").await.unwrap_err(),
        quench_k8s::Error::Transport(_)
    ));
}

#[quench_test::test]
async fn discovery_lists_preferred_versions_and_skips_subresources() {
    let (base, seen) = server(|s| match s.path.as_str() {
        "/apis" => ok(&json!({"groups": [
            {"name": "apps", "preferredVersion": {"version": "v1"}, "versions": [{"version": "v1"}]},
            {"name": "traefik.io", "preferredVersion": {"version": "v1alpha1"}},
            {"name": "broken.example", "preferredVersion": {"version": "v1"}},
            {"name": "no-preferred"},
        ]})),
        "/api/v1" => ok(&json!({"resources": [
            {"name": "pods", "kind": "Pod", "namespaced": true, "verbs": ["get", "list", "delete"]},
            {"name": "pods/log", "kind": "Pod", "namespaced": true, "verbs": ["get"]},
            {"name": "nodes", "kind": "Node", "namespaced": false, "verbs": ["get", "list"]},
            {"kind": "Nameless"},
        ]})),
        "/apis/apps/v1" => ok(&json!({"resources": [
            {"name": "deployments", "kind": "Deployment", "namespaced": true, "verbs": ["list", "create"]},
        ]})),
        "/apis/traefik.io/v1alpha1" => ok(&json!({"resources": [
            {"name": "ingressroutes", "kind": "IngressRoute", "namespaced": true, "verbs": ["list"]},
        ]})),
        _ => status(500, "InternalError", "unavailable"),
    })
    .await;
    let discovery = client(&base).discover().await.unwrap();
    let groups = discovery.groups();
    assert_eq!(
        groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
        ["", "apps", "traefik.io"]
    );
    assert_eq!(
        groups[0].recommended_resources().len(),
        2,
        "subresources and nameless entries are left out"
    );

    let (pod, caps) = discovery.find_kind("Pod").unwrap();
    assert_eq!(
        (pod.group.as_str(), pod.plural.as_str(), pod.namespaced),
        ("", "pods", true)
    );
    assert_eq!(caps.scope, quench_k8s::Scope::Namespaced);
    assert!(caps.supports_operation("delete") && !caps.supports_operation("watch"));
    assert_eq!(
        discovery.find_kind("Node").unwrap().1.scope,
        quench_k8s::Scope::Cluster
    );
    assert!(discovery.find_kind("Nope").is_none());

    let manifest = json!({"apiVersion": "traefik.io/v1alpha1", "kind": "IngressRoute"});
    assert_eq!(
        discovery.find_manifest(&manifest).unwrap().0.plural,
        "ingressroutes"
    );
    assert_eq!(
        discovery
            .find_manifest(&json!({"apiVersion": "v1", "kind": "Pod"}))
            .unwrap()
            .0
            .kind,
        "Pod"
    );
    assert!(
        discovery
            .find_manifest(&json!({"apiVersion": "apps/v2", "kind": "Deployment"}))
            .is_none()
    );
    assert!(discovery.find_manifest(&json!({"kind": "Pod"})).is_none());
    assert!(seen.lock().unwrap().len() >= 5);
}

#[quench_test::test]
async fn discovery_fails_when_the_group_list_does() {
    let (base, _) = server(|_| status(403, "Forbidden", "no discovery for you")).await;
    assert!(client(&base).discover().await.unwrap_err().is_forbidden());
}

quench_test::main!();
