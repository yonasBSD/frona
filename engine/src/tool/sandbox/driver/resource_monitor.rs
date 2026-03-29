use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use dashmap::DashMap;

#[cfg(target_os = "linux")]
use procfs::CurrentSI;

#[derive(Debug, Clone, Default)]
pub struct ResourceUsageSample {
    pub cpu: f64,
    pub mem: f64,
}

#[derive(Default)]
pub struct ResourceUsage {
    agents: DashMap<String, (AtomicU64, AtomicU64)>,
    global_cpu: AtomicU64,
    global_mem: AtomicU64,
    pub max_agent_cpu_pct: f64,
    pub max_agent_memory_pct: f64,
    pub max_total_cpu_pct: f64,
    pub max_total_memory_pct: f64,
}

impl ResourceUsage {
    pub fn new(max_agent_cpu_pct: f64, max_agent_memory_pct: f64, max_total_cpu_pct: f64, max_total_memory_pct: f64) -> Self {
        Self {
            max_agent_cpu_pct,
            max_agent_memory_pct,
            max_total_cpu_pct,
            max_total_memory_pct,
            ..Default::default()
        }
    }

    pub fn track(
        &self,
        agent_id: &str,
        prev: &ResourceUsageSample,
        cur: &ResourceUsageSample,
    ) -> (ResourceUsageSample, ResourceUsageSample) {
        let delta_cpu = to_fixed(cur.cpu - prev.cpu);
        let delta_mem = to_fixed(cur.mem - prev.mem);

        // Ensure entry exists (short write lock), then release it
        if !self.agents.contains_key(agent_id) {
            self.agents.insert(agent_id.to_string(), (AtomicU64::new(0), AtomicU64::new(0)));
        }
        // Read lock only — safe to hold alongside other read locks
        let entry = self.agents.get(agent_id).unwrap();
        let agent_cpu = entry.0.fetch_add(delta_cpu, Relaxed) + delta_cpu;
        let agent_mem = entry.1.fetch_add(delta_mem, Relaxed) + delta_mem;
        drop(entry);

        let global_cpu = self.global_cpu.fetch_add(delta_cpu, Relaxed) + delta_cpu;
        let global_mem = self.global_mem.fetch_add(delta_mem, Relaxed) + delta_mem;

        (
            ResourceUsageSample {
                cpu: from_fixed(agent_cpu),
                mem: from_fixed(agent_mem),
            },
            ResourceUsageSample {
                cpu: from_fixed(global_cpu),
                mem: from_fixed(global_mem),
            },
        )
    }

    pub fn clear_agent(&self, agent_id: &str) {
        if let Some(entry) = self.agents.get(agent_id) {
            let cpu = entry.0.swap(0, Relaxed);
            let mem = entry.1.swap(0, Relaxed);
            drop(entry);
            self.global_cpu.fetch_sub(cpu, Relaxed);
            self.global_mem.fetch_sub(mem, Relaxed);
        }
    }
}

fn to_fixed(v: f64) -> u64 {
    (v.max(0.0) * 1000.0) as u64
}

fn from_fixed(v: u64) -> f64 {
    v as f64 / 1000.0
}

#[cfg(target_os = "linux")]
pub struct ResourceMonitor {
    pid: u32,
    agent_id: String,
    prev: ResourceUsageSample,
    prev_process_ticks: u64,
    prev_system_ticks: u64,
    total_memory_bytes: u64,
}

#[cfg(target_os = "linux")]
impl ResourceMonitor {
    pub fn new(pid: u32, agent_id: String) -> Result<Self, String> {
        let process_ticks = read_tree_ticks(pid);
        let system_ticks = read_system_ticks()?;
        let total_memory_bytes = read_total_memory()?;

        Ok(Self {
            pid,
            agent_id,
            prev: ResourceUsageSample::default(),
            prev_process_ticks: process_ticks,
            prev_system_ticks: system_ticks,
            total_memory_bytes,
        })
    }

    pub fn check(&mut self, usage: &ResourceUsage) -> bool {
        let cur = match self.read_proc() {
            Ok(sample) => sample,
            Err(_) => return false,
        };
        let (a, g) = usage.track(&self.agent_id, &self.prev, &cur);
        self.prev = cur;

        let exceeded = if a.cpu > usage.max_agent_cpu_pct {
            Some(format!("agent CPU {:.1}% > {:.1}%", a.cpu, usage.max_agent_cpu_pct))
        } else if a.mem > usage.max_agent_memory_pct {
            Some(format!("agent memory {:.1}% > {:.1}%", a.mem, usage.max_agent_memory_pct))
        } else if g.cpu > usage.max_total_cpu_pct {
            Some(format!("global CPU {:.1}% > {:.1}%", g.cpu, usage.max_total_cpu_pct))
        } else if g.mem > usage.max_total_memory_pct {
            Some(format!("global memory {:.1}% > {:.1}%", g.mem, usage.max_total_memory_pct))
        } else {
            None
        };

        if let Some(reason) = &exceeded {
            tracing::warn!(
                agent_id = %self.agent_id,
                pid = self.pid,
                "Killing process: {reason}"
            );
        }

        exceeded.is_some()
    }

    fn read_proc(&mut self) -> Result<ResourceUsageSample, String> {
        let tree_pids = collect_tree_pids(self.pid);
        let tree_ticks = read_tree_ticks_from_pids(&tree_pids);
        let tree_rss = read_tree_rss(&tree_pids);
        let system_ticks = read_system_ticks()?;

        let process_delta = tree_ticks.saturating_sub(self.prev_process_ticks);
        let system_delta = system_ticks.saturating_sub(self.prev_system_ticks);

        self.prev_process_ticks = tree_ticks;
        self.prev_system_ticks = system_ticks;

        let cpu_pct = if system_delta > 0 {
            (process_delta as f64 / system_delta as f64) * 100.0
        } else {
            0.0
        };

        let mem_pct = if self.total_memory_bytes > 0 {
            (tree_rss as f64 / self.total_memory_bytes as f64) * 100.0
        } else {
            0.0
        };

        Ok(ResourceUsageSample {
            cpu: cpu_pct,
            mem: mem_pct,
        })
    }
}

#[cfg(target_os = "linux")]
fn collect_tree_pids(root_pid: u32) -> Vec<u32> {
    let mut pids = vec![root_pid];
    let Ok(all) = procfs::process::all_processes() else {
        return pids;
    };
    for entry in all.flatten() {
        if let Ok(stat) = entry.stat() {
            if stat.ppid > 0 && pids.contains(&(stat.ppid as u32)) {
                pids.push(stat.pid as u32);
            }
        }
    }
    pids
}

#[cfg(target_os = "linux")]
fn read_tree_ticks(root_pid: u32) -> u64 {
    let pids = collect_tree_pids(root_pid);
    read_tree_ticks_from_pids(&pids)
}

#[cfg(target_os = "linux")]
fn read_tree_ticks_from_pids(pids: &[u32]) -> u64 {
    let mut total = 0u64;
    for &pid in pids {
        if let Ok(proc) = procfs::process::Process::new(pid as i32) {
            if let Ok(stat) = proc.stat() {
                total += stat.utime + stat.stime;
            }
        }
    }
    total
}

#[cfg(target_os = "linux")]
fn read_tree_rss(pids: &[u32]) -> u64 {
    let mut total = 0u64;
    for &pid in pids {
        if let Ok(proc) = procfs::process::Process::new(pid as i32) {
            if let Ok(stat) = proc.stat() {
                total += stat.rss * procfs::page_size();
            }
        }
    }
    total
}

#[cfg(target_os = "linux")]
fn read_system_ticks() -> Result<u64, String> {
    let stats = procfs::KernelStats::current().map_err(|e| format!("read /proc/stat: {e}"))?;
    let cpu = &stats.total;
    Ok(cpu.user + cpu.nice + cpu.system + cpu.idle
        + cpu.iowait.unwrap_or(0)
        + cpu.irq.unwrap_or(0)
        + cpu.softirq.unwrap_or(0))
}

fn effective_total_memory() -> u64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.cgroup_limits()
        .map(|cg| cg.total_memory)
        .unwrap_or_else(|| sys.total_memory())
}

#[cfg(target_os = "linux")]
fn read_total_memory() -> Result<u64, String> {
    Ok(effective_total_memory())
}

pub fn log_system_resources() {
    use sysinfo::System;
    let cpus = System::physical_core_count().unwrap_or(0);
    let mem_gb = effective_total_memory() as f64 / 1_073_741_824.0;
    tracing::info!("System resources: {cpus} CPUs, {mem_gb:.1} GB memory");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_usage_tracks_deltas() {
        let ru = ResourceUsage::default();
        let prev = ResourceUsageSample { cpu: 0.0, mem: 0.0 };
        let cur = ResourceUsageSample { cpu: 10.0, mem: 5.0 };

        let (agent, global) = ru.track("agent_1", &prev, &cur);
        assert!((agent.cpu - 10.0).abs() < 0.1);
        assert!((agent.mem - 5.0).abs() < 0.1);
        assert!((global.cpu - 10.0).abs() < 0.1);
        assert!((global.mem - 5.0).abs() < 0.1);
    }

    #[test]
    fn test_resource_usage_accumulates() {
        let ru = ResourceUsage::default();

        let prev = ResourceUsageSample { cpu: 0.0, mem: 0.0 };
        let cur = ResourceUsageSample { cpu: 10.0, mem: 5.0 };
        ru.track("agent_1", &prev, &cur);

        let prev2 = ResourceUsageSample { cpu: 10.0, mem: 5.0 };
        let cur2 = ResourceUsageSample { cpu: 25.0, mem: 12.0 };
        let (agent, _) = ru.track("agent_1", &prev2, &cur2);

        assert!((agent.cpu - 25.0).abs() < 0.1);
        assert!((agent.mem - 12.0).abs() < 0.1);
    }

    #[test]
    fn test_resource_usage_multiple_agents() {
        let ru = ResourceUsage::default();
        let zero = ResourceUsageSample { cpu: 0.0, mem: 0.0 };

        let cur1 = ResourceUsageSample { cpu: 30.0, mem: 20.0 };
        let cur2 = ResourceUsageSample { cpu: 40.0, mem: 10.0 };

        let (a1, g1) = ru.track("agent_1", &zero, &cur1);
        let (a2, g2) = ru.track("agent_2", &zero, &cur2);

        assert!((a1.cpu - 30.0).abs() < 0.1);
        assert!((a2.cpu - 40.0).abs() < 0.1);
        assert!((g1.cpu - 30.0).abs() < 0.1);
        assert!((g2.cpu - 70.0).abs() < 0.1);
    }

    #[test]
    fn test_resource_usage_negative_delta_clamped() {
        let ru = ResourceUsage::default();
        let prev = ResourceUsageSample { cpu: 10.0, mem: 5.0 };
        let cur = ResourceUsageSample { cpu: 5.0, mem: 2.0 };

        let (agent, _) = ru.track("agent_1", &prev, &cur);
        assert!((agent.cpu - 0.0).abs() < 0.1);
        assert!((agent.mem - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_fixed_point_roundtrip() {
        let values = [0.0, 1.5, 99.999, 100.0, 0.001];
        for v in values {
            let rt = from_fixed(to_fixed(v));
            assert!((rt - v).abs() < 0.002, "roundtrip failed for {v}: got {rt}");
        }
    }
}
