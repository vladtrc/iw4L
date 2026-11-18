pub fn prefer_performance_cores() -> Option<PerformanceCorePlan> {
    let plan = performance_core_plan(
        &std::fs::read_to_string("/sys/devices/cpu_core/cpus").ok()?,
        &current_affinity()?,
    )?;
    set_affinity(&plan.chosen)?;
    Some(plan)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerformanceCorePlan {
    pub performance: Vec<usize>,

    pub allowed: Vec<usize>,

    pub chosen: Vec<usize>,
}

fn performance_core_plan(cpu_core_list: &str, allowed: &[usize]) -> Option<PerformanceCorePlan> {
    let performance = parse_cpu_list(cpu_core_list);
    let chosen: Vec<usize> = allowed
        .iter()
        .copied()
        .filter(|cpu| performance.contains(cpu))
        .collect();
    (!chosen.is_empty() && chosen.len() < allowed.len()).then(|| PerformanceCorePlan {
        performance,
        allowed: allowed.to_vec(),
        chosen,
    })
}

fn parse_cpu_list(list: &str) -> Vec<usize> {
    let mut cpus = Vec::new();
    for part in list.trim().split(',').filter(|part| !part.is_empty()) {
        let mut ends = part.split('-').map(str::parse::<usize>);
        match (ends.next(), ends.next(), ends.next()) {
            (Some(Ok(only)), None, None) => cpus.push(only),
            (Some(Ok(first)), Some(Ok(last)), None) if first <= last => cpus.extend(first..=last),
            _ => return Vec::new(),
        }
    }
    cpus.sort_unstable();
    cpus.dedup();
    cpus
}

#[cfg(target_os = "linux")]
fn current_affinity() -> Option<Vec<usize>> {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        (libc::sched_getaffinity(0, size_of::<libc::cpu_set_t>(), &raw mut set) == 0).then(|| {
            (0..libc::CPU_SETSIZE as usize)
                .filter(|cpu| libc::CPU_ISSET(*cpu, &set))
                .collect()
        })
    }
}

#[cfg(target_os = "linux")]
fn set_affinity(cpus: &[usize]) -> Option<()> {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        for cpu in cpus {
            if *cpu >= libc::CPU_SETSIZE as usize {
                return None;
            }
            libc::CPU_SET(*cpu, &mut set);
        }
        (libc::sched_setaffinity(0, size_of::<libc::cpu_set_t>(), &raw const set) == 0)
            .then_some(())
    }
}

#[cfg(not(target_os = "linux"))]
fn current_affinity() -> Option<Vec<usize>> {
    None
}

#[cfg(not(target_os = "linux"))]
fn set_affinity(_cpus: &[usize]) -> Option<()> {
    None
}
