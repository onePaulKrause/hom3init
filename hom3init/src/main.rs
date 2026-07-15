// main.rs - HOM3 native PID 1.
//
// The sovereign boot sequence, in order. This binary is /sbin/init in the
// image. It owns the boot, enforces airgap-by-default and the entropy gate,
// runs preflight, launches the native experience, and supervises as PID 1.
//
// Status: the sequence is real and runs on hardware. A few security-critical
// steps are honestly marked TODO(next) in boot.rs (at-rest LUKS unlock, wired
// netlink link-down) - they are stubbed in shape, never faked. The custody
// policy consulted at preflight is abstracted behind the `Sovereignty` trait;
// the reference implementation here is permissive (see boot.rs).

mod sys;
mod boot;

fn main() {
    boot::log("HOM3 native init starting (PID 1)");

    // 1. essential mounts (hardened)
    boot::early_mounts();

    // 2. entropy gate (signing is refused until seeded)
    let _entropy_ready = boot::entropy_gate();

    // 3. airgap by default (radios down before anything else)
    boot::airgap_default();

    // 4. portable encrypted data partition
    let data_ok = boot::mount_data();

    // 5. preflight: a breach drops to the INERT recovery console (no signing),
    //    not an immediate power-off, so the operator can see + fix the cause.
    let child = if boot::preflight(data_ok) {
        boot::launch()
    } else {
        boot::log("preflight breach: recovery console only (no experience)");
        boot::launch_recovery()
    };

    // 6/7. if nothing could launch, safe-stop; else supervise forever.
    if child.is_none() {
        boot::safe_stop();
    }
    boot::supervise(child);
}
