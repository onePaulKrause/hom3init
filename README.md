# hom3init

A sovereign, airgap-first **PID 1** (`init`) for aarch64 Linux, written in Rust
with **zero external crates**. It is a development boot and supervision layer of the HOM3
sovereign OS, extracted for public use.

`hom3init` replaces the distro init as `/sbin/init`. It owns the boot from the
moment the kernel hands off: it brings up a hardened filesystem set, gates on
entropy before any signing path can run, forces all radios down by default,
mounts a portable data partition, runs a preflight policy check, launches the
node/experience, and then supervises as PID 1 for the life of the system.

It was built for an OrangePi 5 Plus (Rockchip RK3588) but nothing in it is
board-specific except the data-partition label.

---

## Why

Most "minimal" Linux appliances still boot a general-purpose init, a shell, and
a pile of userland you have to trust. `hom3init` is the opposite bet: a single
small binary, no external dependencies, a boot sequence you can read top to
bottom in one sitting, and **airgap + entropy as boot invariants** rather than
afterthoughts. If it cannot come up sovereign, it lands on an inert recovery
console or powers off - it never comes up half-trusted.

## The boot rail

The sequence is load-bearing; the order is the design. Full detail in
[`docs/BOOT_RAIL.md`](docs/BOOT_RAIL.md).

1. **Early mounts** - `proc`, `sys`, `dev`, `run`, `tmp` with `nosuid/nodev/noexec` where valid.
2. **Entropy gate** - wait (bounded) for a seeded pool + hardware RNG; record readiness. Signing is refused until seeded.
3. **Airgap default** - issue an rfkill block-all before anything else runs. (Wired netlink link-down is on the roadmap.)
4. **Data partition** - mount the portable `HOM3DATA` carrier (keystore + workload state). At-rest LUKS unlock is on the roadmap and is honestly stubbed, not faked.
5. **Preflight** - consult a `Sovereignty` policy. A hard breach drops to the inert recovery console (no signing), not a silent power-off, so the operator can see and fix the cause.
6. **Launch** - start the experience, falling through `graphic shell -> node -> recovery`, so even a node-only image reaches a usable surface.
7. **Supervise** - reap all children as PID 1; restart the experience on exit with backoff; safe-stop if it cannot be relaunched.

## What this is *not*

This repo is the **boot + init layer only**. It deliberately excludes:

- **The signing/keystore workload** (`hom3node`: Ed25519, Argon2, canonicalization, the append-only logbook). Key generation and custody happen there, not here.
- **The provenance / attestation method** - the subject of a pending patent - which is not named or disclosed anywhere in this repository.
- **Board firmware** (Rockchip DDR blob, ARM Trusted Firmware/BL31, U-Boot) - third-party, below the sovereignty line.

The custody policy checked at preflight is abstracted behind the `Sovereignty`
trait (`src/boot.rs`). **The reference implementation enforces no custody model**
- it always reports "unprovisioned first boot" so the system boots to a
provision/recovery surface. To enforce pinned update keys, keystore placement
rules, and a custody seal, implement `Sovereignty` yourself and call
`preflight_with(&your_policy, data_ok)`. See [`NOTICE`](NOTICE).

## Build

Quick version (host with `rustup`):

```bash
rustup target add aarch64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
# -> target/aarch64-unknown-linux-musl/release/hom3init
# -> target/aarch64-unknown-linux-musl/release/hom3recovery
```

`musl` + static linking is the intended target so the binary has no dynamic
loader dependency. Full instructions, including installing it as `/sbin/init`
and wiring it into a Buildroot image, are in
[`docs/BUILDING.md`](docs/BUILDING.md).

## Status / roadmap

Runs on real RK3588 hardware today. Honestly-marked open items (see `TODO(next)`
in `src/boot.rs`):

- **At-rest encryption** - the data partition currently mounts plaintext; dm-crypt/LUKS unlock before mount is the next step. Until then, do not treat data-at-rest as protected.
- **Wired link-down** - rfkill covers radios; netlink `RTM_SETLINK` down for wired/USB interfaces is pending.
- **libc removal** - `src/sys.rs` is a tiny FFI shim over musl; the set is small on purpose so a `no_std` raw-syscall version is a short step.


---

1<3
