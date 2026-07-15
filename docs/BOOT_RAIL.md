# The boot rail

`hom3init` runs a fixed seven-step sequence. The order is a security argument,
not a convenience: each step establishes an invariant the next step relies on.
This document explains each step and where the honest gaps are.

## 0. Precondition: we are PID 1

The kernel has mounted the root filesystem and executed `/sbin/init`, which is
`hom3init`. Nothing else has run. There is no shell, no service manager, no
network. Stdout is the kernel console, so `log()` writes go straight to serial /
tty. If `hom3init` exits, the kernel panics - so it must never return; it either
supervises forever or performs a clean power-off.

## 1. Early mounts (hardened)

We create and mount the minimum virtual filesystems:

| mount | fs | flags |
|---|---|---|
| `/proc` | proc | nosuid, nodev, noexec |
| `/sys` | sysfs | nosuid, nodev, noexec |
| `/dev` | devtmpfs | nosuid |
| `/run` | tmpfs | nosuid, nodev |
| `/tmp` | tmpfs | nosuid, nodev |

`noexec` is applied where it is always valid; `/dev` keeps exec off the flag set
because device nodes are not executed but the mount itself must allow device
special files. These are the surfaces every later step reads (entropy from
`/proc`, rfkill from `/dev` and `/sys`, the data device from `/dev/disk`).

## 2. Entropy gate

Before any code path that could generate or use a key runs, we require a usable
CSPRNG. We poll (bounded, ~30s) for `entropy_avail >= 256` **and** the presence
of `/dev/hwrng`, then write `/run/hom3-entropy-ready` = `1` or `0`.

This is **fail-closed**: if entropy is not ready, the flag is `0` and the signing
workload is expected to refuse keygen/sign. The gate is a *liveness* signal, not
a cryptographic proof - on modern kernels `entropy_avail` is coarse, so the
workload that actually signs should still use blocking `getrandom(2)` semantics.
The gate exists so a node cannot silently produce low-entropy keys on first boot.

## 3. Airgap by default

We issue an rfkill **block-all** event by writing an 8-byte `rfkill_event`
(`op = CHANGE_ALL`, `soft = 1`) to `/dev/rfkill`. All radios go soft-blocked
before anything else runs. Sovereignty is the default state; connectivity is an
explicit, later, revocable act - not the condition you boot into.

**Gap (roadmap):** wired and USB interfaces are not radios and are not covered
by rfkill. Bringing them administratively down requires a netlink
`RTM_SETLINK`. That is marked `TODO(next)` and is not faked - today the airgap
guarantee is RF-complete but not wired-complete.

## 4. Data partition

We mount the portable `HOM3DATA` carrier (looked up by filesystem label) at
`/var/lib/hom3` with `nosuid,nodev`, and ensure `data/` and `keystore/`
subdirectories exist. Keys live on this removable carrier, never on the boot
medium - so the boot image can be public and reproducible while custody travels
separately.

**Gap (roadmap):** at-rest encryption. The partition currently mounts plaintext.
The intended design unlocks a dm-crypt/LUKS volume (via ioctl, without linking
libcryptsetup) *before* mounting. Until that lands, treat data-at-rest as
unprotected. This is marked `TODO(next)`.

## 5. Preflight

Preflight consults a `Sovereignty` policy (a trait) and returns go / no-go:

- **seal intact** - a hard invariant in production; a broken seal is an
  immediate no-go.
- **keystore policy** - placement/integrity rules for the keystore.
- **provisioned?** - if the node has durable custody state, it must also have a
  usable update trust root, or it is a no-go. An *unprovisioned* node is allowed
  to boot (to a provision/recovery surface) even without a trust root, because
  first boot is how you establish one.

A no-go does **not** power off immediately. It launches the **recovery console
only** - inert, no signing, no experience - so the operator can see the failure
reason on screen and correct it. Silent bricking is a worse failure mode than a
visible halt.

> In this open-source build, the shipped `ReferencePolicy` enforces no custody
> model: it always returns "unprovisioned first boot". The production policy -
> pinned keys, keystore rules, custody seal - is a separate, private
> implementation of the same trait. Swap yours in via `preflight_with`.

## 6. Launch

We start the first present binary of `hom3home` (graphic experience) ->
`hom3node` (workload) -> `hom3recovery` (this crate). The fall-through means a
minimal image with only the recovery console still boots to a usable, visible
surface, and a node-only image reaches the workload without needing the GUI.
Each child is `fork`+`setsid`+`execv`; the parent keeps the pid to supervise.

## 7. Supervise

As PID 1 we must reap every child (orphans reparent to us). We `waitpid(-1)` in
a loop; when the *experience* child exits we restart it after a short backoff.
If it cannot be relaunched, we hold at **safe stop**: sync and power off, rather
than leave an unattended, half-up node running with no experience.

---

### Summary of honest gaps

| Step | Gap | Status |
|---|---|---|
| 3 | wired/USB link-down (netlink) | TODO(next), not faked |
| 4 | at-rest LUKS unlock | TODO(next), not faked |
| 2 | `entropy_avail` is coarse on new kernels | documented; workload should use blocking getrandom |
| — | libc still linked (musl FFI) | small shim; no_std raw-syscall path is short |

Nothing above is hidden in the code; each is marked at its call site.
