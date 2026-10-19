use std::collections::HashMap;
use std::time::Duration;

use bollard::Docker;
use bollard::query_parameters::{
    ListContainersOptionsBuilder, StatsOptionsBuilder, StopContainerOptionsBuilder,
};
use futures_util::{StreamExt, future::join_all};

use crate::history::History;

pub const CONTAINER_HISTORY_LEN: usize = 120;

#[derive(Clone)]
pub struct ContainerRow {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub running: bool,
    pub cpu_pct: f64,
    pub mem_usage: u64,
    pub mem_limit: u64,
    pub net_rx_bps: f64,
    pub net_tx_bps: f64,
    pub pids: u64,
    pub cpu_history: History,
}

struct RawStats {
    cpu_pct: f64,
    mem_usage: u64,
    mem_limit: u64,
    rx_total: u64,
    tx_total: u64,
    pids: u64,
}

pub struct DockerMonitor {
    docker: Option<Docker>,
    pub available: bool,
    pub error: Option<String>,
    pub containers: Vec<ContainerRow>,
    prev_net: HashMap<String, (u64, u64)>,
    histories: HashMap<String, History>,
}

impl DockerMonitor {
    pub async fn connect() -> Self {
        match Docker::connect_with_defaults() {
            Ok(docker) => match docker.ping().await {
                Ok(_) => Self {
                    docker: Some(docker),
                    available: true,
                    error: None,
                    containers: Vec::new(),
                    prev_net: HashMap::new(),
                    histories: HashMap::new(),
                },
                Err(e) => Self::unavailable(format!("docker daemon unreachable: {e}")),
            },
            Err(e) => Self::unavailable(format!("docker connect failed: {e}")),
        }
    }

    fn unavailable(msg: String) -> Self {
        Self {
            docker: None,
            available: false,
            error: Some(msg),
            containers: Vec::new(),
            prev_net: HashMap::new(),
            histories: HashMap::new(),
        }
    }

    pub async fn refresh(&mut self, interval_secs: f64) {
        let Some(docker) = self.docker.clone() else {
            return;
        };

        let opts = ListContainersOptionsBuilder::default().all(true).build();
        let summaries = match docker.list_containers(Some(opts)).await {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(format!("list_containers failed: {e}"));
                return;
            }
        };

        struct Meta {
            id: String,
            name: String,
            image: String,
            status: String,
            running: bool,
        }

        let metas: Vec<Meta> = summaries
            .into_iter()
            .filter_map(|summary| {
                let id = summary.id?;
                if id.is_empty() {
                    return None;
                }
                let short_id = id.chars().take(12).collect::<String>();
                let name = summary
                    .names
                    .as_ref()
                    .and_then(|n| n.first())
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or(short_id);
                let image = summary.image.unwrap_or_default();
                let status = summary.status.unwrap_or_default();
                let running = summary
                    .state
                    .as_ref()
                    .map(|s| format!("{s:?}").eq_ignore_ascii_case("running"))
                    .unwrap_or(false);
                Some(Meta {
                    id,
                    name,
                    image,
                    status,
                    running,
                })
            })
            .collect();

        let stats_futures = metas.iter().map(|m| {
            let docker = docker.clone();
            let id = m.id.clone();
            let running = m.running;
            async move {
                if running {
                    (id.clone(), fetch_stats(&docker, &id).await)
                } else {
                    (id.clone(), None)
                }
            }
        });
        let results: HashMap<String, Option<RawStats>> = join_all(stats_futures).await.into_iter().collect();

        let mut rows = Vec::with_capacity(metas.len());
        for m in metas {
            let raw = results.get(&m.id).and_then(|o| o.as_ref());

            let (cpu_pct, mem_usage, mem_limit, pids) = raw
                .map(|r| (r.cpu_pct, r.mem_usage, r.mem_limit, r.pids))
                .unwrap_or((0.0, 0, 0, 0));

            let (net_rx_bps, net_tx_bps) = if let Some(r) = raw {
                let (prev_rx, prev_tx) = self
                    .prev_net
                    .insert(m.id.clone(), (r.rx_total, r.tx_total))
                    .unwrap_or((r.rx_total, r.tx_total));
                (
                    r.rx_total.saturating_sub(prev_rx) as f64 / interval_secs,
                    r.tx_total.saturating_sub(prev_tx) as f64 / interval_secs,
                )
            } else {
                (0.0, 0.0)
            };

            let hist = self
                .histories
                .entry(m.id.clone())
                .or_insert_with(|| History::new(CONTAINER_HISTORY_LEN));
            hist.push(cpu_pct);

            rows.push(ContainerRow {
                id: m.id,
                name: m.name,
                image: m.image,
                status: m.status,
                running: m.running,
                cpu_pct,
                mem_usage,
                mem_limit,
                net_rx_bps,
                net_tx_bps,
                pids,
                cpu_history: hist.clone(),
            });
        }

        let live_ids: std::collections::HashSet<_> = rows.iter().map(|r| r.id.clone()).collect();
        self.histories.retain(|id, _| live_ids.contains(id));
        self.prev_net.retain(|id, _| live_ids.contains(id));

        rows.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap());
        self.containers = rows;
    }

    pub async fn stop_container(&self, id: &str) -> anyhow::Result<()> {
        if let Some(docker) = &self.docker {
            let opts = StopContainerOptionsBuilder::default().build();
            docker.stop_container(id, Some(opts)).await?;
        }
        Ok(())
    }
}

async fn fetch_stats(docker: &Docker, id: &str) -> Option<RawStats> {
    let opts = StatsOptionsBuilder::default()
        .stream(false)
        .one_shot(false)
        .build();
    let mut stream = docker.stats(id, Some(opts));
    let stats = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .ok()??
        .ok()?;

    let cpu_pct = stats
        .cpu_stats
        .as_ref()
        .zip(stats.precpu_stats.as_ref())
        .map(|(cur, prev)| {
            let cur_total = cur
                .cpu_usage
                .as_ref()
                .and_then(|u| u.total_usage)
                .unwrap_or(0) as f64;
            let prev_total = prev
                .cpu_usage
                .as_ref()
                .and_then(|u| u.total_usage)
                .unwrap_or(0) as f64;
            let cur_sys = cur.system_cpu_usage.unwrap_or(0) as f64;
            let prev_sys = prev.system_cpu_usage.unwrap_or(0) as f64;
            let cpu_delta = cur_total - prev_total;
            let sys_delta = cur_sys - prev_sys;
            let online = cur.online_cpus.unwrap_or(1).max(1) as f64;
            if sys_delta > 0.0 && cpu_delta > 0.0 {
                (cpu_delta / sys_delta) * online * 100.0
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);

    let (mem_usage, mem_limit) = stats
        .memory_stats
        .as_ref()
        .map(|m| {
            let cache = m
                .stats
                .as_ref()
                .and_then(|s| s.get("cache").or_else(|| s.get("inactive_file")))
                .copied()
                .unwrap_or(0);
            let usage = m.usage.unwrap_or(0).saturating_sub(cache);
            (usage, m.limit.unwrap_or(0))
        })
        .unwrap_or((0, 0));

    let (rx_total, tx_total): (u64, u64) = stats
        .networks
        .as_ref()
        .map(|nets| {
            nets.values().fold((0u64, 0u64), |(rx, tx), n| {
                (rx + n.rx_bytes.unwrap_or(0), tx + n.tx_bytes.unwrap_or(0))
            })
        })
        .unwrap_or((0, 0));

    let pids = stats.pids_stats.as_ref().and_then(|p| p.current).unwrap_or(0);

    Some(RawStats {
        cpu_pct,
        mem_usage,
        mem_limit,
        rx_total,
        tx_total,
        pids,
    })
}
