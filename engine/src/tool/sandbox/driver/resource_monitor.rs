use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;

use dashmap::DashMap;
use sysinfo::{MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct AgentLimits {
    pub cpu_pct: f64,
    pub mem_pct: f64,
}

pub struct TrackedProcess {
    pub agent_id: String,
    pub killed: AtomicBool,
}

pub struct SystemResourceManager {
    pub max_agent_cpu_pct: f64,
    pub max_agent_memory_pct: f64,
    pub max_total_cpu_pct: f64,
    pub max_total_memory_pct: f64,
    agent_limits: DashMap<String, AgentLimits>,
    tracked: DashMap<u32, TrackedProcess>,
    cancel_token: CancellationToken,
}

impl SystemResourceManager {
    pub fn new(
        max_agent_cpu_pct: f64,
        max_agent_memory_pct: f64,
        max_total_cpu_pct: f64,
        max_total_memory_pct: f64,
    ) -> Self {
        Self {
            max_agent_cpu_pct,
            max_agent_memory_pct,
            max_total_cpu_pct,
            max_total_memory_pct,
            agent_limits: DashMap::new(),
            tracked: DashMap::new(),
            cancel_token: CancellationToken::new(),
        }
    }

    pub fn set_agent_limits(&self, agent_id: &str, cpu_pct: Option<f64>, mem_pct: Option<f64>) {
        if cpu_pct.is_some() || mem_pct.is_some() {
            self.agent_limits.insert(
                agent_id.to_string(),
                AgentLimits {
                    cpu_pct: cpu_pct.unwrap_or(self.max_agent_cpu_pct),
                    mem_pct: mem_pct.unwrap_or(self.max_agent_memory_pct),
                },
            );
        }
    }

    pub fn effective_agent_limits(&self, agent_id: &str) -> (f64, f64) {
        match self.agent_limits.get(agent_id) {
            Some(l) => (l.cpu_pct, l.mem_pct),
            None => (self.max_agent_cpu_pct, self.max_agent_memory_pct),
        }
    }

    pub fn register(&self, pid: u32, agent_id: &str) {
        self.tracked.insert(
            pid,
            TrackedProcess {
                agent_id: agent_id.to_string(),
                killed: AtomicBool::new(false),
            },
        );
    }

    pub fn unregister(&self, pid: u32) {
        self.tracked.remove(&pid);
    }

    pub fn is_killed(&self, pid: u32) -> bool {
        self.tracked
            .get(&pid)
            .map(|p| p.killed.load(Relaxed))
            .unwrap_or(false)
    }

    pub fn stop_polling(&self) {
        self.cancel_token.cancel();
    }

    pub fn start_polling(self: &Arc<Self>) -> JoinHandle<()> {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let mut sys = System::new();
            sys.refresh_memory();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));

            loop {
                tokio::select! {
                    biased;
                    _ = manager.cancel_token.cancelled() => break,
                    _ = interval.tick() => {
                        manager.poll_once(&mut sys);
                    }
                }
            }
        })
    }

    fn poll_once(&self, sys: &mut System) {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        sys.refresh_cpu_usage();
        sys.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());

        if self.tracked.is_empty() {
            return;
        }

        let total_memory = sys.total_memory();

        let mut children_map: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, process) in sys.processes() {
            if let Some(parent) = process.parent() {
                children_map.entry(parent).or_default().push(*pid);
            }
        }

        let mut per_pid: HashMap<u32, (f64, u64)> = HashMap::new();
        let mut dead_pids: Vec<u32> = Vec::new();

        for entry in self.tracked.iter() {
            let pid = *entry.key();
            let sysinfo_pid = Pid::from_u32(pid);

            if sys.process(sysinfo_pid).is_none() {
                dead_pids.push(pid);
                continue;
            }

            if total_memory > 0 {
                let (cpu, mem_bytes) = collect_tree_usage(sys, sysinfo_pid, &children_map);
                per_pid.insert(pid, (cpu, mem_bytes));
            }
        }

        for pid in &dead_pids {
            self.tracked.remove(pid);
        }

        if total_memory == 0 {
            return;
        }

        let mut pid_usage: Vec<(u32, String, f64, f64, u64)> = Vec::new();
        for entry in self.tracked.iter() {
            let pid = *entry.key();
            let tracked = entry.value();
            if tracked.killed.load(Relaxed) {
                continue;
            }
            if let Some(&(cpu, mem_bytes)) = per_pid.get(&pid) {
                let mem_pct = (mem_bytes as f64 / total_memory as f64) * 100.0;
                pid_usage.push((pid, tracked.agent_id.clone(), cpu, mem_pct, mem_bytes));
            }
        }

        self.enforce_agent_limits(&pid_usage);
        self.enforce_global_limits(sys, total_memory, &pid_usage);
    }

    fn enforce_agent_limits(&self, pid_usage: &[(u32, String, f64, f64, u64)]) {
        let mut agent_ids: Vec<String> = pid_usage.iter().map(|(_, a, _, _, _)| a.clone()).collect();
        agent_ids.sort();
        agent_ids.dedup();

        for agent_id in &agent_ids {
            let (max_cpu, max_mem) = self.effective_agent_limits(agent_id);

            loop {
                let mut total_cpu = 0.0f64;
                let mut total_mem = 0.0f64;
                let mut largest_mem_pid: Option<(u32, u64)> = None;
                let mut largest_cpu_pid: Option<(u32, f64)> = None;

                for &(pid, ref aid, cpu, mem_pct, mem_bytes) in pid_usage {
                    if aid != agent_id {
                        continue;
                    }
                    if self.tracked.get(&pid).is_some_and(|t| t.killed.load(Relaxed)) {
                        continue;
                    }
                    total_cpu += cpu;
                    total_mem += mem_pct;
                    if largest_mem_pid.is_none_or(|(_, prev)| mem_bytes > prev) {
                        largest_mem_pid = Some((pid, mem_bytes));
                    }
                    if largest_cpu_pid.is_none_or(|(_, prev)| cpu > prev) {
                        largest_cpu_pid = Some((pid, cpu));
                    }
                }

                if total_mem > max_mem
                    && let Some((pid, _)) = largest_mem_pid
                {
                    tracing::warn!(
                        pid, agent = %agent_id,
                        "Killing process: agent memory {total_mem:.1}% > {max_mem:.1}%"
                    );
                    if let Some(entry) = self.tracked.get(&pid) {
                        entry.killed.store(true, Relaxed);
                    }
                    kill_process(pid);
                    continue;
                } else if total_cpu > max_cpu
                    && let Some((pid, _)) = largest_cpu_pid
                {
                    tracing::warn!(
                        pid, agent = %agent_id,
                        "Killing process: agent CPU {total_cpu:.1}% > {max_cpu:.1}%"
                    );
                    if let Some(entry) = self.tracked.get(&pid) {
                        entry.killed.store(true, Relaxed);
                    }
                    kill_process(pid);
                    continue;
                }
                break;
            }
        }
    }

    fn enforce_global_limits(
        &self,
        sys: &System,
        total_memory: u64,
        pid_usage: &[(u32, String, f64, f64, u64)],
    ) {
        let global_cpu = sys.global_cpu_usage() as f64;
        let global_mem = (sys.used_memory() as f64 / total_memory as f64) * 100.0;

        if global_cpu <= self.max_total_cpu_pct && global_mem <= self.max_total_memory_pct {
            return;
        }

        let exceeded_cpu = global_cpu > self.max_total_cpu_pct;
        let reason = if exceeded_cpu {
            format!("global CPU {global_cpu:.1}% > {:.1}%", self.max_total_cpu_pct)
        } else {
            format!("global memory {global_mem:.1}% > {:.1}%", self.max_total_memory_pct)
        };

        let mut candidates: Vec<(u32, String, f64, u64)> = pid_usage.iter()
            .filter(|(pid, _, _, _, _)| {
                !self.tracked.get(pid).is_some_and(|t| t.killed.load(Relaxed))
            })
            .map(|&(pid, ref aid, cpu, _, mem_bytes)| (pid, aid.clone(), cpu, mem_bytes))
            .collect();

        if exceeded_cpu {
            candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            candidates.sort_by(|a, b| b.3.cmp(&a.3));
        }

        if let Some((pid, agent_id, _, _)) = candidates.into_iter().next() {
            tracing::warn!(pid, agent = %agent_id, "Killing process: {reason}");
            if let Some(entry) = self.tracked.get(&pid) {
                entry.killed.store(true, Relaxed);
            }
            kill_process(pid);
        }
    }
}

fn collect_tree_usage(
    sys: &System,
    root: Pid,
    children_map: &HashMap<Pid, Vec<Pid>>,
) -> (f64, u64) {
    let mut total_cpu = 0.0f64;
    let mut total_mem = 0u64;
    let mut stack = vec![root];

    while let Some(pid) = stack.pop() {
        if let Some(process) = sys.process(pid) {
            total_cpu += process.cpu_usage() as f64;
            total_mem += process.memory();
        }
        if let Some(children) = children_map.get(&pid) {
            stack.extend(children);
        }
    }

    (total_cpu, total_mem)
}

fn kill_process(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        tracing::warn!("Process kill not supported on this platform");
    }
}

pub fn log_system_resources() {
    use sysinfo::System;
    let cpus = System::physical_core_count().unwrap_or(0);
    let mem_gb = effective_total_memory() as f64 / 1_073_741_824.0;
    tracing::info!("System resources: {cpus} CPUs, {mem_gb:.1} GB memory");
}

fn effective_total_memory() -> u64 {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.cgroup_limits()
        .map(|cg| cg.total_memory)
        .unwrap_or_else(|| sys.total_memory())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_unregister() {
        let manager = SystemResourceManager::new(80.0, 80.0, 90.0, 90.0);
        manager.register(1234, "agent_1");
        assert!(manager.tracked.contains_key(&1234));
        manager.unregister(1234);
        assert!(!manager.tracked.contains_key(&1234));
    }

    #[test]
    fn test_is_killed_default_false() {
        let manager = SystemResourceManager::new(80.0, 80.0, 90.0, 90.0);
        manager.register(1234, "agent_1");
        assert!(!manager.is_killed(1234));
    }

    #[test]
    fn test_is_killed_unregistered() {
        let manager = SystemResourceManager::new(80.0, 80.0, 90.0, 90.0);
        assert!(!manager.is_killed(9999));
    }

    #[test]
    fn test_set_agent_limits() {
        let manager = SystemResourceManager::new(80.0, 80.0, 90.0, 90.0);
        manager.set_agent_limits("agent_1", Some(50.0), Some(60.0));

        let limits = manager.agent_limits.get("agent_1").unwrap();
        assert!((limits.cpu_pct - 50.0).abs() < f64::EPSILON);
        assert!((limits.mem_pct - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_set_agent_limits_partial() {
        let manager = SystemResourceManager::new(80.0, 80.0, 90.0, 90.0);
        manager.set_agent_limits("agent_1", Some(50.0), None);

        let limits = manager.agent_limits.get("agent_1").unwrap();
        assert!((limits.cpu_pct - 50.0).abs() < f64::EPSILON);
        assert!((limits.mem_pct - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_set_agent_limits_no_overrides() {
        let manager = SystemResourceManager::new(80.0, 80.0, 90.0, 90.0);
        manager.set_agent_limits("agent_1", None, None);
        assert!(!manager.agent_limits.contains_key("agent_1"));
    }

    #[tokio::test]
    async fn test_stop_polling() {
        let manager = Arc::new(SystemResourceManager::new(80.0, 80.0, 90.0, 90.0));
        let handle = manager.start_polling();
        manager.stop_polling();
        handle.await.unwrap();
    }

    #[test]
    fn test_poll_once_auto_cleans_dead_pids() {
        let manager = SystemResourceManager::new(80.0, 80.0, 90.0, 90.0);
        manager.register(999_999_999, "agent_1");
        assert!(manager.tracked.contains_key(&999_999_999));

        let mut sys = System::new();
        manager.poll_once(&mut sys);

        assert!(!manager.tracked.contains_key(&999_999_999));
    }

    #[test]
    fn test_collect_tree_usage_empty() {
        let sys = System::new();
        let children_map = HashMap::new();
        let (cpu, mem) = collect_tree_usage(&sys, Pid::from_u32(999_999_999), &children_map);
        assert!((cpu - 0.0).abs() < f64::EPSILON);
        assert_eq!(mem, 0);
    }

}
