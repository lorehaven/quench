//! Cluster routing against scripted nodes that implement just enough of Redis:
//! `CLUSTER SLOTS` / `CLUSTER NODES`, `GET` / `SET` / `SCAN`, `ASKING`, and the
//! `MOVED` / `ASK` / `CLUSTERDOWN` replies a real cluster gives.

#![allow(clippy::significant_drop_tightening)] // tests hold guards for their whole body on purpose

use quench_redis::resp::{Value, decode};
use quench_redis::{ClusterClient, Error, Redis, cmd, key_slot};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// What every node agrees on.
#[derive(Default)]
struct Cluster {
    /// Node index serving each half: slots below `split` go to node 0, the rest to node 1.
    split: Mutex<u16>,
    ports: Mutex<Vec<u16>>,
    /// Per-node key/value data.
    data: Mutex<Vec<HashMap<String, String>>>,
    /// Slots being migrated from node 0 to node 1: node 0 answers `ASK` for keys it no longer has.
    migrating: Mutex<Vec<u16>>,
    /// Nodes that behave as if they were down.
    down: Mutex<Vec<bool>>,
    /// Answer the next keyed command with `CLUSTERDOWN` once.
    clusterdown_once: AtomicBool,
    /// Always redirect to the node itself (a broken cluster).
    redirect_loop: AtomicBool,
    commands_seen: Vec<AtomicUsize>,
    topology_reads: AtomicUsize,
    asking_seen: AtomicUsize,
}

impl Cluster {
    fn owner(&self, slot: u16) -> usize {
        usize::from(slot >= *self.split.lock().unwrap())
    }

    fn address(&self, node: usize) -> String {
        format!("127.0.0.1:{}", self.ports.lock().unwrap()[node])
    }

    fn slots_reply(&self) -> Value {
        let split = i64::from(*self.split.lock().unwrap());
        let entry = |from: i64, to: i64, node: usize| {
            let port = i64::from(self.ports.lock().unwrap()[node]);
            Value::Array(vec![
                Value::Int(from),
                Value::Int(to),
                Value::Array(vec![
                    Value::Bulk(b"127.0.0.1".to_vec()),
                    Value::Int(port),
                    Value::Bulk(b"id".to_vec()),
                ]),
            ])
        };
        let mut ranges = Vec::new();
        if split > 0 {
            ranges.push(entry(0, split - 1, 0));
        }
        if split <= 16383 {
            ranges.push(entry(split, 16383, 1));
        }
        Value::Array(ranges)
    }
}

fn bulk(text: &str) -> Value {
    Value::Bulk(text.as_bytes().to_vec())
}

fn encode(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Simple(s) => out.extend_from_slice(format!("+{s}\r\n").as_bytes()),
        Value::Int(n) => out.extend_from_slice(format!(":{n}\r\n").as_bytes()),
        Value::Bulk(b) => {
            out.extend_from_slice(format!("${}\r\n", b.len()).as_bytes());
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        Value::Nil => out.extend_from_slice(b"$-1\r\n"),
        Value::Array(items) => {
            out.extend_from_slice(format!("*{}\r\n", items.len()).as_bytes());
            for item in items {
                encode(item, out);
            }
        }
        Value::Error(e) => {
            out.extend_from_slice(format!("-{} {}\r\n", e.kind, e.message).as_bytes());
        }
        other => panic!("the fake server does not send {other:?}"),
    }
}

fn error(text: &str) -> Value {
    Value::Error(quench_redis::ServerError::parse(text))
}

fn handle(cluster: &Cluster, node: usize, asking: &mut bool, args: &[String]) -> Value {
    cluster.commands_seen[node].fetch_add(1, Ordering::SeqCst);
    let name = args[0].to_ascii_uppercase();
    match name.as_str() {
        "PING" => return Value::Simple("PONG".into()),
        "ASKING" => {
            cluster.asking_seen.fetch_add(1, Ordering::SeqCst);
            *asking = true;
            return Value::Simple("OK".into());
        }
        "CLUSTER" if args.get(1).is_some_and(|a| a.eq_ignore_ascii_case("SLOTS")) => {
            cluster.topology_reads.fetch_add(1, Ordering::SeqCst);
            return cluster.slots_reply();
        }
        "CLUSTER" => {
            let line = |n: usize, flags: &str| {
                let port = cluster.ports.lock().unwrap()[n];
                format!("id{n} 127.0.0.1:{port}@1{port} {flags} - 0 0 {n} connected\n")
            };
            return bulk(
                &[
                    line(0, "myself,master"),
                    line(1, "master"),
                    "idr 127.0.0.1:9@19 slave id0 0 0 0 connected\n".to_string(),
                    line(1, "master,fail"),
                ]
                .concat(),
            );
        }
        "SCAN" => {
            let pattern = args.get(3).cloned().unwrap_or_default();
            let prefix = pattern.trim_end_matches('*').to_string();
            let keys: Vec<Value> = cluster.data.lock().unwrap()[node]
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .map(|k| bulk(k))
                .collect();
            return Value::Array(vec![bulk("0"), Value::Array(keys)]);
        }
        _ => {}
    }
    let key = &args[1];
    let slot = key_slot(key.as_bytes());
    if cluster.clusterdown_once.swap(false, Ordering::SeqCst) {
        return error("CLUSTERDOWN The cluster is down");
    }
    if cluster.redirect_loop.load(Ordering::SeqCst) {
        return error(&format!("MOVED {slot} {}", cluster.address(node)));
    }
    let was_asking = std::mem::take(asking);
    let owner = cluster.owner(slot);
    let migrating = cluster.migrating.lock().unwrap().contains(&slot);
    if migrating {
        // Node 0 still owns the slot on paper but has handed the key over to node 1.
        if node == 0 && !cluster.data.lock().unwrap()[0].contains_key(key) {
            return error(&format!("ASK {slot} {}", cluster.address(1)));
        }
        if node == 1 && !was_asking {
            return error(&format!("MOVED {slot} {}", cluster.address(0)));
        }
    } else if owner != node {
        return error(&format!("MOVED {slot} {}", cluster.address(owner)));
    }
    let mut data = cluster.data.lock().unwrap();
    match name.as_str() {
        "SET" => {
            data[node].insert(key.clone(), args[2].clone());
            Value::Simple("OK".into())
        }
        "GET" => data[node].get(key).map_or(Value::Nil, |v| bulk(v)),
        "WRONGTYPE" => error("WRONGTYPE bad"),
        _ => error("ERR unknown command"),
    }
}

async fn serve_node(cluster: Arc<Cluster>, node: usize, listener: TcpListener) {
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        if cluster.down.lock().unwrap()[node] {
            continue; // accepted and dropped: the client sees the connection die
        }
        let cluster = cluster.clone();
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut asking = false;
            loop {
                let mut chunk = [0u8; 4096];
                let n = stream.read(&mut chunk).await.unwrap_or(0);
                if n == 0 || cluster.down.lock().unwrap()[node] {
                    return;
                }
                buf.extend_from_slice(&chunk[..n]);
                let mut out = Vec::new();
                while let Some((Value::Array(items), used)) = decode(&buf).unwrap() {
                    let args: Vec<String> = items
                        .iter()
                        .map(|v| v.as_str().unwrap().to_string())
                        .collect();
                    buf.drain(..used);
                    encode(&handle(&cluster, node, &mut asking, &args), &mut out);
                }
                if !out.is_empty() {
                    stream.write_all(&out).await.unwrap();
                }
            }
        });
    }
}

async fn start() -> (Arc<Cluster>, Vec<String>) {
    let listeners = [
        TcpListener::bind("127.0.0.1:0").await.unwrap(),
        TcpListener::bind("127.0.0.1:0").await.unwrap(),
    ];
    let cluster = Arc::new(Cluster {
        split: Mutex::new(8192),
        ports: Mutex::new(
            listeners
                .iter()
                .map(|l| l.local_addr().unwrap().port())
                .collect(),
        ),
        data: Mutex::new(vec![HashMap::new(), HashMap::new()]),
        down: Mutex::new(vec![false, false]),
        commands_seen: vec![AtomicUsize::new(0), AtomicUsize::new(0)],
        ..Cluster::default()
    });
    let urls = listeners
        .iter()
        .map(|l| format!("redis://127.0.0.1:{}", l.local_addr().unwrap().port()))
        .collect();
    for (node, listener) in listeners.into_iter().enumerate() {
        tokio::spawn(serve_node(cluster.clone(), node, listener));
    }
    (cluster, urls)
}

/// A key whose slot is on the low half (node 0) or the high half (node 1) of the initial split.
fn key_on(low: bool, tag: &str) -> String {
    (0..u32::MAX)
        .map(|i| format!("{tag}-{i}"))
        .find(|k| (key_slot(k.as_bytes()) < 8192) == low)
        .unwrap()
}

#[tokio::test]
async fn commands_are_routed_by_slot() {
    let (cluster, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[0]]).await.unwrap();
    assert!(redis.is_cluster());
    let (low, high) = (key_on(true, "a"), key_on(false, "b"));
    redis
        .exec::<()>(&cmd("SET").arg(&low).arg("one"))
        .await
        .unwrap();
    redis
        .exec::<()>(&cmd("SET").arg(&high).arg("two"))
        .await
        .unwrap();
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&low)).await.unwrap(),
        "one"
    );
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&high)).await.unwrap(),
        "two"
    );
    assert_eq!(
        redis
            .exec::<Option<String>>(&cmd("GET").arg(&key_on(true, "absent")))
            .await
            .unwrap(),
        None
    );
    let data = cluster.data.lock().unwrap();
    assert!(data[0].contains_key(&low) && !data[0].contains_key(&high));
    assert!(data[1].contains_key(&high) && !data[1].contains_key(&low));
    assert_eq!(
        cluster.topology_reads.load(Ordering::SeqCst),
        1,
        "no redirects, so the topology was read once"
    );
}

#[tokio::test]
async fn hash_tags_keep_related_keys_together() {
    let (cluster, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[1]]).await.unwrap();
    for i in 0..10 {
        redis
            .exec::<()>(&cmd("SET").arg(&format!("{{session42}}:{i}")).arg("v"))
            .await
            .unwrap();
    }
    let data = cluster.data.lock().unwrap();
    assert!(
        data[0].len() == 10 || data[1].len() == 10,
        "all ten on one node"
    );
}

#[tokio::test]
async fn moved_replies_update_the_routing_table() {
    let (cluster, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[0]]).await.unwrap();
    let key = key_on(false, "moving");
    redis
        .exec::<()>(&cmd("SET").arg(&key).arg("before"))
        .await
        .unwrap();
    // The slot (indeed everything) moves to node 0 behind the client's back.
    *cluster.split.lock().unwrap() = 16384;
    cluster.data.lock().unwrap()[0].insert(key.clone(), "after".into());
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&key)).await.unwrap(),
        "after"
    );
    let seen = cluster.commands_seen[1].load(Ordering::SeqCst);
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&key)).await.unwrap(),
        "after"
    );
    assert_eq!(
        cluster.commands_seen[1].load(Ordering::SeqCst),
        seen,
        "the second request went straight to the new owner"
    );
}

#[tokio::test]
async fn ask_redirects_send_asking_first() {
    let (cluster, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[0]]).await.unwrap();
    let key = key_on(true, "migrating");
    let slot = key_slot(key.as_bytes());
    cluster.migrating.lock().unwrap().push(slot);
    cluster.data.lock().unwrap()[1].insert(key.clone(), "on the target".into());
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&key)).await.unwrap(),
        "on the target"
    );
    assert_eq!(cluster.asking_seen.load(Ordering::SeqCst), 1);
    // ASK does not change the routing table: the next request still starts at node 0.
    let before = cluster.commands_seen[0].load(Ordering::SeqCst);
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&key)).await.unwrap(),
        "on the target"
    );
    assert!(cluster.commands_seen[0].load(Ordering::SeqCst) > before);
}

#[tokio::test]
async fn endless_redirects_give_up() {
    let (cluster, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[0]]).await.unwrap();
    cluster.redirect_loop.store(true, Ordering::SeqCst);
    let err = redis
        .exec::<String>(&cmd("GET").arg(&key_on(true, "loop")))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Cluster(m) if m.contains("redirected")),
        "{err}"
    );
}

#[tokio::test]
async fn clusterdown_triggers_a_topology_refresh_and_a_retry() {
    let (cluster, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[0]]).await.unwrap();
    cluster.clusterdown_once.store(true, Ordering::SeqCst);
    redis
        .exec::<()>(&cmd("SET").arg(&key_on(true, "down")).arg("v"))
        .await
        .unwrap();
    assert_eq!(cluster.topology_reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn other_errors_pass_through() {
    let (_, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[0]]).await.unwrap();
    let err = redis
        .exec::<()>(&cmd("WRONGTYPE").arg(&key_on(true, "e")))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Server(s) if s.kind == "WRONGTYPE"),
        "{err}"
    );
}

#[tokio::test]
async fn a_dead_node_makes_the_client_relearn_the_topology() {
    let (cluster, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[0], &urls[1]]).await.unwrap();
    let key = key_on(false, "failover");
    redis
        .exec::<()>(&cmd("SET").arg(&key).arg("v1"))
        .await
        .unwrap();
    // Node 1 dies; node 0 takes over the whole keyspace.
    cluster.down.lock().unwrap()[1] = true;
    *cluster.split.lock().unwrap() = 16384;
    cluster.data.lock().unwrap()[0].insert(key.clone(), "survivor".into());
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&key)).await.unwrap(),
        "survivor"
    );
}

#[tokio::test]
async fn per_node_commands_and_primaries() {
    let (cluster, urls) = start().await;
    let redis = Redis::connect_cluster(&[&urls[0]]).await.unwrap();
    let (low, high) = (key_on(true, "scan-a"), key_on(false, "scan-b"));
    redis
        .exec::<()>(&cmd("SET").arg(&low).arg("1"))
        .await
        .unwrap();
    redis
        .exec::<()>(&cmd("SET").arg(&high).arg("2"))
        .await
        .unwrap();
    let primaries = redis.primaries().await.unwrap();
    assert_eq!(
        primaries,
        [cluster.address(0), cluster.address(1)],
        "healthy primaries only, replicas and failed nodes skipped"
    );
    let mut found = Vec::new();
    for node in &primaries {
        let (_, keys): (u64, Vec<String>) = redis
            .exec_on(
                node,
                &cmd("SCAN")
                    .arg(&0)
                    .arg("MATCH")
                    .arg("scan-*")
                    .arg("COUNT")
                    .arg(&100),
            )
            .await
            .unwrap();
        found.extend(keys);
    }
    found.sort();
    let mut expected = vec![low, high];
    expected.sort();
    assert_eq!(found, expected);
}

#[tokio::test]
async fn connecting_needs_a_reachable_seed() {
    assert!(matches!(
        ClusterClient::connect(&[]).await,
        Err(Error::Config(_))
    ));
    assert!(matches!(
        ClusterClient::connect(&["not a url"]).await,
        Err(Error::Config(_))
    ));
    let err = ClusterClient::connect(&["redis://127.0.0.1:1"])
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Cluster(m) if m.contains("topology")),
        "{err}"
    );
    let (_, urls) = start().await;
    let cluster = ClusterClient::connect(&[&urls[0]]).await.unwrap();
    assert_eq!(cluster.nodes().len(), 2);
    assert!(format!("{cluster:?}").contains("ClusterClient"));
}
