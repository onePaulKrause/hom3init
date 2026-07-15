// boot.rs - the sovereign boot sequence. Each step is a rail invariant.
//
// Order is load-bearing:
//   1 early mounts   2 entropy gate   3 airgap default   4 data partition
//   5 preflight (safe-stop on breach)  6 launch experience  7 supervise
//
// The custody policy consulted at preflight (step 5) is abstracted behind the
// `Sovereignty` trait. The reference implementation in this repository is
// PERMISSIVE: it verifies only that the boot rails are intact and always treats
// the node as an unprovisioned first boot. A production custody model - pinned
// update keys, keystore placement rules, and custody-seal semantics - is
// intentionally not shipped here; provide your own `Sovereignty` impl to
// enforce one. (See README: "What this is not".)
//
// Steps 1-3, 5-7 are implemented in shape and run on hardware. At-rest LUKS
// unlock (part of 4) and wired netlink link-down (part of 3) are marked
// TODO(next) with their design, and are NOT faked.

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use crate::sys;

const DATA_LABEL: &str = "HOM3DATA";
const DATA_MOUNT: &str = "/var/lib/hom3";
const EXPERIENCE: &str = "/usr/bin/hom3home";   // the graphic boot-shell (spawns the node)
const NODE: &str = "/usr/bin/hom3node";         // the node directly (Stage A: no graphic shell)
const RECOVERY: &str = "/usr/bin/hom3recovery"; // native recovery console (this crate)

pub fn log(msg: &str) {
    // PID1 logs to the kernel console (stdout is the console at this stage).
    println!("[hom3init] {msg}");
}

/// 1. Mount the essential virtual filesystems (hardened flags).
pub fn early_mounts() {
    let _ = fs::create_dir_all("/proc");
    let _ = fs::create_dir_all("/sys");
    let _ = fs::create_dir_all("/dev");
    let _ = fs::create_dir_all("/run");
    let _ = fs::create_dir_all("/tmp");
    let _ = sys::mount_vfs("proc", "/proc", "proc", sys::MS_NOSUID | sys::MS_NODEV | sys::MS_NOEXEC);
    let _ = sys::mount_vfs("sysfs", "/sys", "sysfs", sys::MS_NOSUID | sys::MS_NODEV | sys::MS_NOEXEC);
    let _ = sys::mount_vfs("devtmpfs", "/dev", "devtmpfs", sys::MS_NOSUID);
    let _ = sys::mount_vfs("tmpfs", "/run", "tmpfs", sys::MS_NOSUID | sys::MS_NODEV);
    let _ = sys::mount_vfs("tmpfs", "/tmp", "tmpfs", sys::MS_NOSUID | sys::MS_NODEV);
    log("early mounts up (proc/sys/dev/run/tmp, hardened flags)");
}

/// 2. Entropy gate: do not let the signing path run before the CSPRNG is seeded.
/// We wait (bounded) for a healthy pool + the hardware RNG, and record status.
///
/// NOTE: on Linux >= 5.18 the CRNG-seeded semantics changed; `entropy_avail`
/// is a coarse proxy. Treat this as a liveness gate, not a security proof, and
/// prefer `getrandom(2)` blocking semantics in the workload that actually signs.
pub fn entropy_gate() -> bool {
    let has_hwrng = Path::new("/dev/hwrng").exists();
    for _ in 0..60 {
        if let Ok(s) = fs::read_to_string("/proc/sys/kernel/random/entropy_avail") {
            if let Ok(avail) = s.trim().parse::<u32>() {
                if avail >= 256 && has_hwrng {
                    log(&format!("entropy ready (avail={avail}, hwrng=true)"));
                    let _ = fs::write("/run/hom3-entropy-ready", "1");
                    return true;
                }
            }
        }
        sleep(Duration::from_millis(500));
    }
    // fail-closed: the workload's signing gate refuses keygen/sign on this.
    log("WARN entropy not ready; signing will be refused until seeded");
    let _ = fs::write("/run/hom3-entropy-ready", "0");
    false
}

/// 3. Airgap default: bring all radios down via /dev/rfkill (block-all event).
/// Wired/tethered link-down via netlink is TODO(next); rfkill covers RF now.
pub fn airgap_default() {
    // struct rfkill_event { u32 idx; u8 type; u8 op; u8 soft; u8 hard; }
    // op = RFKILL_OP_CHANGE_ALL (3); type = RFKILL_TYPE_ALL (0); soft = 1.
    let ev: [u8; 8] = [0, 0, 0, 0, /*type*/0, /*op*/3, /*soft*/1, /*hard*/0];
    match fs::OpenOptions::new().write(true).open("/dev/rfkill") {
        Ok(mut f) => {
            use std::io::Write;
            let _ = f.write_all(&ev);
            log("airgap default: rfkill block-all issued");
        }
        Err(_) => log("WARN /dev/rfkill not present; RF block skipped"),
    }
    // TODO(next): netlink RTM_SETLINK down for wired/usb interfaces.
}

/// 4. Mount the portable data partition (keystore + workload state).
pub fn mount_data() -> bool {
    let _ = fs::create_dir_all(DATA_MOUNT);
    let by_label = format!("/dev/disk/by-label/{DATA_LABEL}");
    // TODO(next): LUKS unlock via dm-crypt ioctl (libcryptsetup-free) before mount;
    // at-rest encryption. Skeleton mounts the mapped/plain device if present.
    let dev = if Path::new(&by_label).exists() { by_label } else {
        log("WARN data partition (label HOM3DATA) not found");
        return false;
    };
    match sys::mount_dev(&dev, DATA_MOUNT, "ext4", sys::MS_NOSUID | sys::MS_NODEV) {
        Ok(()) => {
            let _ = fs::create_dir_all(format!("{DATA_MOUNT}/data"));
            let _ = fs::create_dir_all(format!("{DATA_MOUNT}/keystore"));
            log("data partition mounted at /var/lib/hom3");
            true
        }
        Err(e) => { log(&format!("WARN data mount failed errno={e}")); false }
    }
}

// ---------------------------------------------------------------------------
// Sovereignty policy (preflight, step 5)
// ---------------------------------------------------------------------------

/// The custody policy consulted at preflight.
///
/// The reference implementation shipped here (`ReferencePolicy`) enforces NO
/// custody model: it always reports "unprovisioned first boot" and lets the
/// node boot to a recovery/provision surface. A production policy - pinned
/// update-key verification, keystore placement/integrity rules, and a custody
/// seal - is maintained separately and is the subject of a pending patent. To
/// enforce your own model, implement this trait and pass it to `preflight_with`.
pub trait Sovereignty {
    /// Has this node been provisioned with durable custody state?
    fn is_provisioned(&self, data_ok: bool) -> bool;
    /// Is the update trust-root present and usable?
    fn trust_root_ok(&self) -> bool;
    /// Is the custody seal intact? (a hard invariant in production)
    fn seal_intact(&self) -> bool;
    /// Does the keystore satisfy placement/integrity rules?
    fn keystore_ok(&self, data_ok: bool) -> bool;
}

/// Permissive reference policy. Enough to boot, supervise, and land somewhere.
/// Enforces no custody model - replace for production use.
pub struct ReferencePolicy;

impl Sovereignty for ReferencePolicy {
    fn is_provisioned(&self, _data_ok: bool) -> bool { false }
    fn trust_root_ok(&self) -> bool { true }
    fn seal_intact(&self) -> bool { true }
    fn keystore_ok(&self, _data_ok: bool) -> bool { true }
}

/// 5. Preflight with the built-in permissive policy.
pub fn preflight(data_ok: bool) -> bool {
    preflight_with(&ReferencePolicy, data_ok)
}

/// 5. Preflight against an arbitrary sovereignty policy. Safe-stop on a HARD
/// breach; first-boot aware (an unprovisioned node may boot to provision).
pub fn preflight_with(policy: &dyn Sovereignty, data_ok: bool) -> bool {
    // custody seal MUST be intact in every state (hard invariant).
    if !policy.seal_intact() {
        log("PREFLIGHT FAIL: custody seal not intact");
        return false;
    }
    // keystore placement/integrity rules.
    if !policy.keystore_ok(data_ok) {
        log("PREFLIGHT FAIL: keystore policy not satisfied");
        return false;
    }
    if policy.is_provisioned(data_ok) {
        // a provisioned node must be able to verify updates.
        if !policy.trust_root_ok() {
            log("PREFLIGHT FAIL: provisioned node missing update trust root");
            return false;
        }
        log("preflight OK (provisioned)");
    } else {
        log("preflight OK (unprovisioned first boot; ready to provision)");
    }
    true
}

/// 6. Launch the experience. Falls through: graphic shell -> node directly ->
/// recovery. So a node-only (Stage A) image still reaches the ceremony.
pub fn launch() -> Option<i32> {
    for target in [EXPERIENCE, NODE, RECOVERY] {
        if Path::new(target).exists() {
            match sys::spawn(target, &[target]) {
                Ok(pid) => {
                    log(&format!("launched {target} pid={pid}"));
                    return Some(pid);
                }
                Err(e) => log(&format!("launch {target} failed errno={e}")),
            }
        }
    }
    log("no experience / node / recovery binary present");
    None
}

/// Launch the recovery console ONLY (used on a preflight breach; no experience).
pub fn launch_recovery() -> Option<i32> {
    if Path::new(RECOVERY).exists() {
        match sys::spawn(RECOVERY, &[RECOVERY]) {
            Ok(pid) => { log(&format!("launched recovery pid={pid}")); Some(pid) }
            Err(e) => { log(&format!("recovery launch failed errno={e}")); None }
        }
    } else {
        log("no recovery binary present");
        None
    }
}

/// 7. Supervise as PID 1: reap all children, restart the experience on exit.
pub fn supervise(mut child: Option<i32>) -> ! {
    loop {
        if let Some((pid, _status)) = sys::reap_one() {
            if Some(pid) == child {
                log("experience exited; restarting after backoff");
                std::thread::sleep(Duration::from_secs(2));
                child = launch();
                if child.is_none() {
                    log("cannot relaunch; holding at safe stop");
                    safe_stop();
                }
            }
            // else: reaped an orphan; continue.
        } else {
            // no children / interrupted; brief idle
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

/// Safe stop: no experience, no networking, no signing. Hold powered but inert,
/// or power off. We power off to avoid an unattended, half-up node.
pub fn safe_stop() -> ! {
    log("SAFE STOP: powering off (no experience / preflight breach)");
    sys::sync_all();
    sys::power_off();
}
