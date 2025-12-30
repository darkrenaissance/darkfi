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

use std::collections::VecDeque;

use tracing::debug;

use crate::util::time::NanoTimestamp;

/// Struct representing metering configuration parameters.
#[derive(Clone, Debug)]
pub struct MeteringConfiguration {
    /// Defines the threshold after which rate limit kicks in.
    /// Set to 0 for no threshold.
    ///
    /// If we don't use raw count as our metric, it should be calculated
    /// by multiplying the median increase of the measured item with the
    /// "max" number of items we want before rate limit starts.
    /// For example, if we measure some item that increases our total
    /// measurement by ~5 and want to rate limit after about 10, this
    /// should be set as 50.
    pub threshold: u64,
    /// Sleep time for each unit over the threshold, in milliseconds.
    ///
    /// This is used to calculate sleep time when ratelimit is active.
    /// The computed sleep time when we are over the threshold will be:
    ///     sleep_time = (total - threshold) * sleep_step
    pub sleep_step: u64,
    /// Parameter defining the expiration of each item, for time based
    /// decay, in nano seconds. Set to 0 for no expiration.
    pub expiry_time: NanoTimestamp,
}

impl MeteringConfiguration {
    /// Generate a new `MeteringConfiguration` for provided threshold,
    /// sleep step and expiration time (seconds).
    pub fn new(threshold: u64, sleep_step: u64, expiry_time: u128) -> Self {
        Self { threshold, sleep_step, expiry_time: NanoTimestamp::from_secs(expiry_time) }
    }
}

impl Default for MeteringConfiguration {
    fn default() -> Self {
        Self { threshold: 0, sleep_step: 0, expiry_time: NanoTimestamp(0) }
    }
}

/// Default `MeteringConfiguration` as a constant,
/// so it can be used in trait macros.
pub const DEFAULT_METERING_CONFIGURATION: MeteringConfiguration =
    MeteringConfiguration { threshold: 0, sleep_step: 0, expiry_time: NanoTimestamp(0) };

/// Struct to keep track of some sequential metered actions and compute
/// rate limits.
///
/// The queue uses a time based decay and prunes metering information
/// after corresponding expiration time has passed.
#[derive(Debug)]
pub struct MeteringQueue {
    /// Metering configuration of the queue.
    config: MeteringConfiguration,
    /// Ring buffer keeping track of action execution timestamp and
    /// its metered value.
    queue: VecDeque<(NanoTimestamp, u64)>,
}

impl MeteringQueue {
    /// Generate a new `MeteringQueue` for provided `MeteringConfiguration`.
    pub fn new(config: MeteringConfiguration) -> Self {
        Self { config, queue: VecDeque::new() }
    }

    /// Prune expired metering information from the queue.
    pub fn clean(&mut self) {
        // Check if expiration has been set
        if self.config.expiry_time.0 == 0 {
            return
        }

        // Iterate the queue to cleanup expired elements
        while let Some((ts, _)) = self.queue.front() {
            // This is an edge case where system reports a future timestamp
            // therefore elapsed computation fails.
            let Ok(elapsed) = ts.elapsed() else {
                debug!(target: "net::metering::MeteringQueue::clean", "Timestamp [{ts}] is in future. Removing...");
                let _ = self.queue.pop_front();
                continue
            };

            // Check if elapsed time is over the expiration limit
            if elapsed < self.config.expiry_time {
                break
            }

            // Remove element
            let _ = self.queue.pop_front();
        }
    }

    /// Add new metering value to the queue, after
    /// prunning expired metering information.
    /// If no threshold has been set, the insert is
    /// ignored.
    pub fn push(&mut self, value: &u64) {
        // Check if threshold has been set
        if self.config.threshold == 0 {
            return
        }

        // Prune expired elements
        self.clean();

        // Push the new value
        self.queue.push_back((NanoTimestamp::current_time(), *value));
    }

    /// Compute the current metered values total.
    pub fn total(&self) -> u64 {
        let mut total = 0;
        for (_, value) in &self.queue {
            total += value;
        }
        total
    }

    /// Compute sleep time for current metered values total, based on
    /// the metering configuration.
    ///
    /// The sleep time increases linearly, based on configuration sleep
    /// step. For example, in a raw count metering model, if we set the
    /// configuration with threshold = 6 and sleep_step = 250, when
    /// total = 10, returned sleep time will be 1000 ms.
    ///
    /// Sleep times table for the above example:
    ///
    /// | Total | Sleep Time (ms) |
    /// |-------|-----------------|
    /// | 0     | 0               |
    /// | 4     | 0               |
    /// | 6     | 0               |
    /// | 7     | 250             |
    /// | 8     | 500             |
    /// | 9     | 750             |
    /// | 10    | 1000            |
    /// | 14    | 2000            |
    /// | 18    | 3000            |
    pub fn sleep_time(&self) -> Option<u64> {
        // Check if threshold has been set
        if self.config.threshold == 0 {
            return None
        }

        // Check if we are over the threshold
        let total = self.total();
        if total < self.config.threshold {
            return None
        }

        // Compute the actual sleep time
        Some((total - self.config.threshold) * self.config.sleep_step)
    }
}

#[test]
fn test_net_metering_queue_default() {
    let mut queue = MeteringQueue::new(MeteringConfiguration::default());
    for _ in 0..100 {
        queue.push(&1);
        assert!(queue.queue.is_empty());
        assert_eq!(queue.total(), 0);
        assert!(queue.sleep_time().is_none());
    }
}

#[test]
fn test_net_metering_queue_raw_count() {
    let threshold = 6;
    let sleep_step = 250;
    let metering_configuration = MeteringConfiguration::new(threshold, sleep_step, 0);
    let mut queue = MeteringQueue::new(metering_configuration);
    for i in 1..threshold {
        queue.push(&1);
        assert_eq!(queue.total(), i);
        assert!(queue.sleep_time().is_none());
    }
    for i in threshold..100 {
        queue.push(&1);
        assert_eq!(queue.total(), i);
        assert_eq!(queue.sleep_time(), Some((i - threshold) * sleep_step));
    }
}

#[test]
fn test_net_metering_queue_sleep_time() {
    let metered_value_median = 5;
    let threshold_items = 10;
    let threshold = metered_value_median * threshold_items;
    let sleep_step = 50;
    let metering_configuration = MeteringConfiguration::new(threshold, sleep_step, 0);
    let mut queue = MeteringQueue::new(metering_configuration);
    for i in 1..threshold_items {
        queue.push(&metered_value_median);
        assert_eq!(queue.total(), (i * metered_value_median));
        assert!(queue.sleep_time().is_none());
    }
    for i in threshold_items..100 {
        queue.push(&metered_value_median);
        let expected_total = i * metered_value_median;
        assert_eq!(queue.total(), expected_total);
        assert_eq!(queue.sleep_time(), Some((expected_total - threshold) * sleep_step));
    }
}
