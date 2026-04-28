/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, OnceLock,
    },
};

use sled_overlay::sled;
use smol::{channel, future, Executor};
use url::Url;

use crate::{
    error::Result,
    event_graph::{proto::ProtocolEventGraph, Event, EventGraph, EventGraphConfig, EventGraphPtr},
    net::{session::SESSION_DEFAULT, settings::NetworkProfile, P2p, Settings},
};

pub fn test_config() -> EventGraphConfig {
    EventGraphConfig {
        initial_genesis: 1_704_067_200_000, // 2024-01-01 UTC
        hours_rotation: 0,
        genesis_contents: b"darkfi-test-graph".to_vec(),
        max_dags: Some(24),
    }
}

/// Bounded-mode config for tests that exercise [`DagStore`]
/// directly (without constructing an [`EventGraph`]).
///
/// Uses `hours_rotation = 1` so `DagStore::new` populates the
/// 24-slot rotation ring (vs the single-slot path under
/// `hours_rotation = 0`). Safe because no `EventGraph` is built,
/// so there's no prune task to leak.
pub fn bounded_dag_store_config() -> EventGraphConfig {
    EventGraphConfig { hours_rotation: 1, ..test_config() }
}

/// Archive-mode config: never evicts old DAGs and discovers
/// existing trees from sled on construction. Like
/// [`bounded_dag_store_config`] this is for `DagStore`-direct
/// tests only.
pub fn archive_config() -> EventGraphConfig {
    EventGraphConfig { max_dags: None, ..bounded_dag_store_config() }
}

/// Initialise tracing-subscriber once per process. Safe to call
/// multiple times. Tests that want to see log output can call this
/// at the top of their body.
pub fn init_logger() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();
    });
}

/// Process-wide [`ZkKeys`].
fn shared_zk_keys() -> Arc<crate::event_graph::rln::ZkKeys> {
    use crate::event_graph::rln::{
        ZkKeys, RLN2_REGISTER_ZKBIN, RLN2_SIGNAL_ZKBIN, RLN2_SLASH_ZKBIN,
    };

    static SHARED: OnceLock<Arc<ZkKeys>> = OnceLock::new();
    SHARED
        .get_or_init(|| {
            // Hash the three .zk.bin blobs to derive a stable per-version
            // cache directory.
            let mut hasher = blake3::Hasher::new();
            hasher.update(RLN2_REGISTER_ZKBIN);
            hasher.update(RLN2_SIGNAL_ZKBIN);
            hasher.update(RLN2_SLASH_ZKBIN);
            let zkbin_hash = hasher.finalize().to_hex();
            let cache_dir =
                std::env::temp_dir().join(format!("darkfi-test-zk-cache-{}", &zkbin_hash[..16]));

            let db = sled::Config::new().path(&cache_dir).open().unwrap_or_else(|e| {
                panic!(
                    "failed to open shared ZK key sled DB at {}: {e}\n\
                         (if the cache is corrupted, run `rm -rf {}`)",
                    cache_dir.display(),
                    cache_dir.display(),
                )
            });
            let keys = ZkKeys::build_and_load(&db).expect("failed to build shared ZK keys");
            Arc::new(keys)
        })
        .clone()
}

pub async fn make_eg() -> EventGraphPtr {
    let ex = Arc::new(Executor::new());
    let p2p = P2p::new(Settings::default(), ex.clone()).await.unwrap();
    let sled_db = sled::Config::new().temporary(true).open().unwrap();
    EventGraph::with_zk_keys(
        p2p,
        sled_db,
        "/tmp".into(),
        false,
        test_config(),
        shared_zk_keys(),
        ex,
    )
    .await
    .unwrap()
}

/// Number of nodes a `make_network` call brings up.
pub const N_NODES: usize = 5;

/// Outbound peer count per node.
pub const N_CONNS: usize = 2;

/// Allocate a fresh non-overlapping TCP port range for one
/// `make_network` call. Process-wide counter so parallel tests
/// never collide.
fn alloc_port_base() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(13_400);
    NEXT.fetch_add(N_NODES as u16, Ordering::SeqCst)
}

/// Spawn one `EventGraph` node on a local port, peered with the
/// given `peer_offsets` (relative to `port_base`).
async fn spawn_node(
    port_base: u16,
    port_offset: usize,
    peer_offsets: Vec<usize>,
    ex: Arc<Executor<'static>>,
) -> EventGraphPtr {
    let mut profiles = HashMap::new();
    profiles.insert(
        "tcp".to_string(),
        NetworkProfile { outbound_connect_timeout: 2, ..Default::default() },
    );
    let inbound =
        vec![Url::parse(&format!("tcp://127.0.0.1:{}", port_base + port_offset as u16)).unwrap()];
    let peers: Vec<_> = peer_offsets
        .iter()
        .map(|p| Url::parse(&format!("tcp://127.0.0.1:{}", port_base + *p as u16)).unwrap())
        .collect();

    let settings = Settings {
        localnet: true,
        inbound_addrs: inbound,
        outbound_connections: 0,
        inbound_connections: usize::MAX,
        peers,
        active_profiles: vec!["tcp".to_string()],
        profiles,
        ..Default::default()
    };

    let p2p = P2p::new(settings, ex.clone()).await.unwrap();
    let sled_db = sled::Config::new().temporary(true).open().unwrap();
    let eg = EventGraph::with_zk_keys(
        p2p.clone(),
        sled_db,
        "/tmp".into(),
        false,
        test_config(),
        shared_zk_keys(),
        ex.clone(),
    )
    .await
    .unwrap();

    // Mark synced so protocol handlers accept events during tests.
    eg.synced.store(true, Ordering::Release);

    let eg_weak = Arc::downgrade(&eg);
    p2p.protocol_registry()
        .register(SESSION_DEFAULT, move |channel, _| {
            let eg_weak = eg_weak.clone();
            async move {
                let eg =
                    eg_weak.upgrade().expect("EventGraph dropped before protocol factory invoked");
                ProtocolEventGraph::init(eg, channel).await.unwrap()
            }
        })
        .await;

    eg
}

/// Bootstrap an N-node ring, start the P2P stacks, and wait 5
/// seconds for connections to converge.
///
/// Each call gets a fresh non-overlapping port range, so multiple
/// `make_network` invocations can run in parallel.
pub async fn make_network(ex: Arc<Executor<'static>>) -> Vec<EventGraphPtr> {
    use rand::{prelude::SliceRandom, rngs::ThreadRng};

    let port_base = alloc_port_base();
    let mut rng: ThreadRng = rand::thread_rng();
    let idxs: Vec<usize> = (0..N_NODES).collect();
    let mut nodes = vec![];
    for i in 0..N_NODES {
        let mut others = idxs.clone();
        others.remove(i);
        let conns: Vec<usize> = others.choose_multiple(&mut rng, N_CONNS).copied().collect();
        nodes.push(spawn_node(port_base, i, conns, ex.clone()).await);
    }
    for eg in &nodes {
        eg.p2p.clone().start().await.unwrap();
    }
    crate::system::sleep(5).await;
    nodes
}

/// Stop every node's P2P stack. Call at end of multi-node tests.
pub async fn shutdown_network(nodes: &[EventGraphPtr]) {
    for eg in nodes {
        eg.p2p.clone().stop().await;
    }
}

/// Run a multi-node test body on an executor sized for `N_NODES`.
pub fn run_multi_node_test<F, Fut>(body: F)
where
    F: FnOnce(Arc<Executor<'static>>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let ex = Arc::new(Executor::new());
    let ex_ = ex.clone();
    let (signal, shutdown) = channel::unbounded::<()>();
    easy_parallel::Parallel::new()
        .each(0..N_NODES, |_| future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            future::block_on(async {
                body(ex_).await;
                drop(signal);
            })
        });
}

mod test_identity {
    use darkfi_sdk::{crypto::poseidon_hash, pasta::pallas};
    use halo2_proofs::circuit::Value;
    use rand::rngs::OsRng;

    use super::*;
    use crate::{
        event_graph::{
            event::Header,
            rln::{
                epoch_of, hash_event, Blob, RLNNode, RegistrationAttestation, RegistrationBlob,
                MAX_MSG_LIMIT, RLN2_REGISTER_ZKBIN, RLN2_SIGNAL_ZKBIN,
            },
            NULL_PARENTS,
        },
        zk::{Proof, Witness, ZkCircuit},
        zkas::ZkBinary,
    };

    /// A test RLN identity with deterministic secrets and an
    /// auto-incrementing per-epoch `message_id` counter.
    pub struct TestIdentity {
        pub nullifier: pallas::Base,
        pub trapdoor: pallas::Base,
        pub user_message_limit: u64,
        pub message_id: u64,
        pub last_epoch: u64,
    }

    impl TestIdentity {
        /// Default test identity ("Alice").
        pub fn new() -> Self {
            Self {
                nullifier: pallas::Base::from(0xa11ce_u64),
                trapdoor: pallas::Base::from(0xb0b_u64),
                user_message_limit: RegistrationAttestation::FREE_TIER_LIMIT,
                message_id: 0,
                last_epoch: 0,
            }
        }

        /// Construct an identity from a seed for cross-identity
        /// tests. Different seeds yield distinct identities; the
        /// same seed always reproduces the same identity.
        pub fn with_seed(seed: u64) -> Self {
            // Mixing constants chosen so with_seed(1) does NOT
            // collide with new() (which uses 0xa11ce / 0xb0b
            // directly).
            let n = seed.wrapping_mul(0x9E3779B97F4A7C15_u64).wrapping_add(0x100);
            let t = seed.wrapping_mul(0xBF58476D1CE4E5B9_u64).wrapping_add(0x200);
            Self {
                nullifier: pallas::Base::from(n | 1),
                trapdoor: pallas::Base::from(t | 1),
                user_message_limit: RegistrationAttestation::FREE_TIER_LIMIT,
                message_id: 0,
                last_epoch: 0,
            }
        }

        pub fn identity_secret(&self) -> pallas::Base {
            poseidon_hash([self.nullifier, self.trapdoor])
        }

        pub fn identity_secret_hash(&self) -> pallas::Base {
            poseidon_hash([self.identity_secret(), pallas::Base::from(self.user_message_limit)])
        }

        pub fn commitment(&self) -> pallas::Base {
            poseidon_hash([self.identity_secret_hash()])
        }

        /// Advance the per-epoch message-id counter. Returns `None`
        /// when the per-epoch budget is exhausted.
        pub fn next_message_id(&mut self, now_millis: u64) -> Option<u64> {
            let epoch = epoch_of(now_millis);
            if epoch != self.last_epoch {
                self.last_epoch = epoch;
                self.message_id = 0;
            }
            if self.message_id >= self.user_message_limit {
                return None
            }
            let m = self.message_id;
            self.message_id += 1;
            Some(m)
        }

        /// Build a real registration proof and blob.
        pub fn create_registration(&self, eg: &EventGraphPtr) -> Result<RegistrationBlob> {
            let witnesses = vec![
                Witness::Base(Value::known(self.nullifier)),
                Witness::Base(Value::known(self.trapdoor)),
                Witness::Base(Value::known(pallas::Base::from(self.user_message_limit))),
                Witness::Base(Value::known(pallas::Base::from(MAX_MSG_LIMIT))),
            ];
            let pi = vec![
                self.commitment(),
                pallas::Base::from(self.user_message_limit),
                pallas::Base::from(MAX_MSG_LIMIT),
            ];
            let zkbin = ZkBinary::decode(RLN2_REGISTER_ZKBIN, false)?;
            let circuit = ZkCircuit::new(witnesses, &zkbin);
            let pk = eg.zk_keys.load_register_pk()?;
            let proof = Proof::create(&pk, &[circuit], &pi, &mut OsRng)?;
            Ok(RegistrationBlob {
                proof,
                user_message_limit: self.user_message_limit,
                max_message_limit: MAX_MSG_LIMIT,
                attestation: RegistrationAttestation::Free,
            })
        }

        /// Build a real signal proof and blob.
        pub async fn create_signal(
            &self,
            event: &Event,
            message_id: u64,
            eg: &EventGraphPtr,
        ) -> Result<Blob> {
            let commitment = self.commitment();
            let (root, path) = eg.rln_membership_path(&commitment).await;

            let app_id = eg.rln_app_id().as_field();
            let epoch = epoch_of(event.header.timestamp);
            let epoch_field = pallas::Base::from(epoch);
            let external_nullifier = poseidon_hash([epoch_field, app_id]);

            let a_0 = self.identity_secret_hash();
            let a_1 = poseidon_hash([a_0, external_nullifier, pallas::Base::from(message_id)]);
            let x = hash_event(event);
            let y = a_0 + x * a_1;
            let internal_nullifier = poseidon_hash([a_1]);

            let witnesses = vec![
                Witness::Base(Value::known(self.nullifier)),
                Witness::Base(Value::known(self.trapdoor)),
                Witness::Base(Value::known(pallas::Base::from(message_id))),
                Witness::SparseMerklePath(Value::known(path.path)),
                Witness::Base(Value::known(x)),
                Witness::Base(Value::known(pallas::Base::from(self.user_message_limit))),
                Witness::Base(Value::known(app_id)),
                Witness::Base(Value::known(epoch_field)),
            ];
            let pi = vec![
                root,
                external_nullifier,
                pallas::Base::from(self.user_message_limit),
                x,
                y,
                internal_nullifier,
            ];
            let zkbin = ZkBinary::decode(RLN2_SIGNAL_ZKBIN, false)?;
            let circuit = ZkCircuit::new(witnesses, &zkbin);
            let pk = eg.zk_keys.load_signal_pk()?;
            let proof = Proof::create(&pk, &[circuit], &pi, &mut OsRng)?;

            Ok(Blob {
                proof,
                y,
                internal_nullifier,
                user_msg_limit: self.user_message_limit,
                merkle_root: root,
            })
        }

        /// Register this identity directly into `eg` (skipping the
        /// gossip layer).
        pub async fn register_directly(&self, eg: &EventGraphPtr) -> Result<()> {
            let _blob = self.create_registration(eg)?;
            let commitment = self.commitment();
            let node = RLNNode::Registration(commitment);
            let content = darkfi_serial::serialize_async(&node).await;
            let mut parents = NULL_PARENTS;
            parents[0] = blake3::hash(b"register_directly-parent");
            let header = Header {
                timestamp: eg.current_genesis.read().await.header.timestamp,
                parents,
                layer: 1,
                content_hash: blake3::hash(&content),
            };
            let ev = Event { header, content };
            eg.apply_rln_static_event(&ev, &node).await?;
            Ok(())
        }
    }

    impl Default for TestIdentity {
        fn default() -> Self {
            Self::new()
        }
    }
}

pub use test_identity::TestIdentity;
