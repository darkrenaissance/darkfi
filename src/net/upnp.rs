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

//! UPnP IGD port mapping implementation
//!
//! This module provides UPnP Internet Gateway Device (IGD) port mapping
//! with automatic lease renewal and persistent retry for roaming support.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use oxy_upnp_igd::{add_port_mapping_lazy, Protocol, RenewalHandle};
use smol::lock::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tracing::error;
use url::Url;

use crate::{
    net::settings::Settings,
    system::{sleep, ExecutorPtr, StoppableTask, StoppableTaskPtr},
    util::logger::verbose,
    Error, Result,
};

/// Trait for port mapping protocols (UPnP, NAT-PMP, PCP)
///
/// Each protocol runs its own persistent task that:
/// 1. Attempts to discover a gateway
/// 2. Creates port mappings when gateway is found
/// 3. Periodically refreshes the external address
/// 4. Retries discovery on failures (supports roaming)
pub trait PortMapping: Send + Sync {
    /// Start the port mapping protocol - runs forever with retries
    fn start(
        self: Arc<Self>,
        settings: Arc<AsyncRwLock<Settings>>,
        executor: ExecutorPtr,
    ) -> Result<()>;

    /// Stop the port mapping protocol
    fn stop(self: Arc<Self>);
}

/// UPnP port mapping configuration
#[derive(Clone, Debug)]
pub struct UpnpConfig {
    /// Port mapping lease duration in seconds
    pub lease_duration: u32,
    /// Gateway discovery timeout in seconds
    pub discovery_timeout_secs: u64,
    /// Description for port mapping (visible in router admin panel)
    pub mapping_description: String,
    /// External address refresh interval in seconds
    pub ext_addr_refresh: u64,
    /// How often to retry discovery if gateway not found (roaming support)
    pub retry_interval_secs: u64,
}

impl Default for UpnpConfig {
    fn default() -> Self {
        Self {
            lease_duration: 300,
            discovery_timeout_secs: 3,
            mapping_description: "DarkFi".to_string(),
            ext_addr_refresh: 120,
            retry_interval_secs: 60,
        }
    }
}

/// UPnP IGD port mapping protocol
///
/// Maintains a persistent task that:
/// - Discovers UPnP gateway
/// - Creates port mappings
/// - Periodically refreshes the external address
/// - Retries discovery on failures (supports roaming devices)
pub struct UpnpPortMapping {
    config: UpnpConfig,
    internal_endpoint: Url,
    handle: AsyncMutex<Option<RenewalHandle>>,
    task: StoppableTaskPtr,
}

impl UpnpPortMapping {
    /// Create a new UPnP port mapping instance
    pub fn new(config: UpnpConfig, internal_endpoint: Url) -> Self {
        Self {
            config,
            internal_endpoint,
            handle: AsyncMutex::new(None),
            task: StoppableTask::new(),
        }
    }

    /// Main protocol loop - runs forever with retries
    async fn run(&self, settings: Arc<AsyncRwLock<Settings>>, ex: &ExecutorPtr) -> Result<()> {
        loop {
            if self.try_create_mapping(ex).await.is_err() {
                verbose!(
                    target: "net::upnp",
                    "[P2P] UPnP: Gateway discovery failed, retrying in {}s",
                    self.config.retry_interval_secs
                );
                sleep(self.config.retry_interval_secs).await;
                continue;
            }

            verbose!(
                target: "net::upnp",
                "[P2P] UPnP: Gateway discovered, mapping active for {}",
                self.internal_endpoint
            );

            if self.run_refresh_loop(settings.clone()).await.is_err() {
                verbose!(
                    target: "net::upnp",
                    "[P2P] UPnP: Gateway lost, retrying discovery in {}s",
                    self.config.retry_interval_secs
                );
                sleep(self.config.retry_interval_secs).await;
                continue;
            }

            unreachable!("UPnP refresh loop should never complete normally");
        }
    }

    /// Attempt to discover gateway and create initial port mapping
    async fn try_create_mapping(&self, ex: &ExecutorPtr) -> Result<()> {
        let protocol = match self.internal_endpoint.scheme() {
            "tcp" | "tcp+tls" => Protocol::TCP,
            "quic" => Protocol::UDP,
            s => {
                verbose!(
                    target: "net::upnp",
                    "[P2P] UPnP: Unsupported scheme '{s}', skipping"
                );
                return Err(Error::NetworkServiceStopped);
            }
        };

        // UPnP IGD port mapping is IPv4-only
        let is_ipv4 = match self.internal_endpoint.host() {
            Some(url::Host::Ipv4(_)) => true,
            Some(url::Host::Ipv6(_)) => false,
            // Treating domains as IPv4 is safe and generally useful
            Some(url::Host::Domain(_)) => true,
            None => false,
        };

        if !is_ipv4 {
            verbose!(
                target: "net::upnp",
                "[P2P] UPnP: Skipping IPv6 endpoint {} (IGD pinhole not implemented)",
                self.internal_endpoint
            );
            return Err(Error::NetworkServiceStopped);
        }

        let internal_port = match self.internal_endpoint.port() {
            Some(port) => port,
            None => {
                verbose!(
                    target: "net::upnp",
                    "[P2P] UPnP: Invalid endpoint (missing port): {}",
                    self.internal_endpoint
                );
                return Err(Error::NetworkServiceStopped);
            }
        };

        let timeout = Duration::from_secs(self.config.discovery_timeout_secs);

        verbose!(
            target: "net::upnp",
            "[P2P] UPnP: Attempting port mapping for internal port {}",
            internal_port
        );

        // This will return immediately with a lazy handle
        let handle = add_port_mapping_lazy(
            ex.clone(),
            internal_port,
            protocol,
            &self.config.mapping_description,
            self.config.lease_duration,
            timeout,
        )
        .await?;

        *self.handle.lock().await = Some(handle);
        Ok(())
    }

    /// Refresh loop - updates external address periodically
    async fn run_refresh_loop(&self, settings: Arc<AsyncRwLock<Settings>>) -> Result<()> {
        loop {
            sleep(self.config.ext_addr_refresh).await;

            let Some(external_url) = self.get_external_address().await else {
                verbose!(
                    target: "net::upnp",
                    "[P2P] UPnP: Gateway no longer available"
                );
                return Err(Error::NetworkServiceStopped);
            };

            // Update settings with new external address
            let mut settings = settings.write().await;

            // Remove our old address (avoid duplicates)
            let internal_id = format_address_id(&self.internal_endpoint, "upnp");
            settings.external_addrs.retain(|addr: &Url| {
                if let Some(query) = addr.query() {
                    !query.contains(internal_id.as_str())
                } else {
                    true // Keep manually configured addresses
                }
            });

            // Add new external address
            settings.external_addrs.push(external_url.clone());

            verbose!(
                target: "net::upnp",
                "[P2P] UPnP: Updated external address: {}",
                external_url
            );
        }
    }

    /// Get current external address from UPnP handle
    async fn get_external_address(&self) -> Option<Url> {
        let handle = self.handle.lock().await;
        let handle = handle.as_ref()?;

        let external_ip = handle.external_ip().await;
        if external_ip.is_unspecified() {
            return None;
        }

        let external_port = handle.external_port();
        if external_port == 0 {
            return None;
        }

        let scheme = self.internal_endpoint.scheme();
        let internal_id = format_address_id(&self.internal_endpoint, "upnp");

        Url::parse(&format!(
            "{}://{}:{}?source=upnp&{}",
            scheme, external_ip, external_port, internal_id
        ))
        .ok()
    }
}

#[async_trait]
impl PortMapping for UpnpPortMapping {
    fn start(self: Arc<Self>, settings: Arc<AsyncRwLock<Settings>>, ex: ExecutorPtr) -> Result<()> {
        let self_ = self.clone();
        let settings_ = settings.clone();
        let ex_ = ex.clone();
        self.task.clone().start(
            async move { self_.run(settings_, &ex_).await },
            |result| async move {
                match result {
                    Ok(()) => {
                        // Should never complete normally
                        error!("[P2P] UPnP task completed unexpectedly");
                    }
                    Err(Error::NetworkServiceStopped) => {
                        // Expected when stopping
                    }
                    Err(e) => {
                        error!("[P2P] UPnP task error: {e}");
                    }
                }
            },
            Error::NetworkServiceStopped,
            ex,
        );
        Ok(())
    }

    fn stop(self: Arc<Self>) {
        // Stop the task (synchronous, signals the task to stop)
        self.task.stop_nowait();
        // Handle dropped - mapping expires naturally
        verbose!(
            target: "net::upnp",
            "[P2P] UPnP: Stopped port mapping for {}",
            self.internal_endpoint
        );
    }
}

/// Format an identifier for this listener + protocol combination
///
/// This utility is shared across all port mapping protocols (UPnP, NAT-PMP, PCP)
/// to create consistent, unique identifiers for external addresses.
pub fn format_address_id(endpoint: &Url, protocol: &str) -> String {
    // Hash the endpoint URL to create a unique alphanumeric identifier
    let mut hasher = DefaultHasher::new();
    endpoint.hash(&mut hasher);
    let hash = hasher.finish();

    format!("{}_cookie={:016x}", protocol, hash)
}

/// Create UPnP port mapping from URL query parameters
pub fn create_upnp_from_url(url: &Url) -> Option<Arc<dyn PortMapping>> {
    // Check if UPnP is explicitly enabled
    if !url.query_pairs().any(|(key, value)| key == "upnp_igd" && value == "true") {
        return None;
    }

    // Parse configuration from URL query parameters using safe URL library methods
    let mut config = UpnpConfig::default();

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "upnp_igd_lease_duration" => {
                if let Ok(val) = value.parse::<u32>() {
                    config.lease_duration = val;
                }
            }
            "upnp_igd_timeout" => {
                if let Ok(val) = value.parse::<u64>() {
                    config.discovery_timeout_secs = val;
                }
            }
            "upnp_igd_description" => {
                config.mapping_description = value.into_owned();
            }
            "upnp_igd_ext_addr_refresh" => {
                if let Ok(val) = value.parse::<u64>() {
                    config.ext_addr_refresh = val;
                }
            }
            _ => {}
        }
    }

    Some(Arc::new(UpnpPortMapping::new(config, url.clone())))
}

/// Initialize port mappings from URL query parameters.
///
/// This function parses the endpoint URL for port mapping configuration,
/// creates the appropriate port mapping instances, and starts them.
/// Each port mapping runs its own persistent task for lease renewal
/// and external address updates.
///
/// # Examples
/// ```text
/// // Enable UPnP with defaults
/// ?upnp_igd=true
///
/// // UPnP with custom settings
/// ?upnp_igd=true&upnp_igd_lease_duration=600
///
/// // Multiple protocols
/// ?upnp_igd=true&pcp=true
/// ```
///
/// # Arguments
/// * `endpoint` - The actual endpoint URL with query parameters and
///   *assigned port*
/// * `settings` - P2P settings for updating external addresses
/// * `ex` - Executor for running async tasks
///
/// # Returns
/// A vector of started port mappings (they auto-clean on drop)
pub fn setup_port_mappings(
    actual_endpoint: &Url,
    settings: Arc<AsyncRwLock<Settings>>,
    ex: ExecutorPtr,
) -> Vec<Arc<dyn PortMapping>> {
    let Some(mapping) = create_upnp_from_url(actual_endpoint) else { return vec![] };

    if let Err(e) = Arc::clone(&mapping).start(settings.clone(), ex.clone()) {
        error!(
            target: "net::upnp",
            "[P2P] UPnP port mapping: Failed to start for {}: {e}",
            actual_endpoint
        );
        return vec![]
    }

    verbose!(
        target: "net::upnp",
        "[P2P] UPnP: Port mapping started for {}",
        actual_endpoint
    );
    vec![mapping]

    // Future: Add NAT-PMP, PCP here with similar patterns
}
