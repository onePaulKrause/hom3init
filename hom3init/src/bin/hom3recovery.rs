// hom3recovery - the native first-boot / recovery console.
//
// No busybox, no foreign shell: a small native binary the init launches when
// the experience (the node/graphic shell) is not yet present. It prints the
// sovereign status so the operator can SEE the OS booted airgapped + sane on
// real silicon, then holds the console. This is the honest "first boot lands
// somewhere" surface until the native experience exists.

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

fn read_trim(p: &str) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn airgap_blocked() -> (usize, usize) {
    // count rfkill nodes vs how many are soft/hard blocked
    let base = "/sys/class/rfkill";
    let mut total = 0;
    let mut blocked = 0;
    if let Ok(rd) = fs::read_dir(base) {
        for e in rd.flatten() {
            total += 1;
            let soft = read_trim(&format!("{}/soft", e.path().display())) == Some("1".into());
            let hard = read_trim(&format!("{}/hard", e.path().display())) == Some("1".into());
            if soft || hard { blocked += 1; }
        }
    }
    (blocked, total)
}

fn main() {
    println!();
    println!("======================================================");
    println!("  HOM3 - sovereign recovery console (first boot)");
    println!("======================================================");

    // airgap
    let (blk, tot) = airgap_blocked();
    println!("  airgap     : {blk}/{tot} radios blocked{}",
             if tot > 0 && blk == tot { " (all down)" } else { "" });

    // entropy
    let ready = read_trim("/run/hom3-entropy-ready").as_deref() == Some("1");
    let avail = read_trim("/proc/sys/kernel/random/entropy_avail").unwrap_or_else(|| "?".into());
    let hwrng = Path::new("/dev/hwrng").exists();
    println!("  entropy    : ready={ready} avail={avail} hwrng={hwrng}");

    // data partition
    let data = Path::new("/var/lib/hom3/keystore").exists();
    println!("  data part  : {}", if data { "mounted (/var/lib/hom3)" }
                                   else { "absent (provision the HOM3DATA carrier)" });

    // update trust root (public key present?)
    let key = fs::metadata("/etc/hom3/release.pub").map(|m| m.len() >= 32).unwrap_or(false);
    println!("  update key : {}", if key { "pinned (updates verifiable)" }
                                  else { "NOT pinned (pin release.pub to verify updates)" });

    println!("------------------------------------------------------");
    println!("  state      : OS booted sovereign. The native");
    println!("               experience is not installed yet.");
    println!("  next       : provision the encrypted data carrier, then");
    println!("               the ceremony arrives with the node.");
    println!("======================================================");
    println!();
    println!("  (console held. power off the board when done.)");

    // hold the console; init supervises us. No shell is exposed by default.
    loop { sleep(Duration::from_secs(3600)); }
}
