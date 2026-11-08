//! CPU placement: on a processor whose L3 cache is split into domains of different sizes
//! (AMD's dual-CCD X3D parts: one CCD with the stacked cache, one without), the game runs
//! on the logical processors that share the largest L3, and Bevy's task pools are sized to
//! them. docs/design/frame-rate-audit.md §6 has the measurements and the rules (#1842).
//!
//! **When.** Only on Windows, decided once in `main()` before any thread pool or other
//! thread of the game starts, so every thread made later (Bevy's pools, the render
//! thread, audio, the read pool, the driver's threads) starts there too. Nothing is done
//! unless all hold ([`decide`]): one processor group; two or more L3 domains; one domain
//! strictly larger than every other; that domain has at least [`MIN_LPS`] logical
//! processors and at least as many as every other domain (the game never trades threads
//! for cache: every dual-CCD X3D part has two CCDs of one core count, while e.g. a Ryzen
//! AI 300 "Strix Point" laptop part, whose 4 full cores share 16 MiB and 8 compact cores
//! 8 MiB [hypothesis: from AMD's public specifications, not checked here], is left alone);
//! nobody has restricted the process already (its affinity mask is the
//! system's and it has no default CPU sets: `start /affinity`, Process Lasso or the
//! player chose, and that choice is left alone). Equal L3s (single-CCD parts, non-X3D
//! dual-CCD parts, Intel, Threadripper) are left as they are: on every such machine no
//! affinity or CPU-set call is made and Bevy's task pools are sized exactly as without
//! this module (only the log line is new).
//!
//! **How: CPU sets, not the affinity mask.** `SetProcessDefaultCpuSets` is Microsoft's
//! documented way for a game to say where its threads should run; the affinity mask
//! (`SetProcessAffinityMask`) is the older hard limit. CPU sets were chosen because they
//! are not inherited by processes this one starts (the load hand-off's relaunch,
//! handoff.rs, decides again for itself), they leave the process's affinity mask as the
//! system's (so the "already restricted" test above still means the player chose), and the
//! system may still use the processors it reserves for itself as it needs. The
//! measurements behind this (#1416; again 2026-11-08) used a hard affinity mask set on the
//! parent; that CPU sets give the same frame times is to be measured [hypothesis]. The
//! main thread, already running, is given the same sets itself, so it does not depend on
//! whether the process default reaches a thread that already exists.
//!
//! **The setting.** `cpu-cache-placement` in `[Settings]` of NVForever.ini, `auto` (the
//! default) or `off`, read at start only; `--cpu-cache-placement auto|off` on the command
//! line wins over it for that run (for A/B measurements). Unattended runs (test aids) read
//! the INI only in their own home, as every setting (`nvf_play::settings`).
//!
//! The startup log has one line starting `cpu placement:` saying what was found and done
//! (a smoke scenario matches it; keep the wording).

// Elsewhere than Windows only the setting and its line are used; the rules stay compiled
// and tested there.
#![cfg_attr(not(windows), allow(dead_code))]

use std::fmt::Write;

/// The key in `[Settings]` of NVForever.ini.
pub const INI_KEY: &str = "cpu-cache-placement";

/// The fewest logical processors a domain needs for the game to be put on it: fewer would
/// crowd the main and render threads with the game's own threads and the I/O and async
/// pools [hypothesis; docs/design/frame-rate-audit.md §6].
pub const MIN_LPS: u32 = 8;

/// The setting's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
    /// Put the game on the largest L3 domain when the rules allow (module notes).
    Auto,
    /// Leave the process on every logical processor.
    Off,
}

impl Mode {
    /// An INI value: `auto` or `off`, any case; `None` for anything else.
    fn parse(v: &str) -> Option<Mode> {
        match v.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Mode::Auto),
            "off" => Some(Mode::Off),
            _ => None,
        }
    }
}

/// The run's mode and where it came from: the command line wins, then the INI's value,
/// then `auto`. A value the INI holds that is not `auto`/`off` is skipped with a warning.
fn mode_of(cli: Option<Mode>, ini: Option<&str>) -> (Mode, &'static str, Option<String>) {
    if let Some(m) = cli {
        return (m, "--cpu-cache-placement", None);
    }
    match ini {
        Some(v) => match Mode::parse(v) {
            Some(m) => (m, "NVForever.ini", None),
            None => (Mode::Auto, "default", Some(format!("cpu placement: [Settings] {INI_KEY}={v} not understood (auto or off); auto"))),
        },
        None => (Mode::Auto, "default", None),
    }
}

/// One L3 cache: its size and the logical processors sharing it (bit n: processor n of
/// processor group 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Domain {
    pub bytes: u64,
    pub mask: u64,
}

impl Domain {
    pub fn lps(&self) -> u32 {
        self.mask.count_ones()
    }
}

/// The processor's L3 caches as read from the system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Topology {
    /// Each L3 once, largest first ([`Topology::new`]).
    pub domains: Vec<Domain>,
    /// More than one processor group, or a cache outside group 0: masks of one group do
    /// not cover the machine, so nothing is done.
    pub multi_group: bool,
}

impl Topology {
    /// Caches listed more than once kept once; ordered largest first, then by the lowest
    /// processor.
    pub fn new(mut domains: Vec<Domain>, multi_group: bool) -> Topology {
        domains.retain(|d| d.mask != 0);
        domains.sort_by_key(|d| (std::cmp::Reverse(d.bytes), d.mask.trailing_zeros(), d.mask));
        domains.dedup();
        Topology { domains, multi_group }
    }
}

/// What to do with a topology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    MultiGroup,
    /// No L3 found, or one.
    OneDomain,
    /// Every L3 the same size.
    SameSize,
    /// Two or more share the largest size, with a smaller one beside them.
    NoSingleLargest,
    /// The largest has fewer than [`MIN_LPS`] logical processors (how many it has).
    TooSmall(u32),
    /// Another domain has more logical processors than the largest L3's (its count, the
    /// most any other has): moving there would give up threads.
    FewerThreads(u32, u32),
    /// Someone already chose where the process runs.
    AlreadySet,
    /// Put the process on this domain.
    Restrict(Domain),
}

impl Decision {
    /// Whether anything is to be done: only [`Decision::Restrict`]. Every other result
    /// makes no affinity or CPU-set call and leaves Bevy's pools as Bevy sizes them.
    pub fn domain(&self) -> Option<Domain> {
        match self {
            Decision::Restrict(d) => Some(*d),
            _ => None,
        }
    }

    /// Bevy's task pools' thread budget for this result; `None`: Bevy's own sizing, the
    /// same `TaskPoolOptions` as without placement.
    pub fn pool_threads(&self) -> Option<usize> {
        self.domain().as_ref().map(pool_threads)
    }
}

/// The rules of the module notes, in order; `already_set`: the process's affinity mask
/// differs from the system's or it has default CPU sets.
pub fn decide(topology: &Topology, already_set: bool) -> Decision {
    if topology.multi_group {
        return Decision::MultiGroup;
    }
    let d = &topology.domains;
    if d.len() < 2 {
        return Decision::OneDomain;
    }
    // Largest first (Topology::new).
    let largest = d[0];
    if d.iter().all(|x| x.bytes == largest.bytes) {
        return Decision::SameSize;
    }
    if d[1].bytes == largest.bytes {
        return Decision::NoSingleLargest;
    }
    if largest.lps() < MIN_LPS {
        return Decision::TooSmall(largest.lps());
    }
    let most_elsewhere = d[1..].iter().map(|x| x.lps()).max().unwrap_or(0);
    if most_elsewhere > largest.lps() {
        return Decision::FewerThreads(largest.lps(), most_elsewhere);
    }
    if already_set {
        return Decision::AlreadySet;
    }
    Decision::Restrict(largest)
}

/// Bevy's task pools' thread budget on a domain: one thread per logical processor of it
/// (Bevy then gives I/O and async 25 % each, at most 4, and compute the rest). Bevy sizes
/// from `available_parallelism`, which on Windows counts every processor whatever the
/// process is limited to [observed in #1416: 32 when pinned to 16].
pub fn pool_threads(domain: &Domain) -> usize {
    domain.lps() as usize
}

/// A cache size as the log says it: whole MiB, else KiB, else bytes.
fn size_text(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB && bytes.is_multiple_of(MIB) {
        format!("{} MiB", bytes / MIB)
    } else if bytes >= KIB && bytes.is_multiple_of(KIB) {
        format!("{} KiB", bytes / KIB)
    } else {
        format!("{bytes} B")
    }
}

/// `L3 domains 96 MiB (16 LPs), 32 MiB (16 LPs)`.
fn domains_text(domains: &[Domain]) -> String {
    let mut s = String::from("L3 domains ");
    for (i, d) in domains.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let _ = write!(s, "{} ({} LPs)", size_text(d.bytes), d.lps());
    }
    s
}

/// The log line for a decision that does nothing (all but [`Decision::Restrict`]).
fn idle_line(topology: &Topology, decision: Decision) -> String {
    let doms = domains_text(&topology.domains);
    match decision {
        Decision::MultiGroup => "cpu placement: more than one processor group, nothing to do".to_string(),
        Decision::OneDomain if topology.domains.is_empty() => "cpu placement: no L3 domain found, nothing to do".to_string(),
        Decision::OneDomain => "cpu placement: one L3 domain, nothing to do".to_string(),
        Decision::SameSize => format!("cpu placement: {doms}; all the same size, nothing to do"),
        Decision::NoSingleLargest => format!("cpu placement: {doms}; no single largest, nothing to do"),
        Decision::TooSmall(n) => format!("cpu placement: {doms}; the largest has {n} LPs (under {MIN_LPS}), nothing to do"),
        Decision::FewerThreads(n, m) => format!("cpu placement: {doms}; the largest has {n} LPs, another {m}, nothing to do"),
        Decision::AlreadySet => format!("cpu placement: {doms}; affinity already set, left alone"),
        Decision::Restrict(d) => restricted_line(topology, &d, pool_threads(&d)),
    }
}

fn restricted_line(topology: &Topology, d: &Domain, pools: usize) -> String {
    format!(
        "cpu placement: {}; restricted to the {} domain ({} LPs), task pools {pools}",
        domains_text(&topology.domains),
        size_text(d.bytes),
        d.lps()
    )
}

/// One of the system's CPU sets: its group, logical processor index, ID, and whether this
/// process may use it (not allocated to another process for its exclusive use).
pub type CpuSet = (u16, u8, u32, bool);

/// The CPU set IDs of the logical processors in `mask` (group 0), only when every one of
/// them has a CPU set this process may use: a domain partly reserved elsewhere is left
/// alone, as the game would run on fewer processors than its task pools are sized for.
pub fn ids_for_mask(sets: &[CpuSet], mask: u64) -> Result<Vec<u32>, String> {
    let mut ids = Vec::new();
    let mut covered = 0u64;
    for &(group, lp, id, usable) in sets {
        if group != 0 || lp >= 64 || mask & (1u64 << lp) == 0 {
            continue;
        }
        if !usable {
            return Err(format!("logical processor {lp} is reserved for another process"));
        }
        ids.push(id);
        covered |= 1u64 << lp;
    }
    if ids.is_empty() || covered != mask {
        return Err(format!("{} of the domain's {} logical processors have a CPU set", covered.count_ones(), mask.count_ones()));
    }
    Ok(ids)
}

/// What `place` found and did, logged once the log is up ([`Placement::report`]).
#[derive(Clone, Debug)]
pub struct Placement {
    /// The `cpu placement:` line.
    pub line: String,
    pub warnings: Vec<String>,
    /// Bevy's task pools' thread budget; `None`: Bevy's own sizing, as before.
    pub pool_threads: Option<usize>,
}

impl Placement {
    pub fn report(&self) {
        for w in &self.warnings {
            bevy::log::warn!("{w}");
        }
        bevy::log::info!("{}", self.line);
    }
}

/// `[Settings] cpu-cache-placement` of the INI this run reads (`SettingsFile::for_run`).
fn ini_value() -> Option<String> {
    // Unattended: a test aid drives this run (as nvf_play's test_aid::Unattended).
    let unattended = std::env::vars_os().any(|(k, _)| k.to_str().is_some_and(nvf_play::drives_run));
    let path = nvf_play::settings::SettingsFile::for_run(unattended).0?;
    let text = std::fs::read_to_string(path).ok()?;
    fnv_install::ini_get(&text, nvf_play::settings::SECTION, INI_KEY)
}

/// Decide and apply the placement (module notes). Call before any thread pool starts.
pub fn place(cli: Option<Mode>) -> Placement {
    let ini = if cli.is_none() { ini_value() } else { None };
    let (mode, from, warning) = mode_of(cli, ini.as_deref());
    let warnings: Vec<String> = warning.into_iter().collect();
    if mode == Mode::Off {
        let line = match from {
            "--cpu-cache-placement" => "cpu placement: off by setting (--cpu-cache-placement off)".to_string(),
            _ => format!("cpu placement: off by setting (NVForever.ini [Settings] {INI_KEY}=off)"),
        };
        return Placement { line, warnings, pool_threads: None };
    }
    let (line, pool_threads) = os::place();
    Placement { line, warnings, pool_threads }
}

#[cfg(not(windows))]
mod os {
    /// Linux would read `/sys/devices/system/cpu/cpu*/cache/index3` (later).
    pub fn place() -> (String, Option<usize>) {
        ("cpu placement: only done on Windows so far, nothing to do".to_string(), None)
    }
}

#[cfg(windows)]
mod os {
    use super::{decide, idle_line, ids_for_mask, restricted_line, CpuSet, Decision, Domain, Topology};
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER};
    use windows_sys::Win32::System::SystemInformation::{
        CacheUnified, CpuSetInformation, GetLogicalProcessorInformationEx, GetSystemCpuSetInformation, RelationCache, CACHE_RELATIONSHIP,
        SYSTEM_CPU_SET_INFORMATION, SYSTEM_CPU_SET_INFORMATION_0_0, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };
    use windows_sys::Win32::System::Threading::{
        GetActiveProcessorGroupCount, GetCurrentProcess, GetCurrentThread, GetProcessAffinityMask, GetProcessDefaultCpuSets,
        GetThreadSelectedCpuSets, SetProcessDefaultCpuSets, SetThreadSelectedCpuSets,
    };

    /// `SYSTEM_CPU_SET_INFORMATION` flag bits [documented: Microsoft Learn,
    /// "SYSTEM_CPU_SET_INFORMATION structure": Parked, Allocated, AllocatedToTargetProcess,
    /// RealTime, in that bit order].
    const ALLOCATED: u8 = 0x2;
    const ALLOCATED_TO_TARGET_PROCESS: u8 = 0x4;

    /// Where each record's union starts: after its two 4-byte header fields
    /// (`Relationship`/`Size`, `Size`/`Type`), at the union's 8-byte alignment [design:
    /// windows-sys SystemInformation/mod.rs].
    const BODY: usize = 8;

    pub fn place() -> (String, Option<usize>) {
        let topology = match topology() {
            Ok(t) => t,
            Err(e) => return (format!("cpu placement: topology not read ({e}), nothing to do"), None),
        };
        // Only asked when it can matter: the rules before it need no system call.
        let decision = match decide(&topology, false) {
            Decision::Restrict(_) => match already_set() {
                Ok(set) => decide(&topology, set),
                Err(e) => return (format!("cpu placement: process placement not read ({e}), nothing to do"), None),
            },
            d => d,
        };
        // Nothing to do: no affinity or CPU-set call, Bevy's pools as they are.
        let (Some(domain), Some(pools)) = (decision.domain(), decision.pool_threads()) else { return (idle_line(&topology, decision), None) };
        match restrict(&domain) {
            Ok(()) => (restricted_line(&topology, &domain, pools), Some(pools)),
            Err(e) => (format!("cpu placement: {}; restricting failed ({e}), nothing changed", super::domains_text(&topology.domains)), None),
        }
    }

    /// A buffer of `len` bytes, 8-byte aligned.
    fn buffer(len: u32) -> Vec<u64> {
        vec![0u64; (len as usize).div_ceil(8)]
    }

    fn last_error() -> u32 {
        // SAFETY: reads this thread's last-error value; no arguments.
        unsafe { GetLastError() }
    }

    /// The L3 caches (`GetLogicalProcessorInformationEx`, `RelationCache`, level 3,
    /// unified) with their processors.
    fn topology() -> Result<Topology, String> {
        let mut len = 0u32;
        // SAFETY: a null buffer of length 0 asks for the length needed; `len` is a valid u32.
        let ok = unsafe { GetLogicalProcessorInformationEx(RelationCache, std::ptr::null_mut(), &mut len) };
        if ok != 0 || last_error() != ERROR_INSUFFICIENT_BUFFER || len == 0 {
            return Err(format!("GetLogicalProcessorInformationEx gave no length, error {}", last_error()));
        }
        let mut buf = buffer(len);
        // SAFETY: `buf` holds at least `len` bytes, 8-byte aligned; the call writes at most `len`.
        if unsafe { GetLogicalProcessorInformationEx(RelationCache, buf.as_mut_ptr().cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(), &mut len) } == 0 {
            return Err(format!("GetLogicalProcessorInformationEx failed, error {}", last_error()));
        }
        // SAFETY: GetActiveProcessorGroupCount takes nothing and returns a count.
        let mut multi_group = unsafe { GetActiveProcessorGroupCount() } > 1;
        let bytes = &as_bytes(&buf)[..(len as usize).min(buf.len() * 8)];
        let mut domains = Vec::new();
        let mut at = 0usize;
        while at + BODY <= bytes.len() {
            let relationship = i32::from_ne_bytes(bytes[at..at + 4].try_into().unwrap());
            let size = u32::from_ne_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            if size < BODY || at + size > bytes.len() {
                break;
            }
            if relationship == RelationCache && size >= BODY + size_of::<CACHE_RELATIONSHIP>() {
                // SAFETY: the record holds a whole CACHE_RELATIONSHIP at BODY (checked above);
                // read unaligned, as plain data.
                let cache = unsafe { bytes.as_ptr().add(at + BODY).cast::<CACHE_RELATIONSHIP>().read_unaligned() };
                if cache.Level == 3 && cache.Type == CacheUnified {
                    // SAFETY: both union fields start with the first GROUP_AFFINITY; plain data.
                    let group = unsafe { cache.Anonymous.GroupMask };
                    // GroupCount is 0 before Windows 11 (the field was reserved): one group.
                    if cache.GroupCount > 1 || group.Group != 0 {
                        multi_group = true;
                    }
                    domains.push(Domain { bytes: u64::from(cache.CacheSize), mask: group.Mask as u64 });
                }
            }
            at += size;
        }
        Ok(Topology::new(domains, multi_group))
    }

    fn as_bytes(buf: &[u64]) -> &[u8] {
        // SAFETY: u64 has no padding; the byte view covers exactly the same memory.
        unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len() * 8) }
    }

    /// Whether someone already chose where this process runs: an affinity mask other than
    /// the system's, or default CPU sets.
    fn already_set() -> Result<bool, String> {
        // SAFETY: a pseudo-handle for this process; no memory is shared.
        let process = unsafe { GetCurrentProcess() };
        let (mut mine, mut system) = (0usize, 0usize);
        // SAFETY: both out-pointers are valid usizes.
        if unsafe { GetProcessAffinityMask(process, &mut mine, &mut system) } == 0 {
            return Err(format!("GetProcessAffinityMask failed, error {}", last_error()));
        }
        if mine != system {
            return Ok(true);
        }
        let mut count = 0u32;
        // SAFETY: a null buffer of length 0 asks for the count only; it fails with
        // ERROR_INSUFFICIENT_BUFFER when the process has default CPU sets.
        let ok = unsafe { GetProcessDefaultCpuSets(process, std::ptr::null_mut(), 0, &mut count) };
        if ok == 0 && last_error() != ERROR_INSUFFICIENT_BUFFER {
            return Err(format!("GetProcessDefaultCpuSets failed, error {}", last_error()));
        }
        if count > 0 {
            return Ok(true);
        }
        // A launcher may also have chosen the main thread's own sets, which win over the
        // process default; those are left alone too.
        // SAFETY: a pseudo-handle for this thread; a null buffer of length 0 asks for the count.
        let ok = unsafe { GetThreadSelectedCpuSets(GetCurrentThread(), std::ptr::null_mut(), 0, &mut count) };
        if ok == 0 && last_error() != ERROR_INSUFFICIENT_BUFFER {
            return Err(format!("GetThreadSelectedCpuSets failed, error {}", last_error()));
        }
        Ok(count > 0)
    }

    /// The system's CPU sets as (group, logical processor index, ID, usable by this process).
    fn cpu_sets() -> Result<Vec<CpuSet>, String> {
        // SAFETY: a pseudo-handle for this process; no memory is shared.
        let process = unsafe { GetCurrentProcess() };
        let mut len = 0u32;
        // SAFETY: a null buffer of length 0 asks for the length needed.
        let ok = unsafe { GetSystemCpuSetInformation(std::ptr::null_mut(), 0, &mut len, process, 0) };
        if ok != 0 || last_error() != ERROR_INSUFFICIENT_BUFFER || len == 0 {
            return Err(format!("GetSystemCpuSetInformation gave no length, error {}", last_error()));
        }
        let mut buf = buffer(len);
        let cap = len;
        // SAFETY: `buf` holds at least `cap` bytes, 8-byte aligned; the call writes at most `cap`.
        if unsafe { GetSystemCpuSetInformation(buf.as_mut_ptr().cast::<SYSTEM_CPU_SET_INFORMATION>(), cap, &mut len, process, 0) } == 0 {
            return Err(format!("GetSystemCpuSetInformation failed, error {}", last_error()));
        }
        let bytes = &as_bytes(&buf)[..(len as usize).min(buf.len() * 8)];
        let mut out = Vec::new();
        let mut at = 0usize;
        while at + BODY <= bytes.len() {
            let size = u32::from_ne_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let kind = i32::from_ne_bytes(bytes[at + 4..at + 8].try_into().unwrap());
            if size < BODY || at + size > bytes.len() {
                break;
            }
            if kind == CpuSetInformation && size >= BODY + size_of::<SYSTEM_CPU_SET_INFORMATION_0_0>() {
                // SAFETY: the record holds a whole CpuSet body at BODY (checked above); plain data.
                let set = unsafe { bytes.as_ptr().add(at + BODY).cast::<SYSTEM_CPU_SET_INFORMATION_0_0>().read_unaligned() };
                // SAFETY: the flags union is a plain u8 in every variant.
                let flags = unsafe { set.Anonymous1.AllFlags };
                let usable = flags & ALLOCATED == 0 || flags & ALLOCATED_TO_TARGET_PROCESS != 0;
                out.push((set.Group, set.LogicalProcessorIndex, set.Id, usable));
            }
            at += size;
        }
        Ok(out)
    }

    /// The process's default CPU sets, and the main thread's own, to the domain's.
    fn restrict(domain: &Domain) -> Result<(), String> {
        let ids = ids_for_mask(&cpu_sets()?, domain.mask)?;
        // SAFETY: a pseudo-handle for this process; `ids` is a valid slice of its length.
        if unsafe { SetProcessDefaultCpuSets(GetCurrentProcess(), ids.as_ptr(), ids.len() as u32) } == 0 {
            return Err(format!("SetProcessDefaultCpuSets failed, error {}", last_error()));
        }
        // The main thread already runs: give it the same sets itself (module notes). If that
        // is refused, the process default is cleared again, so a failure changes nothing.
        // SAFETY: a pseudo-handle for this thread; `ids` is a valid slice of its length.
        if unsafe { SetThreadSelectedCpuSets(GetCurrentThread(), ids.as_ptr(), ids.len() as u32) } == 0 {
            let err = last_error();
            // SAFETY: a null list of length 0 clears the process's default CPU sets.
            unsafe { SetProcessDefaultCpuSets(GetCurrentProcess(), std::ptr::null(), 0) };
            return Err(format!("SetThreadSelectedCpuSets failed, error {err}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    fn dom(mib: u64, first: u32, count: u32) -> Domain {
        let mask = if count >= 64 { u64::MAX } else { ((1u64 << count) - 1) << first };
        Domain { bytes: mib * MIB, mask }
    }

    #[test]
    fn a_dual_ccd_x3d_goes_to_the_cache_ccd() {
        // 7945HX3D / 7950X3D / 9950X3D: 96 MiB on 0-15, 32 MiB on 16-31.
        let t = Topology::new(vec![dom(32, 16, 16), dom(96, 0, 16)], false);
        assert_eq!(t.domains[0].bytes, 96 * MIB);
        let d = decide(&t, false);
        assert_eq!(d, Decision::Restrict(dom(96, 0, 16)));
        assert_eq!(pool_threads(&dom(96, 0, 16)), 16);
        assert_eq!(
            restricted_line(&t, &dom(96, 0, 16), 16),
            "cpu placement: L3 domains 96 MiB (16 LPs), 32 MiB (16 LPs); restricted to the 96 MiB domain (16 LPs), task pools 16"
        );
        // The cache CCD second in the processor order: still the cache CCD.
        let t = Topology::new(vec![dom(32, 0, 16), dom(96, 16, 16)], false);
        assert_eq!(decide(&t, false), Decision::Restrict(dom(96, 16, 16)));
    }

    #[test]
    fn a_twelve_core_x3d_still_acts_but_six_cores_without_smt_do_not() {
        // 7900X3D / 9900X3D: 6 cores a CCD, 12 LPs with SMT.
        let t = Topology::new(vec![dom(96, 0, 12), dom(32, 12, 12)], false);
        assert_eq!(decide(&t, false), Decision::Restrict(dom(96, 0, 12)));
        // SMT off: 6 LPs on the cache CCD.
        let t = Topology::new(vec![dom(96, 0, 6), dom(32, 6, 6)], false);
        assert_eq!(decide(&t, false), Decision::TooSmall(6));
        assert_eq!(idle_line(&t, Decision::TooSmall(6)), "cpu placement: L3 domains 96 MiB (6 LPs), 32 MiB (6 LPs); the largest has 6 LPs (under 8), nothing to do");
    }

    #[test]
    fn a_larger_cache_on_fewer_threads_is_left_alone() {
        // Ryzen AI 9 HX 370 ("Strix Point"): 4 Zen 5 cores (8 LPs) share 16 MiB, 8 Zen 5c
        // cores (16 LPs) share 8 MiB. Moving to the 16 MiB domain would give up 16 threads.
        let t = Topology::new(vec![dom(16, 0, 8), dom(8, 8, 16)], false);
        assert_eq!(decide(&t, false), Decision::FewerThreads(8, 16));
        assert_eq!(
            idle_line(&t, Decision::FewerThreads(8, 16)),
            "cpu placement: L3 domains 16 MiB (8 LPs), 8 MiB (16 LPs); the largest has 8 LPs, another 16, nothing to do"
        );
        // No restriction is asked for, so the pools keep their defaults.
        assert_eq!(decide(&t, false).domain(), None);
        // A cache CCD with as many LPs as the other still acts (every dual-CCD X3D part).
        let t = Topology::new(vec![dom(96, 0, 16), dom(32, 16, 16)], false);
        assert_eq!(decide(&t, false), Decision::Restrict(dom(96, 0, 16)));
    }

    #[test]
    fn one_or_equal_l3s_are_left_alone() {
        // 9800X3D, Intel desktop: one L3.
        let t = Topology::new(vec![dom(96, 0, 16)], false);
        assert_eq!(decide(&t, false), Decision::OneDomain);
        assert_eq!(idle_line(&t, Decision::OneDomain), "cpu placement: one L3 domain, nothing to do");
        // The same cache listed twice is one domain.
        let t = Topology::new(vec![dom(36, 0, 24), dom(36, 0, 24)], false);
        assert_eq!(decide(&t, false), Decision::OneDomain);
        let t = Topology::new(Vec::new(), false);
        assert_eq!(idle_line(&t, decide(&t, false)), "cpu placement: no L3 domain found, nothing to do");
        // 7950X: two 32 MiB L3s; a Threadripper's eight.
        let t = Topology::new(vec![dom(32, 0, 16), dom(32, 16, 16)], false);
        assert_eq!(decide(&t, false), Decision::SameSize);
        assert_eq!(idle_line(&t, Decision::SameSize), "cpu placement: L3 domains 32 MiB (16 LPs), 32 MiB (16 LPs); all the same size, nothing to do");
        let t = Topology::new((0..4).map(|i| dom(32, i * 16, 16)).collect(), false);
        assert_eq!(decide(&t, false), Decision::SameSize);
    }

    #[test]
    fn machines_without_unequal_l3s_get_nothing_done() {
        let cases: [(&str, Vec<Domain>, bool); 7] = [
            ("one 32 MiB L3, 16 LPs (7700X-like)", vec![dom(32, 0, 16)], false),
            ("two equal 32 MiB L3s (7950X-like)", vec![dom(32, 0, 16), dom(32, 16, 16)], false),
            ("one 96 MiB L3, 16 LPs (7800X3D-like)", vec![dom(96, 0, 16)], false),
            ("one 36 MiB L3, 32 LPs (Intel-like)", vec![dom(36, 0, 32)], false),
            ("largest under 8 LPs", vec![dom(96, 0, 6), dom(32, 6, 6)], false),
            ("several processor groups", vec![dom(96, 0, 16), dom(32, 16, 16)], true),
            ("no L3 read", Vec::new(), false),
        ];
        for (what, domains, multi) in cases {
            let t = Topology::new(domains, multi);
            for already in [false, true] {
                let d = decide(&t, already);
                assert_eq!(d.domain(), None, "{what}: nothing restricted");
                assert_eq!(d.pool_threads(), None, "{what}: Bevy's pools as they are");
            }
        }
        // An affinity the player narrowed on a machine that would otherwise act.
        let d = decide(&Topology::new(vec![dom(96, 0, 16), dom(32, 16, 16)], false), true);
        assert_eq!((d, d.domain(), d.pool_threads()), (Decision::AlreadySet, None, None));
        // Only the acting case sizes the pools.
        let d = decide(&Topology::new(vec![dom(96, 0, 16), dom(32, 16, 16)], false), false);
        assert_eq!(d.pool_threads(), Some(16));
    }

    #[test]
    fn two_largest_of_one_size_are_not_chosen_between() {
        let t = Topology::new(vec![dom(96, 0, 16), dom(96, 16, 16), dom(32, 32, 16)], false);
        assert_eq!(decide(&t, false), Decision::NoSingleLargest);
    }

    #[test]
    fn a_machine_of_several_processor_groups_is_left_alone() {
        let t = Topology::new(vec![dom(96, 0, 16), dom(32, 16, 16)], true);
        assert_eq!(decide(&t, false), Decision::MultiGroup);
        assert_eq!(idle_line(&t, Decision::MultiGroup), "cpu placement: more than one processor group, nothing to do");
    }

    #[test]
    fn a_placement_the_player_chose_wins() {
        let t = Topology::new(vec![dom(96, 0, 16), dom(32, 16, 16)], false);
        assert_eq!(decide(&t, true), Decision::AlreadySet);
        assert_eq!(idle_line(&t, Decision::AlreadySet), "cpu placement: L3 domains 96 MiB (16 LPs), 32 MiB (16 LPs); affinity already set, left alone");
        // A single-L3 machine says so first, whoever set its affinity.
        let t = Topology::new(vec![dom(96, 0, 16)], false);
        assert_eq!(decide(&t, true), Decision::OneDomain);
    }

    #[test]
    fn the_command_line_wins_over_the_ini_and_auto_is_the_default() {
        assert_eq!(mode_of(None, None), (Mode::Auto, "default", None));
        assert_eq!(mode_of(None, Some("off")), (Mode::Off, "NVForever.ini", None));
        assert_eq!(mode_of(None, Some(" Auto ")), (Mode::Auto, "NVForever.ini", None));
        assert_eq!(mode_of(Some(Mode::Auto), Some("off")), (Mode::Auto, "--cpu-cache-placement", None));
        assert_eq!(mode_of(Some(Mode::Off), None).0, Mode::Off);
        let (m, from, warning) = mode_of(None, Some("1"));
        assert_eq!((m, from), (Mode::Auto, "default"));
        assert_eq!(warning.as_deref(), Some("cpu placement: [Settings] cpu-cache-placement=1 not understood (auto or off); auto"));
    }

    #[test]
    fn cpu_set_ids_follow_the_domains_processors() {
        // IDs as Windows numbers them (256 up), one per logical processor of group 0, and a
        // second group's set that must not be taken.
        let sets: Vec<CpuSet> = (0..32u8).map(|lp| (0, lp, 256 + u32::from(lp), true)).chain([(1, 0, 900, true)]).collect();
        assert_eq!(ids_for_mask(&sets, dom(96, 0, 16).mask), Ok((256..272).collect::<Vec<u32>>()));
        assert_eq!(ids_for_mask(&sets, dom(32, 16, 16).mask), Ok((272..288).collect::<Vec<u32>>()));
        assert!(ids_for_mask(&sets, 0).is_err());
    }

    #[test]
    fn a_domain_not_wholly_usable_is_left_alone() {
        // One processor of the cache CCD reserved for another process (e.g. real-time audio).
        let mut sets: Vec<CpuSet> = (0..32u8).map(|lp| (0, lp, 256 + u32::from(lp), true)).collect();
        sets[3].3 = false;
        assert_eq!(ids_for_mask(&sets, dom(96, 0, 16).mask), Err("logical processor 3 is reserved for another process".to_string()));
        // A processor of the domain without any CPU set.
        let sets: Vec<CpuSet> = (0..32u8).filter(|&lp| lp != 5).map(|lp| (0, lp, 256 + u32::from(lp), true)).collect();
        assert_eq!(ids_for_mask(&sets, dom(96, 0, 16).mask), Err("15 of the domain's 16 logical processors have a CPU set".to_string()));
    }

    #[test]
    fn sizes_read_as_mib_or_kib() {
        assert_eq!(size_text(96 * MIB), "96 MiB");
        assert_eq!(size_text(1536 * 1024), "1536 KiB");
        assert_eq!(size_text(1000), "1000 B");
    }
}
