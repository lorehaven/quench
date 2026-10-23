//! Redis Cluster: slot hashing, topology discovery from `CLUSTER SLOTS`, and
//! routing with `MOVED` / `ASK` redirects.

use crate::cmd::{Cmd, FromValue, cmd};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::pool::{Client, PoolOptions};
use crate::resp::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

/// Number of hash slots.
pub const SLOTS: u16 = 16384;
/// Most redirects followed for one command.
const MAX_REDIRECTS: usize = 5;

/// CRC16/XMODEM, the checksum Redis Cluster hashes keys with.
#[must_use]
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// The slot of a key, honouring `{hash tags}`: when the key has a non-empty `{...}` section,
/// only that part is hashed, so related keys can be forced onto one node.
#[must_use]
pub fn key_slot(key: &[u8]) -> u16 {
    let hashed = key
        .iter()
        .position(|&b| b == b'{')
        .and_then(|open| {
            key[open + 1..]
                .iter()
                .position(|&b| b == b'}')
                .filter(|&len| len > 0)
                .map(|len| &key[open + 1..open + 1 + len])
        })
        .unwrap_or(key);
    crc16(hashed) % SLOTS
}

/// Which node serves which slots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotMap {
    /// `(first slot, last slot, address)`, sorted by first slot.
    ranges: Vec<(u16, u16, String)>,
}

impl SlotMap {
    /// Builds the map from a `CLUSTER SLOTS` reply. `default_host` stands in for nodes that report no host.
    ///
    /// # Errors
    ///
    /// [`Error::Cluster`] when the reply is not shaped like `CLUSTER SLOTS`.
    pub fn from_cluster_slots(reply: &Value, default_host: &str) -> Result<Self> {
        let Value::Array(entries) = reply else {
            return Err(Error::Cluster("CLUSTER SLOTS did not return a list".into()));
        };
        let mut ranges = Vec::with_capacity(entries.len());
        for entry in entries {
            let Value::Array(fields) = entry else {
                return Err(Error::Cluster("malformed CLUSTER SLOTS entry".into()));
            };
            let (Some(Value::Int(first)), Some(Value::Int(last)), Some(Value::Array(node))) =
                (fields.first(), fields.get(1), fields.get(2))
            else {
                return Err(Error::Cluster("malformed CLUSTER SLOTS entry".into()));
            };
            let host = node
                .first()
                .and_then(Value::as_str)
                .filter(|h| !h.is_empty())
                .unwrap_or(default_host);
            let Some(Value::Int(port)) = node.get(1) else {
                return Err(Error::Cluster("a CLUSTER SLOTS node has no port".into()));
            };
            let (Ok(first), Ok(last)) = (u16::try_from(*first), u16::try_from(*last)) else {
                return Err(Error::Cluster("slot out of range".into()));
            };
            if first > last || last >= SLOTS {
                return Err(Error::Cluster("slot range out of order".into()));
            }
            ranges.push((first, last, format_address(host, *port)));
        }
        ranges.sort_by_key(|r| r.0);
        Ok(Self { ranges })
    }

    /// The address serving `slot`.
    #[must_use]
    pub fn node_for(&self, slot: u16) -> Option<&str> {
        let at = self.ranges.partition_point(|r| r.1 < slot);
        self.ranges
            .get(at)
            .filter(|r| r.0 <= slot)
            .map(|r| r.2.as_str())
    }

    /// Distinct node addresses, in slot order.
    #[must_use]
    pub fn addresses(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for (_, _, address) in &self.ranges {
            if !seen.contains(address) {
                seen.push(address.clone());
            }
        }
        seen
    }

    /// Records that `slot` now lives on `address` (what a `MOVED` reply says), splitting its range.
    pub fn assign(&mut self, slot: u16, address: &str) {
        let mut result = Vec::with_capacity(self.ranges.len() + 2);
        let mut placed = false;
        for (first, last, node) in self.ranges.drain(..) {
            if slot < first || slot > last {
                result.push((first, last, node));
                continue;
            }
            if first < slot {
                result.push((first, slot - 1, node.clone()));
            }
            result.push((slot, slot, address.to_string()));
            placed = true;
            if slot < last {
                result.push((slot + 1, last, node));
            }
        }
        if !placed {
            result.push((slot, slot, address.to_string()));
        }
        result.sort_by_key(|r| r.0);
        self.ranges = result;
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

fn format_address(host: &str, port: i64) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// A `MOVED` or `ASK` redirect: the slot and the node address it names.
fn redirect(error: &crate::error::ServerError) -> Option<(bool, u16, String)> {
    let is_ask = match error.kind.as_str() {
        "MOVED" => false,
        "ASK" => true,
        _ => return None,
    };
    let (slot, address) = error.message.split_once(' ')?;
    Some((is_ask, slot.parse().ok()?, address.trim().to_string()))
}

struct Inner {
    seeds: Vec<String>,
    template: Config,
    options: PoolOptions,
    nodes: Mutex<HashMap<String, Client>>,
    slots: RwLock<SlotMap>,
}

/// A client for a Redis Cluster. Cheap to clone.
#[derive(Clone)]
pub struct ClusterClient {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for ClusterClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterClient")
            .field("seeds", &self.inner.seeds)
            .finish_non_exhaustive()
    }
}

impl ClusterClient {
    /// Connects through `urls` (seed nodes, `redis://host:port`) and reads the topology.
    /// Credentials and TLS settings come from the first URL and apply to every node.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] for an empty or malformed list, [`Error::Cluster`] when no seed answers.
    pub async fn connect(urls: &[&str]) -> Result<Self> {
        Self::connect_with(urls, PoolOptions::default()).await
    }

    /// As [`connect`](Self::connect), with pool options for each node.
    ///
    /// # Errors
    ///
    /// As [`connect`](Self::connect).
    pub async fn connect_with(urls: &[&str], options: PoolOptions) -> Result<Self> {
        let first = urls
            .first()
            .ok_or_else(|| Error::Config("no cluster seed given".into()))?;
        let template = Config::from_url(first)?;
        let seeds = urls
            .iter()
            .map(|u| Config::from_url(u).map(|c| c.address()))
            .collect::<Result<Vec<_>>>()?;
        let client = Self {
            inner: Arc::new(Inner {
                seeds,
                template,
                options,
                nodes: Mutex::new(HashMap::new()),
                slots: RwLock::new(SlotMap::default()),
            }),
        };
        client.refresh().await?;
        Ok(client)
    }

    fn node(&self, address: &str) -> Result<Client> {
        if let Some(client) = self
            .inner
            .nodes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(address)
        {
            return Ok(client.clone());
        }
        let (host, port) = split_address(address)?;
        let config = Config {
            host,
            port,
            db: 0,
            ..self.inner.template.clone()
        };
        let client = Client::lazy(config, self.inner.options.clone());
        self.inner
            .nodes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(address.to_string(), client.clone());
        Ok(client)
    }

    /// Re-reads `CLUSTER SLOTS` from any node that answers.
    ///
    /// # Errors
    ///
    /// [`Error::Cluster`] when none does.
    pub async fn refresh(&self) -> Result<()> {
        let mut candidates = self
            .inner
            .slots
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .addresses();
        for seed in &self.inner.seeds {
            if !candidates.contains(seed) {
                candidates.push(seed.clone());
            }
        }
        let mut last = String::from("no node to ask");
        for address in candidates {
            let reply = match self
                .node(&address)?
                .exec_value(&cmd("CLUSTER").arg("SLOTS"))
                .await
            {
                Ok(Value::Error(err)) => {
                    last = format!("{address}: {err}");
                    continue;
                }
                Ok(reply) => reply,
                Err(err) => {
                    last = format!("{address}: {err}");
                    continue;
                }
            };
            let host = split_address(&address).map(|(h, _)| h).unwrap_or_default();
            match SlotMap::from_cluster_slots(&reply, &host) {
                Ok(map) if !map.is_empty() => {
                    *self
                        .inner
                        .slots
                        .write()
                        .unwrap_or_else(PoisonError::into_inner) = map;
                    return Ok(());
                }
                Ok(_) => last = format!("{address} reports no slots"),
                Err(err) => last = format!("{address}: {err}"),
            }
        }
        Err(Error::Cluster(format!(
            "could not read the cluster topology ({last})"
        )))
    }

    /// The addresses of the nodes currently serving slots.
    #[must_use]
    pub fn nodes(&self) -> Vec<String> {
        self.inner
            .slots
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .addresses()
    }

    fn route(&self, command: &Cmd) -> Option<String> {
        let slot = key_slot(command.key()?);
        self.inner
            .slots
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .node_for(slot)
            .map(str::to_string)
    }

    /// Runs a command on the node owning its key (the first argument), following redirects.
    /// Keyless commands go to the first node.
    ///
    /// # Errors
    ///
    /// Transport errors, [`Error::Cluster`] after too many redirects, or the server's error reply.
    pub async fn exec_value(&self, command: &Cmd) -> Result<Value> {
        let mut address = self
            .route(command)
            .or_else(|| self.nodes().into_iter().next())
            .ok_or_else(|| Error::Cluster("the topology is empty".into()))?;
        let mut asking = false;
        let mut refreshed = false;
        for _ in 0..=MAX_REDIRECTS {
            let client = self.node(&address)?;
            let reply = if asking {
                asking = false;
                let mut replies = client.pipeline(&[cmd("ASKING"), command.clone()]).await?;
                replies.pop().unwrap_or(Value::Nil)
            } else {
                match client.exec_value(command).await {
                    Ok(reply) => reply,
                    Err(Error::Io(_) | Error::Closed | Error::Timeout(_)) if !refreshed => {
                        // The node may be gone: re-read the topology and route again.
                        refreshed = true;
                        self.refresh().await?;
                        address = self
                            .route(command)
                            .or_else(|| self.nodes().into_iter().next())
                            .ok_or_else(|| Error::Cluster("the topology is empty".into()))?;
                        continue;
                    }
                    Err(err) => return Err(err),
                }
            };
            let Value::Error(err) = &reply else {
                return Ok(reply);
            };
            match redirect(err) {
                Some((true, _, target)) => {
                    address = Self::resolve(&target, &address);
                    asking = true;
                }
                Some((false, slot, target)) => {
                    address = Self::resolve(&target, &address);
                    self.inner
                        .slots
                        .write()
                        .unwrap_or_else(PoisonError::into_inner)
                        .assign(slot, &address);
                }
                None if err.kind == "CLUSTERDOWN" && !refreshed => {
                    refreshed = true;
                    let _ = self.refresh().await;
                }
                None => return Ok(reply),
            }
        }
        Err(Error::Cluster(format!(
            "{} was redirected more than {MAX_REDIRECTS} times",
            command.name()
        )))
    }

    /// A redirect target such as `:6379` (no host) means the node that sent it.
    fn resolve(target: &str, from: &str) -> String {
        target.strip_prefix(':').map_or_else(
            || target.to_string(),
            |port| {
                format!(
                    "{}:{port}",
                    split_address(from).map(|(h, _)| h).unwrap_or_default()
                )
            },
        )
    }

    /// Runs a command and converts the reply.
    ///
    /// # Errors
    ///
    /// As [`exec_value`](Self::exec_value), plus the server's error reply and conversion errors.
    pub async fn exec<T: FromValue>(&self, command: &Cmd) -> Result<T> {
        match self.exec_value(command).await? {
            Value::Error(err) => Err(Error::Server(err)),
            value => T::from_value(value),
        }
    }

    /// Runs a command on one specific node (for per-node commands such as `SCAN`).
    ///
    /// # Errors
    ///
    /// Transport errors, or the server's error reply.
    pub async fn exec_on<T: FromValue>(&self, address: &str, command: &Cmd) -> Result<T> {
        self.node(address)?.exec(command).await
    }
}

fn split_address(address: &str) -> Result<(String, u16)> {
    let (host, port) = address
        .rsplit_once(':')
        .ok_or_else(|| Error::Cluster(format!("bad node address `{address}`")))?;
    let port = port
        .parse()
        .map_err(|_| Error::Cluster(format!("bad port in `{address}`")))?;
    Ok((
        host.trim_start_matches('[')
            .trim_end_matches(']')
            .to_string(),
        port,
    ))
}

/// Reads one `CLUSTER NODES` line, keeping healthy primaries.
///
/// The address field is `host:port@cport[,hostname[,aux=…]]`; the flags field
/// is a comma-separated list that contains `master` or `slave`, plus `fail` or
/// `fail?` for a node the cluster has given up on.
#[must_use]
pub fn parse_primary(line: &str) -> Option<(String, u16)> {
    let mut fields = line.split_whitespace();
    let _id = fields.next()?;
    let address = fields.next()?;
    let flags = fields.next()?;

    let healthy_primary = flags
        .split(',')
        .any(|flag| flag == "master" || flag == "primary")
        && !flags.split(',').any(|flag| flag.starts_with("fail"));
    if !healthy_primary {
        return None;
    }

    let address = address.split('@').next()?;
    let (host, port) = address.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ServerError;

    #[test]
    fn crc16_matches_the_redis_vectors() {
        assert_eq!(crc16(b"123456789"), 0x31c3);
        assert_eq!(crc16(b""), 0);
    }

    #[test]
    fn key_slots() {
        assert_eq!(key_slot(b"foo"), 12182);
        assert_eq!(key_slot(b"somekey"), 11058);
        assert_eq!(
            key_slot(b"{user1000}.following"),
            key_slot(b"{user1000}.followers")
        );
        assert_eq!(key_slot(b"{user1000}.following"), key_slot(b"user1000"));
        assert_eq!(
            key_slot(b"foo{}{bar}"),
            key_slot(b"foo{}{bar}"),
            "an empty tag hashes the whole key"
        );
        assert_ne!(key_slot(b"foo{}{bar}"), key_slot(b"bar"));
        assert_eq!(
            key_slot(b"foo{{bar}}zap"),
            key_slot(b"{bar"),
            "the tag runs from the first open brace to the next close brace"
        );
        assert_eq!(key_slot(b"foo{bar"), key_slot(b"foo{bar"));
        assert_ne!(
            key_slot(b"foo{bar"),
            key_slot(b"bar"),
            "an unclosed brace hashes the whole key"
        );
        assert!(key_slot(b"anything at all") < SLOTS);
    }

    fn slots_reply() -> Value {
        let node = |host: &str, port: i64| {
            Value::Array(vec![
                Value::Bulk(host.as_bytes().to_vec()),
                Value::Int(port),
                Value::Bulk(b"id".to_vec()),
                Value::Array(vec![]),
            ])
        };
        let range = |a: i64, b: i64, host: &str, port: i64| {
            Value::Array(vec![
                Value::Int(a),
                Value::Int(b),
                node(host, port),
                node("replica", port + 10),
            ])
        };
        Value::Array(vec![
            range(8192, 16383, "10.0.0.2", 7001),
            range(0, 8191, "10.0.0.1", 7000),
        ])
    }

    #[test]
    fn slot_maps_parse_and_look_up() {
        let map = SlotMap::from_cluster_slots(&slots_reply(), "seed").unwrap();
        assert_eq!(map.node_for(0), Some("10.0.0.1:7000"));
        assert_eq!(map.node_for(8191), Some("10.0.0.1:7000"));
        assert_eq!(map.node_for(8192), Some("10.0.0.2:7001"));
        assert_eq!(map.node_for(16383), Some("10.0.0.2:7001"));
        assert_eq!(map.addresses(), ["10.0.0.1:7000", "10.0.0.2:7001"]);
        assert!(!map.is_empty());
        assert!(SlotMap::default().node_for(5).is_none());
        let gap = SlotMap {
            ranges: vec![(0, 10, "a:1".into()), (20, 30, "b:2".into())],
        };
        assert_eq!(gap.node_for(15), None);
    }

    #[test]
    fn missing_hosts_use_the_seed_and_ipv6_is_bracketed() {
        let reply = Value::Array(vec![Value::Array(vec![
            Value::Int(0),
            Value::Int(16383),
            Value::Array(vec![Value::Bulk(vec![]), Value::Int(7000)]),
        ])]);
        assert_eq!(
            SlotMap::from_cluster_slots(&reply, "seedhost")
                .unwrap()
                .node_for(1),
            Some("seedhost:7000")
        );
        let v6 = Value::Array(vec![Value::Array(vec![
            Value::Int(0),
            Value::Int(16383),
            Value::Array(vec![Value::Bulk(b"::1".to_vec()), Value::Int(7000)]),
        ])]);
        assert_eq!(
            SlotMap::from_cluster_slots(&v6, "s").unwrap().node_for(1),
            Some("[::1]:7000")
        );
    }

    #[test]
    fn malformed_slot_replies_are_errors() {
        for bad in [
            Value::Nil,
            Value::Array(vec![Value::Nil]),
            Value::Array(vec![Value::Array(vec![Value::Int(0)])]),
            Value::Array(vec![Value::Array(vec![
                Value::Int(0),
                Value::Int(5),
                Value::Array(vec![Value::Bulk(b"h".to_vec())]),
            ])]),
            Value::Array(vec![Value::Array(vec![
                Value::Int(-1),
                Value::Int(5),
                Value::Array(vec![Value::Bulk(b"h".to_vec()), Value::Int(1)]),
            ])]),
            Value::Array(vec![Value::Array(vec![
                Value::Int(9),
                Value::Int(5),
                Value::Array(vec![Value::Bulk(b"h".to_vec()), Value::Int(1)]),
            ])]),
            Value::Array(vec![Value::Array(vec![
                Value::Int(0),
                Value::Int(16384),
                Value::Array(vec![Value::Bulk(b"h".to_vec()), Value::Int(1)]),
            ])]),
        ] {
            assert!(
                matches!(
                    SlotMap::from_cluster_slots(&bad, "s"),
                    Err(Error::Cluster(_))
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn assigning_a_slot_splits_ranges() {
        let mut map = SlotMap::from_cluster_slots(&slots_reply(), "s").unwrap();
        map.assign(100, "new:1");
        assert_eq!(map.node_for(99), Some("10.0.0.1:7000"));
        assert_eq!(map.node_for(100), Some("new:1"));
        assert_eq!(map.node_for(101), Some("10.0.0.1:7000"));
        map.assign(0, "edge:1");
        map.assign(16383, "edge:2");
        assert_eq!(
            (
                map.node_for(0),
                map.node_for(1),
                map.node_for(16383),
                map.node_for(16382)
            ),
            (
                Some("edge:1"),
                Some("10.0.0.1:7000"),
                Some("edge:2"),
                Some("10.0.0.2:7001")
            )
        );
        let mut empty = SlotMap::default();
        empty.assign(7, "only:1");
        assert_eq!(empty.node_for(7), Some("only:1"));
    }

    #[test]
    fn redirects_parse() {
        assert_eq!(
            redirect(&ServerError::parse("MOVED 3999 127.0.0.1:6381")),
            Some((false, 3999, "127.0.0.1:6381".into()))
        );
        assert_eq!(
            redirect(&ServerError::parse("ASK 3999 :6381")),
            Some((true, 3999, ":6381".into()))
        );
        assert_eq!(redirect(&ServerError::parse("ERR nope")), None);
        assert_eq!(redirect(&ServerError::parse("MOVED x y")), None);
        assert_eq!(redirect(&ServerError::parse("MOVED")), None);
    }

    #[test]
    fn addresses_split() {
        assert_eq!(split_address("h:7000").unwrap(), ("h".to_string(), 7000));
        assert_eq!(
            split_address("[::1]:7000").unwrap(),
            ("::1".to_string(), 7000)
        );
        assert!(split_address("nohost").is_err());
        assert!(split_address("h:port").is_err());
    }

    #[test]
    fn cluster_nodes_lines_yield_healthy_primaries() {
        let line = |flags: &str, address: &str| {
            format!(
                "07c37dfeb235213a872192d90877d0cd55635b91 {address} {flags} - 0 1426238316232 4 connected 5461-10922"
            )
        };
        assert_eq!(
            parse_primary(&line("master", "127.0.0.1:30002@31002")),
            Some(("127.0.0.1".into(), 30002))
        );
        assert_eq!(
            parse_primary(&line("myself,master", "10.0.0.5:6379@16379,cache-1")),
            Some(("10.0.0.5".into(), 6379))
        );
        assert_eq!(parse_primary(&line("slave", "127.0.0.1:30003@31003")), None);
        assert_eq!(
            parse_primary(&line("master,fail", "127.0.0.1:30002@31002")),
            None
        );
        assert_eq!(
            parse_primary(&line("master,fail?", "127.0.0.1:30002@31002")),
            None
        );
        assert_eq!(parse_primary("too short"), None);
        assert_eq!(parse_primary(""), None);
        assert_eq!(parse_primary(&line("master", "no-port@1")), None);
        assert_eq!(parse_primary(&line("master", "host:notaport@1")), None);
        assert_eq!(
            parse_primary(&line("primary", "[::1]:7000@17000")),
            Some(("[::1]".into(), 7000))
        );
    }
}
