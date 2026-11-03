//! Per-process dedicated VRAM from the Windows "GPU Process Memory" performance counters.
//!
//! Ollama's `/api/ps` `size_vram` under-reports Gemma models (about 0.3 GB reported
//! against 3 to 4.5 GB really held by the runner), so the engine panel reads the
//! numbers from the OS instead. Sampling is read-only and takes about 10 ms.
//!
//! Only processes that descend from this app are counted, so a foreign
//! `llama-server.exe` (LM Studio, koboldcpp, another Ollama) never shows up as ours.

use std::collections::HashMap;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MAX_PARENT_HOPS: usize = 8;

/// Dedicated VRAM held by the refinement runner and the Whisper server, in GB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProcessVram {
    pub(crate) refine_gb: Option<f64>,
    pub(crate) whisper_gb: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    Refine,
    Whisper,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessInfo {
    exe: String,
    parent: u32,
}

fn owner_of(exe_name: &str) -> Option<Owner> {
    match exe_name.to_ascii_lowercase().as_str() {
        "llama-server.exe" | "ollama.exe" => Some(Owner::Refine),
        "whisper-server.exe" => Some(Owner::Whisper),
        _ => None,
    }
}

/// Parses the process id out of a counter instance such as `pid_30476_luid_0x0_0x1_phys_0`.
fn pid_of_instance(instance: &str) -> Option<u32> {
    instance
        .strip_prefix("pid_")?
        .split('_')
        .next()?
        .parse::<u32>()
        .ok()
}

/// True when `pid` is a (grand)child of `root`. The hop limit also ends parent
/// cycles, which a recycled pid can produce.
fn descends_from(pid: u32, root: u32, table: &HashMap<u32, ProcessInfo>) -> bool {
    let mut current = pid;
    for _ in 0..MAX_PARENT_HOPS {
        let Some(info) = table.get(&current) else {
            return false;
        };
        if info.parent == root {
            return true;
        }
        current = info.parent;
    }
    false
}

/// Sums the usage of every process of this app that belongs to one owner. `None`
/// when no such process holds VRAM, so the panel shows a dash instead of a
/// misleading zero.
fn sum_by_owner(
    usage_by_pid: &HashMap<u32, u64>,
    table: &HashMap<u32, ProcessInfo>,
    root_pid: u32,
) -> ProcessVram {
    let mut refine: Option<u64> = None;
    let mut whisper: Option<u64> = None;
    for (pid, bytes) in usage_by_pid {
        let Some(info) = table.get(pid) else {
            continue;
        };
        let slot = match owner_of(&info.exe) {
            Some(Owner::Refine) => &mut refine,
            Some(Owner::Whisper) => &mut whisper,
            None => continue,
        };
        if !descends_from(*pid, root_pid, table) {
            continue;
        }
        *slot = Some(slot.unwrap_or(0) + bytes);
    }
    ProcessVram {
        refine_gb: refine.map(|b| b as f64 / GIB),
        whisper_gb: whisper.map(|b| b as f64 / GIB),
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn sample() -> Option<ProcessVram> {
    sample_for_root(std::process::id())
}

#[cfg(target_os = "windows")]
fn sample_for_root(root_pid: u32) -> Option<ProcessVram> {
    let usage = dedicated_usage_by_pid()?;
    let table = process_table()?;
    Some(sum_by_owner(&usage, &table, root_pid))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn sample() -> Option<ProcessVram> {
    None
}

#[cfg(target_os = "windows")]
fn dedicated_usage_by_pid() -> Option<HashMap<u32, u64>> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY,
    };

    const PDH_OK: u32 = 0;
    const PDH_MORE_DATA: u32 = 0x800007D2;
    // A process may start between the size probe and the fetch, so retry on MORE_DATA.
    const FETCH_ATTEMPTS: usize = 3;

    unsafe {
        let mut query = PDH_HQUERY::default();
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != PDH_OK {
            return None;
        }
        let result = (|| {
            let mut counter = PDH_HCOUNTER::default();
            if PdhAddEnglishCounterW(
                query,
                w!("\\GPU Process Memory(*)\\Dedicated Usage"),
                0,
                &mut counter,
            ) != PDH_OK
            {
                return None;
            }
            if PdhCollectQueryData(query) != PDH_OK {
                return None;
            }

            let mut bytes_needed = 0u32;
            let mut item_count = 0u32;
            let probe = PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &mut bytes_needed,
                &mut item_count,
                None,
            );
            if probe != PDH_MORE_DATA || bytes_needed == 0 {
                return None;
            }

            // u64 backing keeps the buffer aligned for PDH_FMT_COUNTERVALUE_ITEM_W.
            let mut buffer: Vec<u64> = Vec::new();
            let mut fetched = false;
            for _ in 0..FETCH_ATTEMPTS {
                // Reserve a quarter on top for processes that appeared since the probe.
                let size = bytes_needed as usize + bytes_needed as usize / 4 + 256;
                buffer = vec![0u64; size.div_ceil(8)];
                let mut buffer_bytes = (buffer.len() * 8) as u32;
                let items = buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                let status = PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_LARGE,
                    &mut buffer_bytes,
                    &mut item_count,
                    Some(items),
                );
                if status == PDH_OK {
                    fetched = true;
                    break;
                }
                if status != PDH_MORE_DATA {
                    return None;
                }
                bytes_needed = buffer_bytes.max(bytes_needed);
            }
            if !fetched {
                return None;
            }

            let items = buffer.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W;
            let mut by_pid: HashMap<u32, u64> = HashMap::new();
            for i in 0..item_count as usize {
                let item = &*items.add(i);
                let Ok(instance) = item.szName.to_string() else {
                    continue;
                };
                let Some(pid) = pid_of_instance(&instance) else {
                    continue;
                };
                let value = item.FmtValue.Anonymous.largeValue;
                if value > 0 {
                    *by_pid.entry(pid).or_insert(0) += value as u64;
                }
            }
            Some(by_pid)
        })();
        let _ = PdhCloseQuery(query);
        result
    }
}

#[cfg(target_os = "windows")]
fn process_table() -> Option<HashMap<u32, ProcessInfo>> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut table = HashMap::new();
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                table.insert(
                    entry.th32ProcessID,
                    ProcessInfo {
                        exe: String::from_utf16_lossy(&entry.szExeFile[..len]),
                        parent: entry.th32ParentProcessID,
                    },
                );
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        Some(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;
    const APP: u32 = 1;

    fn proc(exe: &str, parent: u32) -> ProcessInfo {
        ProcessInfo {
            exe: exe.to_string(),
            parent,
        }
    }

    #[test]
    fn parses_the_pid_out_of_a_counter_instance() {
        assert_eq!(
            pid_of_instance("pid_30476_luid_0x00000000_0x0000B7A7_phys_0"),
            Some(30476)
        );
        assert_eq!(pid_of_instance("pid_7"), Some(7));
        assert_eq!(pid_of_instance("luid_0x0_phys_0"), None);
        assert_eq!(pid_of_instance("pid_abc_phys_0"), None);
    }

    #[test]
    fn maps_runner_and_whisper_executables() {
        assert_eq!(owner_of("llama-server.exe"), Some(Owner::Refine));
        assert_eq!(owner_of("Ollama.EXE"), Some(Owner::Refine));
        assert_eq!(owner_of("whisper-server.exe"), Some(Owner::Whisper));
        assert_eq!(owner_of("msedgewebview2.exe"), None);
    }

    #[test]
    fn sums_per_owner_for_children_and_grandchildren_of_the_app() {
        // app(1) -> ollama serve(10) -> llama-server(11); app(1) -> whisper(20)
        let table = HashMap::from([
            (APP, proc("trispr-flow.exe", 0)),
            (10, proc("ollama.exe", APP)),
            (11, proc("llama-server.exe", 10)),
            (20, proc("whisper-server.exe", APP)),
            (30, proc("game.exe", 99)),
        ]);
        let usage = HashMap::from([(11, 3 * GB), (10, GB), (20, GB + GB / 2), (30, 5 * GB)]);
        let vram = sum_by_owner(&usage, &table, APP);
        assert_eq!(vram.refine_gb, Some(4.0));
        assert_eq!(vram.whisper_gb, Some(1.5));
    }

    #[test]
    fn a_foreign_runner_outside_the_app_tree_is_ignored() {
        // LM Studio's llama-server(40) and a foreign whisper-server(41) hold VRAM too.
        let table = HashMap::from([
            (APP, proc("trispr-flow.exe", 0)),
            (40, proc("llama-server.exe", 77)),
            (41, proc("whisper-server.exe", 77)),
            (77, proc("lmstudio.exe", 5)),
        ]);
        let usage = HashMap::from([(40, 20 * GB), (41, 2 * GB)]);
        assert_eq!(
            sum_by_owner(&usage, &table, APP),
            ProcessVram {
                refine_gb: None,
                whisper_gb: None
            }
        );
    }

    #[test]
    fn a_parent_cycle_ends_instead_of_looping() {
        let table = HashMap::from([
            (50, proc("llama-server.exe", 51)),
            (51, proc("something.exe", 50)),
        ]);
        assert!(!descends_from(50, APP, &table));
    }

    #[test]
    fn missing_owner_gives_none_instead_of_zero() {
        let table = HashMap::from([(30, proc("game.exe", APP))]);
        let usage = HashMap::from([(30, 5 * GB)]);
        assert_eq!(
            sum_by_owner(&usage, &table, APP),
            ProcessVram {
                refine_gb: None,
                whisper_gb: None
            }
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "needs a WDDM GPU; run with --ignored --nocapture"]
    fn live_sample_reads_the_counters() {
        // The test binary is not the app, so root the tree at a running trispr-flow.exe.
        let table = process_table().expect("process snapshot");
        let app_pid = table
            .iter()
            .find(|(_, info)| info.exe.eq_ignore_ascii_case("trispr-flow.exe"))
            .map(|(pid, _)| *pid);
        eprintln!("app pid: {app_pid:?}");
        let root = app_pid.unwrap_or(std::process::id());
        let vram = sample_for_root(root);
        eprintln!("live sample: {vram:?}");
        let started = std::time::Instant::now();
        for _ in 0..20 {
            let _ = sample_for_root(root);
        }
        eprintln!("avg sample cost: {:?}", started.elapsed() / 20);
        assert!(vram.is_some());
    }
}
