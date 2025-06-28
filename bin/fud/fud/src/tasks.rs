/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use log::{error, info};
use std::sync::Arc;

use darkfi::{
    dht::DhtHandler,
    system::{sleep, ExecutorPtr, StoppableTask},
    Error, Result,
};

use crate::{
    proto::{FudAnnounce, FudChunkReply, FudDirectoryReply, FudFileReply},
    Fud,
};

pub enum FetchReply {
    Directory(FudDirectoryReply),
    File(FudFileReply),
    Chunk(FudChunkReply),
}

/// Triggered when calling the `fud.get()` method.
/// It creates a new StoppableTask (running `fud.fetch_resource()`) and inserts
/// it into the `fud.fetch_tasks` hashmap. When the task is stopped it's
/// removed from the hashmap.
pub async fn get_task(fud: Arc<Fud>, executor: ExecutorPtr) -> Result<()> {
    loop {
        let (hash, path) = fud.get_rx.recv().await.unwrap();

        // Create the new task
        let mut fetch_tasks = fud.fetch_tasks.write().await;
        let task = StoppableTask::new();
        fetch_tasks.insert(hash, task.clone());
        drop(fetch_tasks);

        // Start the new task
        let fud_1 = fud.clone();
        let fud_2 = fud.clone();
        task.start(
            async move { fud_1.fetch_resource(&hash, &path).await },
            move |res| async move {
                // Remove the task from the `fud.fetch_tasks` hashmap once it is
                // stopped (error, manually, or just done).
                let mut fetch_tasks = fud_2.fetch_tasks.write().await;
                fetch_tasks.remove(&hash);
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "fud::get_task()", "Error while fetching resource: {e}")
                    }
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );
    }
}

/// Background task that announces our files once every hour.
/// Also removes seeders that did not announce for too long.
pub async fn announce_seed_task(fud: Arc<Fud>) -> Result<()> {
    let interval = 3600; // TODO: Make a setting

    loop {
        sleep(interval).await;

        let seeders = vec![fud.dht().node().await.into()];

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
            let _ = fud
                .announce(
                    &resource.hash,
                    &FudAnnounce { key: resource.hash, seeders: seeders.clone() },
                    fud.seeders_router.clone(),
                )
                .await;
        }

        info!(target: "fud::announce_seed_task()", "Pruning seeders...");
        fud.dht().prune_router(fud.seeders_router.clone(), interval.try_into().unwrap()).await;
    }
}
