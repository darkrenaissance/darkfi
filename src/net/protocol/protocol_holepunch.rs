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

//! NAT hole punching protocol (QUIC-only)
//!
//! 1. Peer A wants to connect to peer C (both behind NATs)
//! 2. A finds mutual peer B connected to both
//! 3. A sends `HolepunchRequest` to B
//! 4. B sends `HolepunchConnect` to both A and C with synchronized timing
//! 5. A and C simultaneously connect to each other's observed addresses

use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::{Arc, LazyLock},
    time::{Duration, UNIX_EPOCH},
};

use async_trait::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use rand::{rngs::OsRng, Rng};
use smol::{lock::Mutex as AsyncMutex, Executor};
use tracing::{debug, info, warn};
use url::Url;

use crate::{
    impl_p2p_message,
    net::{
        hosts::HostsPtr, metering::MeteringConfiguration, ChannelPtr, Message, MessageSubscription,
        P2pPtr, ProtocolBase, ProtocolBasePtr, ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    system::{sleep, timeout::timeout},
    util::time::NanoTimestamp,
    Error, Result,
};

const PROTO_NAME: &str = "ProtocolHolepunch";
const ALLOWED_SCHEME: &str = "quic";

/// Maximum time window in ms for a connection instruction to be valid.
const CONNECT_VALIDITY_MS: u64 = 5000;

/// Maximum clock skew allowed between peers in ms
const MAX_CLOCK_SKEW_MS: u64 = 2000;

/// Delay before simultaneous connection attempt in ms
const COORDINATION_DELAY_MS: u64 = 500;

/// Maximum pending holepunch requests per peer IP
const MAX_PENDING_PER_PEER: usize = 5;

/// Nonce expiry time for replay protection
const NONCE_EXPIRY_SECS: u64 = 60;

pub const HOLEPUNCH_MAX_BYTES: u64 = 1024;
pub const HOLEPUNCH_METERING: MeteringConfiguration = MeteringConfiguration {
    threshold: 10,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Request a peer to relay a holepunch
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HolepunchRequest {
    pub nonce: u64,
    pub target_addr: Url,
    pub our_addrs: Vec<Url>,
}

impl_p2p_message!(HolepunchRequest, "hpreq", HOLEPUNCH_MAX_BYTES, 1, HOLEPUNCH_METERING);

/// Instruction to attempt a holepunch connection
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HolepunchConnect {
    pub nonce: u64,
    pub peer_addr: Url,
    pub observed_addr: Url,
    pub connect_at: u64,
}

impl_p2p_message!(HolepunchConnect, "hpconn", HOLEPUNCH_MAX_BYTES, 1, HOLEPUNCH_METERING);

/// Result of a holepunch attempt
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HolepunchResult {
    pub nonce: u64,
    pub success: bool,
    pub error: Option<String>,
}

impl_p2p_message!(HolepunchResult, "hpres", HOLEPUNCH_MAX_BYTES, 1, HOLEPUNCH_METERING);

/// Tracks nonces for active `initiate_punch()` calls.
static INITIATOR_NONCES: LazyLock<AsyncMutex<HashSet<u64>>> =
    LazyLock::new(|| AsyncMutex::new(HashSet::new()));

struct UsedNonce {
    nonce: u64,
    timestamp: u64,
}

pub struct ProtocolHolepunch {
    channel: ChannelPtr,
    request_sub: MessageSubscription<HolepunchRequest>,
    connect_sub: MessageSubscription<HolepunchConnect>,
    _result_sub: MessageSubscription<HolepunchResult>,
    hosts: HostsPtr,
    p2p: P2pPtr,
    jobsman: ProtocolJobsManagerPtr,
    /// Recently used nonces for replay protection
    used_nonces: AsyncMutex<Vec<UsedNonce>>,
    /// Pending requests per peer IP for ratelimiting
    pending_count: AsyncMutex<HashMap<IpAddr, usize>>,
}

impl ProtocolHolepunch {
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        let request_sub = channel
            .subscribe_msg::<HolepunchRequest>()
            .await
            .expect("missing HolepunchRequest dispatcher");

        let connect_sub = channel
            .subscribe_msg::<HolepunchConnect>()
            .await
            .expect("missing HolepunchConnect dispatcher");

        let result_sub = channel
            .subscribe_msg::<HolepunchResult>()
            .await
            .expect("missing HolepunchResult dispatcher");

        Arc::new(Self {
            channel: channel.clone(),
            request_sub,
            connect_sub,
            _result_sub: result_sub,
            hosts: p2p.hosts(),
            p2p,
            jobsman: ProtocolJobsManager::new(PROTO_NAME, channel),
            used_nonces: AsyncMutex::new(Vec::new()),
            pending_count: AsyncMutex::new(HashMap::new()),
        })
    }

    fn is_quic(addr: &Url) -> bool {
        addr.scheme() == ALLOWED_SCHEME
    }

    fn get_ip(addr: &Url) -> Option<IpAddr> {
        addr.host_str().and_then(|h| h.parse().ok())
    }

    fn validate_connect_time(connect_at: u64) -> bool {
        let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        // Not too far in future
        if connect_at > now + CONNECT_VALIDITY_MS {
            return false
        }
        // Not already expired (with clock skew)
        if now > connect_at + CONNECT_VALIDITY_MS + MAX_CLOCK_SKEW_MS {
            return false
        }

        true
    }

    /// Get peer's observed addr from version message ensuring QUIC scheme
    fn get_observed_addr(channel: &ChannelPtr) -> Option<Url> {
        let version = channel.version.get()?;
        let addr = version.resolve_recv_addr.clone().unwrap_or(version.connect_recv_addr.clone());

        if Self::is_quic(&addr) {
            return Some(addr)
        }

        // Try converting to QUIC scheme
        // TODO: This needs to be added to Url crate
        let mut quic_addr = addr;
        quic_addr.set_scheme(ALLOWED_SCHEME).ok()?;
        Some(quic_addr)
    }

    async fn check_nonce(&self, nonce: u64) -> bool {
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();
        let mut used = self.used_nonces.lock().await;

        // Clean expired
        used.retain(|n| now - n.timestamp < NONCE_EXPIRY_SECS);

        // Check replay
        if used.iter().any(|n| n.nonce == nonce) {
            return false
        }

        used.push(UsedNonce { nonce, timestamp: now });
        true
    }

    async fn cleanup_nonces(&self) {
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();
        let mut used = self.used_nonces.lock().await;
        used.retain(|n| now - n.timestamp < NONCE_EXPIRY_SECS);
    }

    async fn check_rate_limit(&self, ip: IpAddr) -> bool {
        let pending = self.pending_count.lock().await;
        pending.get(&ip).copied().unwrap_or(0) < MAX_PENDING_PER_PEER
    }

    async fn inc_pending(&self, ip: IpAddr) {
        let mut pending = self.pending_count.lock().await;
        *pending.entry(ip).or_insert(0) += 1;
    }

    async fn dec_pending(&self, ip: IpAddr) {
        let mut pending = self.pending_count.lock().await;
        if let Some(count) = pending.get_mut(&ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                pending.remove(&ip);
            }
        }
    }

    fn verify_claimed_addrs(&self, claimed: &[Url]) -> bool {
        if claimed.is_empty() {
            return true
        }

        let observed_ip = Self::get_ip(self.channel.address());

        // Check if any claimed IP matches observed
        for addr in claimed {
            if Self::get_ip(addr) == observed_ip {
                return true
            }
        }

        // Check against version message external addrs
        if let Some(version) = self.channel.version.get() {
            for ext in &version.ext_send_addr {
                if claimed.contains(ext) {
                    return true
                }
            }
        }

        false
    }

    fn find_target_channel(&self, target: &Url) -> Option<ChannelPtr> {
        for channel in self.hosts.channels() {
            if !Self::is_quic(channel.address()) {
                continue
            }

            if let Some(version) = channel.version.get() {
                if version.ext_send_addr.contains(target) {
                    return Some(channel);
                }
            }

            if channel.address() == target {
                return Some(channel);
            }
        }

        None
    }

    async fn handle_relay_requests(self: Arc<Self>) -> Result<()> {
        // Only process on QUIC channels
        if !Self::is_quic(self.channel.address()) {
            // TODO: Make sure this does not hang around when chan is dropped.
            loop {
                let _ = self.request_sub.receive().await?;
            }
        }

        loop {
            let req = self.request_sub.receive().await?;

            // Validate target scheme
            if !Self::is_quic(&req.target_addr) {
                continue
            }

            // Replay protection
            if !self.check_nonce(req.nonce).await {
                warn!(
                    target: "net::protocol_holepunch::handle_relay_requests",
                    "[QUIC-NAT-RELAY] Rejecting: nonce replay",
                );
                continue
            }

            // Address verification
            if !self.verify_claimed_addrs(&req.our_addrs) {
                warn!(
                    target: "net::protocol_holepunch::handle_relay_requests",
                    "[QUIC-NAT-RELAY] Rejecting: addr verification failed",
                );
                continue
            }

            // Rate limiting
            let Some(peer_ip) = Self::get_ip(self.channel.address()) else { continue };
            if !self.check_rate_limit(peer_ip).await {
                warn!(
                    target: "net::protocol_holepunch::handle_relay_requests",
                    "[QUIC-NAT-RELAY] Rejecting: ratelimit for {}", peer_ip,
                );
                continue
            }

            // Find target channel
            let Some(target_chan) = self.find_target_channel(&req.target_addr) else {
                let _ = self
                    .channel
                    .send(&HolepunchResult {
                        nonce: req.nonce,
                        success: false,
                        error: Some("not connected to target".into()),
                    })
                    .await;
                continue
            };

            // Get observed addrs
            let Some(requester_observed) = Self::get_observed_addr(&self.channel) else { continue };
            let Some(target_observed) = Self::get_observed_addr(&target_chan) else { continue };

            // Both must be QUIC
            if !Self::is_quic(&requester_observed) || !Self::is_quic(&target_observed) {
                continue
            }

            self.inc_pending(peer_ip).await;

            let connect_at =
                UNIX_EPOCH.elapsed().unwrap().as_millis() as u64 + COORDINATION_DELAY_MS;

            // Send to requester
            let to_requester = HolepunchConnect {
                nonce: req.nonce,
                peer_addr: req.target_addr.clone(),
                observed_addr: target_observed,
                connect_at,
            };

            if self.channel.send(&to_requester).await.is_err() {
                self.dec_pending(peer_ip).await;
                continue
            }

            // Send to target
            let to_target = HolepunchConnect {
                nonce: req.nonce,
                peer_addr: self.channel.address().clone(),
                observed_addr: requester_observed,
                connect_at,
            };

            if target_chan.send(&to_target).await.is_err() {
                self.dec_pending(peer_ip).await;
                continue
            }

            info!(
                target: "net::protocol_holepunch::handle_relay_requests",
                "[QUIC-NAT-RELAY] Relayed punch {} <-> {}",
                self.channel.display_address(),
                target_chan.display_address(),
            );

            // Cleanup rate limit after delay
            let self_ = self.clone();
            self.p2p
                .executor()
                .spawn(async move {
                    sleep(COORDINATION_DELAY_MS / 1000 + 1).await;
                    self_.dec_pending(peer_ip).await;
                })
                .detach();
        }
    }

    async fn handle_connect_instructions(self: Arc<Self>) -> Result<()> {
        if !Self::is_quic(self.channel.address()) {
            loop {
                let _ = self.connect_sub.receive().await?;
            }
        }

        loop {
            let conn = self.connect_sub.receive().await?;

            // Validate
            if !Self::is_quic(&conn.observed_addr) || !Self::validate_connect_time(conn.connect_at)
            {
                continue
            }

            // Skip if an initiator is handling this nonce
            if INITIATOR_NONCES.lock().await.contains(&conn.nonce) {
                continue
            }

            // Spawn connection attempt
            let p2p = self.p2p.clone();
            let observed = conn.observed_addr.clone();
            let connect_at = conn.connect_at;

            p2p.executor()
                .spawn(async move {
                    // Wait until scheduled time
                    let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
                    if connect_at > now {
                        smol::Timer::after(Duration::from_millis(connect_at - now)).await;
                    }

                    // Connect
                    match p2p.session_direct().get_channel(&observed).await {
                        Ok(chan) => {
                            info!(
                                target: "net::protocol_holepunch::handle_connect_instructions",
                                "[QUIC-NAT-CONNECT] Punch succeeded: {}", chan.display_address(),
                            );
                        }
                        Err(e) => {
                            debug!(
                                target: "net::protocol_holepunch::handle_connect_instructions",
                                "[QUIC-NAT-CONNECT] Punch failed to {}: {}", observed, e,
                            );
                        }
                    }
                })
                .detach();
        }
    }

    async fn nonce_cleanup_loop(self: Arc<Self>) -> Result<()> {
        loop {
            sleep(NONCE_EXPIRY_SECS).await;
            self.cleanup_nonces().await;
        }
    }

    /// Initiate a holepunch to target via relay.
    pub async fn initiate_punch(
        p2p: P2pPtr,
        target: &Url,
        relay: &ChannelPtr,
    ) -> Result<ChannelPtr> {
        // Validate schemes
        if !Self::is_quic(target) {
            return Err(Error::UnsupportedTransport(format!(
                "Target must be QUIC: {}",
                target.scheme(),
            )));
        }

        if !Self::is_quic(relay.address()) {
            return Err(Error::UnsupportedTransport(format!(
                "Relay must be QUIC: {}",
                relay.address().scheme()
            )));
        }

        // Register nonce
        let nonce: u64 = OsRng.gen();
        INITIATOR_NONCES.lock().await.insert(nonce);

        // Execute with cleanup
        let result = Self::do_initiate_punch(p2p, target, relay, nonce).await;
        INITIATOR_NONCES.lock().await.remove(&nonce);
        result
    }

    async fn do_initiate_punch(
        p2p: P2pPtr,
        target: &Url,
        relay: &ChannelPtr,
        nonce: u64,
    ) -> Result<ChannelPtr> {
        // Get our QUIC external addr
        // TODO: Perhaps STUN, perhaps IP discovery through live peers
        let our_addrs: Vec<Url> =
            p2p.hosts().external_addrs().await.into_iter().filter(Self::is_quic).collect();

        // Subscribe to receive connect instruction
        let connect_sub =
            relay.subscribe_msg::<HolepunchConnect>().await.map_err(|_| Error::ChannelStopped)?;

        // Send request
        relay.send(&HolepunchRequest { nonce, target_addr: target.clone(), our_addrs }).await?;

        // Wait for our connect instruction
        let conn = loop {
            let msg = timeout(Duration::from_millis(CONNECT_VALIDITY_MS), connect_sub.receive())
                .await
                .map_err(|_| Error::ChannelTimeout)??;

            // We're looking for our nonce specifically
            if msg.nonce == nonce {
                break msg;
            }
        };

        // Validate response
        if !Self::is_quic(&conn.observed_addr) {
            return Err(Error::UnsupportedTransport("Response addr not QUIC".into()));
        }
        if !Self::validate_connect_time(conn.connect_at) {
            return Err(Error::ChannelTimeout);
        }

        // Wait until scheduled time
        let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        if conn.connect_at > now {
            smol::Timer::after(Duration::from_millis(conn.connect_at - now)).await;
        }

        // Attempt punch
        p2p.session_direct().get_channel(&conn.observed_addr).await
    }
}

#[async_trait]
impl ProtocolBase for ProtocolHolepunch {
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_holepunch", "Starting on {}", self.channel.display_address());
        self.jobsman.clone().start(ex.clone());
        self.jobsman.clone().spawn(self.clone().handle_relay_requests(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_connect_instructions(), ex.clone()).await;
        self.jobsman.spawn(self.clone().nonce_cleanup_loop(), ex).await;
        Ok(())
    }

    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}
