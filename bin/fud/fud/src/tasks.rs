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

use std::sync::Arc;

use tracing::{error, info, warn};

use darkfi::{
    dht::{event::DhtEvent, DhtHandler, DhtNode},
    geode::hash_to_string,
    system::{sleep, StoppableTask},
    Error, Result,
};

use crate::{
    event::{self, notify_event},
    proto::FudAnnounce,
    resource::ResourceStatus,
    Fud, FudEvent, FudState,
};

/// Handle DHT events in fud.
pub async fn handle_dht_events(fud: Arc<Fud>) -> Result<()> {
    let sub = fud.dht().subscribe().await;
    loop {
        let event = sub.receive().await;

        match event {
            DhtEvent::ValueLookupCompleted { key, values, .. } => {
                let mut seeders: Vec<_> = values.into_iter().flatten().collect();
                seeders.dedup_by_key(|seeder| seeder.node.id());
                notify_event!(fud, SeedersFound, {
                    hash: key,
                    seeders
                });
            }
            DhtEvent::BootstrapCompleted => {
                let _ = fud.init().await;
                notify_event!(fud, Ready);
            }
            _ => {}
        }
    }
}

/// Triggered when calling the `fud.get()` method.
/// It creates a new StoppableTask (running `fud.fetch_resource()`) and inserts
/// it into the `fud.fetch_tasks` hashmap. When the task is stopped it's
/// removed from the hashmap.
pub async fn get_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        let (hash, path, files) = fud.get_rx.recv().await.unwrap();

        // Create the new task
        let mut fetch_tasks = fud.fetch_tasks.write().await;
        let task = StoppableTask::new();
        fetch_tasks.insert(hash, task.clone());
        drop(fetch_tasks);

        // Start the new task
        let fud_1 = fud.clone();
        let fud_2 = fud.clone();
        task.start(
            async move { fud_1.fetch_resource(&hash, &path, &files).await },
            move |res| async move {
                // Remove the task from the `fud.fetch_tasks` hashmap once it is
                // stopped (error, manually, or just done).
                let mut fetch_tasks = fud_2.fetch_tasks.write().await;
                fetch_tasks.remove(&hash);

                // If there is still a lookup task for this hash, stop it
                let lookup_tasks = fud_2.lookup_tasks.read().await;
                if let Some(lookup_task) = lookup_tasks.get(&hash) {
                    lookup_task.stop().await;
                }

                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "fud::get_task()", "Error while fetching resource: {e}");

                        // Set resource status to `Incomplete`
                        let mut resources = fud_2.resources.write().await;
                        if let Some(resource) = resources.get_mut(&hash) {
                            resource.status = ResourceStatus::Incomplete(Some(e.to_string()));
                        }
                        drop(resources);

                        // Send a DownloadError for any error that stopped the fetch task
                        notify_event!(fud_2, DownloadError, {
                            hash,
                            error: e.to_string(),
                        });
                    }
                }
            },
            Error::DetachedTaskStopped,
            fud.executor.clone(),
        );
    }
}

/// Triggered when calling the `fud.put()` method.
pub async fn put_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        let path = fud.put_rx.recv().await.unwrap();

        // Create the new task
        let mut put_tasks = fud.put_tasks.write().await;
        let task = StoppableTask::new();
        put_tasks.insert(path.clone(), task.clone());
        drop(put_tasks);

        // Start the new task
        let fud_1 = fud.clone();
        let fud_2 = fud.clone();
        let path_ = path.clone();
        task.start(
            async move { fud_1.insert_resource(&path_).await },
            move |res| async move {
                // Remove the task from the `fud.put_tasks` hashmap once it is
                // stopped (error, manually, or just done).
                let mut put_tasks = fud_2.put_tasks.write().await;
                put_tasks.remove(&path);
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "fud::put_task()", "Error while inserting resource: {e}");

                        // Send a InsertError for any error that stopped the fetch task
                        notify_event!(fud_2, InsertError, {
                            path,
                            error: e.to_string(),
                        });
                    }
                }
            },
            Error::DetachedTaskStopped,
            fud.executor.clone(),
        );
    }
}

/// Triggered when you need to lookup seeders for a resource.
pub async fn lookup_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        let key = fud.lookup_rx.recv().await.unwrap();

        let mut lookup_tasks = fud.lookup_tasks.write().await;
        let task = StoppableTask::new();
        lookup_tasks.insert(key, task.clone());
        drop(lookup_tasks);

        let fud_1 = fud.clone();
        let fud_2 = fud.clone();
        task.start(
            async move {
                fud_1.dht.lookup_value(&key).await;
                Ok(())
            },
            move |res| async move {
                // Remove the task from the `fud.lookup_tasks` hashmap once it is
                // stopped (error, manually, or just done).
                let mut lookup_tasks = fud_2.lookup_tasks.write().await;
                lookup_tasks.remove(&key);
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "dht::lookup_task()", "Error in DHT lookup task: {e}");
                    }
                }
            },
            Error::DetachedTaskStopped,
            fud.executor.clone(),
        );
    }
}

/// After pinging an inbound connection, this task is triggered to make sure
/// that you are able to reach at least one of the node's external address.
/// [`Fud::ping()`] will take care of adding the node to our buckets.
pub async fn verify_node_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        let node = fud.verify_node_rx.recv().await.unwrap();
        if let Ok((channel, _)) = fud.dht.create_channel_to_node(&node).await {
            fud.dht.cleanup_channel(channel).await;
        }
    }
}

/// Background task that announces our files once every hour.
/// Also removes seeders that did not announce for too long.
pub async fn announce_seed_task(fud: Arc<Fud>) -> Result<()> {
    let interval = 3600; // TODO: Make a setting

    loop {
        sleep(interval).await;

        info!(target: "fud::announce_seed_task()", "Verifying seeds...");
        let seeding_resources = match fud.verify_resources(None).await {
            Ok(resources) => resources,
            Err(e) => {
                error!(target: "fud::announce_seed_task()", "Error while verifying seeding resources: {e}");
                continue;
            }
        };

        info!(target: "fud::announce_seed_task()", "Announcing files...");
        for resource in seeding_resources {
            if let Ok(seeder) = fud.new_seeder(&resource.hash).await {
                let seeders = vec![seeder];
                let _ = fud
                    .dht
                    .announce(
                        &resource.hash,
                        &seeders.clone(),
                        &FudAnnounce { key: resource.hash, seeders },
                    )
                    .await;
            }
        }

        info!(target: "fud::announce_seed_task()", "Pruning seeders...");
        fud.prune_seeders(interval.try_into().unwrap()).await;
    }
}

/// Background task that:
/// 1. Updates the [`crate::bitcoin::BitcoinHashCache`]
/// 2. Removes old nodes from the DHT
/// 3. Removes old nodes from the seeders router
/// 4. If the Bitcoin block hash we currently use in our `fud.node_data` is too old, we update it and reset our DHT
pub async fn node_id_task(fud: Arc<Fud>) -> Result<()> {
    let interval = 600; // TODO: Make a setting

    loop {
        sleep(interval).await;

        let mut pow = fud.pow.write().await;
        if !pow.settings.read().await.btc_enabled {
            continue
        }

        let btc = &mut pow.bitcoin_hash_cache;

        if btc.update().await.is_err() {
            continue
        }

        let state = fud.state.read().await;
        if state.is_none() {
            continue
        }
        let block = state.clone().unwrap().node_data.btc_block_hash;
        drop(state);
        let needs_dht_reset = match btc.block_hashes.iter().position(|b| *b == block) {
            Some(i) => i < 6,
            None => true,
        };

        if !needs_dht_reset {
            // Removes nodes in the DHT with unknown BTC block hashes.
            let dht = fud.dht();
            let mut buckets = dht.buckets.write().await;
            for bucket in buckets.iter_mut() {
                for (i, node) in bucket.nodes.clone().iter().enumerate().rev() {
                    // If this node's BTC block hash is unknown, remove it from the bucket
                    if !btc.block_hashes.contains(&node.data.btc_block_hash) {
                        bucket.nodes.remove(i);
                        info!(target: "fud::node_id_task()", "Removed node {} from the DHT (BTC block hash too old or unknown)", hash_to_string(&node.id()));
                    }
                }
            }
            drop(buckets);

            // Removes nodes in the seeders router with unknown BTC block hashes
            let mut seeders_table = fud.dht.hash_table.write().await;
            for (key, seeders) in seeders_table.iter_mut() {
                for (i, seeder) in seeders.clone().iter().enumerate().rev() {
                    if !btc.block_hashes.contains(&seeder.node.data.btc_block_hash) {
                        seeders.remove(i);
                        info!(target: "fud::node_id_task()", "Removed node {} from the seeders of key {} (BTC block hash too old or unknown)", hash_to_string(&seeder.node.id()), hash_to_string(key));
                    }
                }
            }

            continue
        }

        info!(target: "fud::node_id_task()", "Creating a new node id...");
        let (node_data, secret_key) = match pow.generate_node().await {
            Ok(res) => res,
            Err(e) => {
                warn!(target: "fud::node_id_task()", "Error creating a new node id: {e}");
                continue
            }
        };
        drop(pow);
        info!(target: "fud::node_id_task()", "New node id: {}", hash_to_string(&node_data.id()));

        // Close all channels
        let dht = fud.dht();
        let mut channel_cache = dht.channel_cache.write().await;
        for channel in dht.p2p.hosts().channels().clone() {
            channel.stop().await;
            channel_cache.remove(&channel.info.id);
        }
        drop(channel_cache);

        // Reset the DHT: removes known nodes and seeders
        dht.reset().await;

        // Update our node data and our secret key
        let mut state = fud.state.write().await;
        *state = Some(FudState { node_data, secret_key });

        // DHT will be bootstrapped on the next channel connection
    }
}

macro_rules! start_task {
    ($fud:expr, $task_name:expr, $task_fn:expr, $tasks:expr) => {{
        info!(target: "fud", "Starting {} task", $task_name);
        let task = StoppableTask::new();
        let fud_ = $fud.clone();
        task.clone().start(
            async move { $task_fn(fud_).await },
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "fud", "Failed starting {} task: {e}", $task_name),
                }
            },
            Error::DetachedTaskStopped,
            $fud.executor.clone(),
        );
        $tasks.insert($task_name.to_string(), task);
    }};
}
pub(crate) use start_task;
