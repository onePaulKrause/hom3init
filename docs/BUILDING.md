# Building hom3init

`hom3init` is a plain Cargo project with no external dependencies. It builds two
binaries: `hom3init` (PID 1) and `hom3recovery` (the recovery console).

## 1. Standalone (host toolchain)

Requires a recent stable Rust via `rustup`. The pinned toolchain and target are
declared in `rust-toolchain.toml`, so `rustup` will fetch them automatically.

```bash
# target added automatically by rust-toolchain.toml on first build,
# or add it explicitly:
rustup target add aarch64-unknown-linux-musl

cargo build --release --target aarch64-unknown-linux-musl
```

Outputs:

```
target/aarch64-unknown-linux-musl/release/hom3init
target/aarch64-unknown-linux-musl/release/hom3recovery
```

**Why musl / static:** an init must run before any dynamic loader or shared
library is guaranteed present. A statically linked musl binary has no runtime
loader dependency, which is exactly what you want for `/sbin/init`.

Sanity check the result is static:

```bash
file target/aarch64-unknown-linux-musl/release/hom3init
# ... ELF 64-bit LSB executable, ARM aarch64, statically linked, ...
```

## 2. Installing as /sbin/init

On the target root filesystem:

```bash
install -Dm755 hom3init      "$ROOTFS/usr/bin/hom3init"
install -Dm755 hom3recovery  "$ROOTFS/usr/bin/hom3recovery"
ln -sf /usr/bin/hom3init     "$ROOTFS/sbin/init"
```

Then tell the kernel to use it. With `extlinux`/U-Boot, the kernel runs
`/sbin/init` by default once the rootfs is mounted; no `init=` override is
needed if the symlink above exists. To be explicit, add to the kernel cmdline:

```
init=/sbin/init
```

`hom3init` expects to be **PID 1**. It will mount `proc`/`sys`/`dev` itself, so
the rootfs does not need a prior mount step. It looks for the launch targets at:

```
/usr/bin/hom3home       (optional graphic experience)
/usr/bin/hom3node        (optional workload)
/usr/bin/hom3recovery    (this crate - the fallback surface)
```

If only `hom3recovery` is present, the board boots to the recovery console -
which is the intended "first boot lands somewhere visible" behavior.

## 3. Buildroot integration (BR2_EXTERNAL)

If you build a full image with Buildroot, package `hom3init` as a local-source
cargo package in your external tree. Minimal `.mk`:

```make
################################################################################
# hom3init - native PID 1 (local-source cargo package, zero deps)
################################################################################
HOM3INIT_VERSION = 0.1.0
HOM3INIT_SITE = $(BR2_EXTERNAL_<YOURNS>_PATH)/src/hom3init
HOM3INIT_SITE_METHOD = local
HOM3INIT_LICENSE = MIT

# replace the distro init: point /sbin/init at our binary
define HOM3INIT_SET_INIT
	ln -sf /usr/bin/hom3init $(TARGET_DIR)/sbin/init
endef
HOM3INIT_POST_INSTALL_TARGET_HOOKS += HOM3INIT_SET_INIT

$(eval $(cargo-package))
```

with a `Config.in`:

```
config BR2_PACKAGE_HOM3INIT
	bool "hom3init"
	depends on BR2_PACKAGE_HOST_RUSTC_TARGET_ARCH_SUPPORTS
	select BR2_PACKAGE_HOST_RUSTC
	help
	  Sovereign PID 1 (init) - zero external crates.
```

Notes:

- Because there are **no external crates**, the cargo build is fully offline -
  nothing is fetched at build time. This is intentional for airgap builds.
- Set the init system to "none / custom" in Buildroot's *System configuration*
  so it does not install a competing init; the symlink hook above owns
  `/sbin/init`.
- If you also want a serial login for debugging, enable a getty on your console
  tty separately. By design `hom3init` exposes **no shell** - it launches the
  experience or the recovery console and supervises.

## 4. Reproducibility

- The toolchain is pinned in `rust-toolchain.toml`. Bump it deliberately, not
  incidentally, so image hashes stay stable across machines.
- `Cargo.lock` is git-ignored because there are no dependencies to lock. If you
  add any (you shouldn't, for the init), commit the lockfile.
- `panic = "abort"` and `strip = true` are set in `Cargo.toml`: an init does not
  unwind, and a stripped, size-optimized binary keeps the trusted surface small.
