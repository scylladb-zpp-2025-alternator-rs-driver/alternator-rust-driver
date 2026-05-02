//! Maintains and updates a list of known live Alternator nodes using the `/localnodes` endpoint.
//!
//! # Overview
//!
//! [`LiveNodes`] is constructed from an [`AlternatorConfig`] and seeded with a list of hosts.
//! Once [`start`] is called, a background Tokio task
//! periodically calls the [`update_live_nodes`] function which requests the known
//! nodes in a random order to get an updated list of live nodes. After starting, the list is
//! guaranteed to always contain nodes in the highest available scope in the fallback chain provided by the user.
//! Underneath it uses a basic [`reqwest::Client`] with timeouts.
//!
//! # Polling cadence
//!
//! The refresh loop has two cadences:
//!
//! - **Active** ([`active_interval`]): used while the client is being called
//!   regularly. Polls run frequently to keep the view fresh under load.
//! - **Idle** ([`idle_interval`]): used when no caller has touched
//!   [`LiveNodes`] recently. An incoming request wakes the loop early via a [`Notify`].
//!
//! Activity is tracked through [`mark_activity`], which every read path calls.
//!
//! # Discovery mechanism
//!
//! Each refresh starts from the highest scope in fallback chain, it shuffles
//! the current node list and walks it as a candidate queue:
//! - If a node responds with a non-empty list, the list is used as the new live nodes list,
//!   and the refresh ends.
//! - If a node responds with an empty list, it is put back at the end of the queue,
//!   and the next node is tried, with the next fallback scope.
//! - A network error causes the node to be dropped from the queue, but the next nodes are
//!   tried with the same scope.
//! - If the queue is exhausted without a successful response, it is populated with
//!   the seed nodes, and the process repeats. If the seeds are exhausted without success, the refresh ends with no changes.
//!
//! Once it successfully gets a non-empty response, atomically updates the [`live_nodes`] list using ['ArcSwap].
//!
//!  # Lifetime
//!
//! The background task holds a [`Weak`] reference to its [`LiveNodes`], so it
//! terminates on its own once the last external [`Arc`] is dropped. [`Drop`]
//! additionally aborts the task to avoid waiting out the current sleep.
//!
//! [`AlternatorConfig`]: crate::config::AlternatorConfig
//! [`RoutingScope`]: crate::routing_scope::RoutingScope
//! [`ArcSwap`]: arc_swap::ArcSwap
//! [`Notify`]: tokio::sync::Notify
//! [`Weak`]: std::sync::Weak
//! [`Arc`]: std::sync::Arc
//! [`active_interval`]: LiveNodes::active_interval
//! [`idle_interval`]: LiveNodes::idle_interval
//! [`mark_activity`]: LiveNodes::mark_activity
//! [`start`]: LiveNodes::start
//! [`update_live_nodes`]: LiveNodes::update_live_nodes
//! [`live_nodes`]: LiveNodes::live_nodes

use crate::routing_scope::RoutingScope;
use arc_swap::ArcSwap;
use rand::seq::SliceRandom;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use url::Url;

const DEFAULT_ACTIVE_REFRESH_INTERVAL_MS: u64 = 1000;
const DEFAULT_IDLE_REFRESH_INTERVAL_MS: u64 = 60000;
#[derive(Debug)]
pub struct LiveNodes {
    routing_scope: RoutingScope,
    active_interval: Duration,
    idle_interval: Duration,
    counter: Arc<AtomicUsize>,
    live_nodes: ArcSwap<Vec<Url>>,
    seed_urls: Vec<Url>,
    alternator_scheme: String,
    port: Option<u16>,
    client: reqwest::Client,
    last_activity: Arc<Mutex<Instant>>,
    notify: Arc<tokio::sync::Notify>,
    bg_task: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
}

impl LiveNodes {
    pub fn new(config: &crate::config::AlternatorConfig) -> Option<Arc<Self>> {
        let active_interval = config
            .active_interval()
            .unwrap_or(DEFAULT_ACTIVE_REFRESH_INTERVAL_MS);
        let idle_interval = config
            .idle_interval()
            .unwrap_or(DEFAULT_IDLE_REFRESH_INTERVAL_MS);
        let routing_scope = config
            .routing_scope()
            .unwrap_or(RoutingScope::from_cluster());
        let alternator_scheme = config.scheme().unwrap_or("http://".to_string());
        let port = config.port();
        let seed_nodes = config.seed_hosts().unwrap_or_default();

        let seed_urls = seed_nodes
            .iter()
            .filter_map(|addr| {
                let mut url = Url::parse(&format!("{}{}", alternator_scheme, addr)).ok()?;
                url.set_port(port).ok()?;
                Some(url)
            })
            .collect::<Vec<_>>();
        if seed_urls.is_empty() {
            return None;
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(2))
            .build()
            .ok()?;

        Some(Arc::new(Self {
            routing_scope,
            active_interval: Duration::from_millis(active_interval),
            idle_interval: Duration::from_millis(idle_interval),
            counter: Arc::new(AtomicUsize::new(0)),
            live_nodes: ArcSwap::from_pointee(seed_urls.clone()),
            seed_urls,
            alternator_scheme,
            port,
            client,
            last_activity: Arc::new(Mutex::new(Instant::now())),
            notify: Arc::new(tokio::sync::Notify::new()),
            bg_task: std::sync::Mutex::new(None),
        }))
    }

    fn host_to_uri(&self, addr: &str) -> Result<Url, url::ParseError> {
        let mut url = Url::parse(&format!("{}{}", self.alternator_scheme, addr))?;
        url.set_port(self.port)
            .map_err(|()| url::ParseError::InvalidPort)?;
        Ok(url)
    }

    pub fn start(self: Arc<Self>) {
        let weak_self = Arc::downgrade(&self);
        let notify = self.notify.clone();

        self.mark_activity();
        let handle = tokio::spawn(async move {
            loop {
                let (idle_interval, active_interval, is_idle) = {
                    let Some(strong_self) = weak_self.upgrade() else {
                        break;
                    };

                    strong_self.update_live_nodes().await;

                    let last = *strong_self.last_activity.lock().unwrap();
                    (
                        strong_self.idle_interval,
                        strong_self.active_interval,
                        last.elapsed() >= strong_self.idle_interval,
                    )
                };

                if !is_idle {
                    tokio::time::sleep(active_interval).await;
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(idle_interval) => {}
                        _ = notify.notified() => {}
                    }
                }
            }
        });

        if let Ok(mut guard) = self.bg_task.lock() {
            *guard = Some(handle.abort_handle());
        }
    }

    fn mark_activity(&self) {
        let now = Instant::now();
        let mut last = self.last_activity.lock().unwrap();
        let was_idle = now.duration_since(*last) > self.idle_interval;
        *last = now;
        if was_idle {
            self.notify.notify_one();
        }
    }

    /// Returns the current list of live nodes starting with the next node in round-robin order.
    /// Used by [`crate::QueryPlan`] round-robin strategy.
    pub fn get_live_nodes_round_robin(&self) -> Vec<Url> {
        self.mark_activity();
        let live_nodes = self.live_nodes.load();

        let len = live_nodes.len();
        if len == 0 {
            return Vec::new();
        }

        let counter = self.counter.fetch_add(1, Ordering::Relaxed) % len;
        let (left, right) = live_nodes.split_at(counter);
        [right, left].concat()
    }

    pub async fn update_live_nodes(&self) {
        let mut scope = &self.routing_scope;
        // Live nodes in a random order.
        let mut nodes = self.live_nodes.load().as_ref().clone();
        nodes.shuffle(&mut rand::rng());
        let mut candidates: VecDeque<Url> = nodes.into();
        let mut using_seeds = false;

        while let Some(node_addr) = candidates.pop_front() {
            let url = scope.build_localnodes_url(node_addr.clone());
            let result = async {
                self.client
                    .get(url)
                    .send()
                    .await
                    .ok()?
                    .json::<Vec<String>>()
                    .await
                    .ok()
            }
            .await;

            // Request failed: try the next candidate, or fall back to seeds.
            let Some(mut nodes) = result else {
                if candidates.is_empty() && !using_seeds {
                    using_seeds = true;
                    candidates = self.seed_urls.clone().into();
                }
                continue;
            };

            nodes.sort();
            let new_nodes: Vec<Url> = nodes
                .into_iter()
                .filter_map(|addr| self.host_to_uri(&addr).ok())
                .collect();

            // Empty result: retry under a fallback scope if one exists.
            if new_nodes.is_empty() {
                let Some(fallback) = scope.fallback() else {
                    return;
                };
                scope = fallback;
                candidates.push_back(node_addr);
                continue;
            }

            if **self.live_nodes.load() != new_nodes {
                self.live_nodes.store(Arc::new(new_nodes));
            }
            return;
        }
    }
}

impl Drop for LiveNodes {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.bg_task.lock()
            && let Some(task) = guard.take()
        {
            task.abort();
        }
    }
}
