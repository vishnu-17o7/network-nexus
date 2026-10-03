# Original archive executable

The original v0.3.0 project ZIP included `bin/nexus`; the root `SHA256SUMS` verifies that archived executable. It is not the current development build. New binaries and matching asset checksums are published at https://github.com/vishnu-17o7/network-nexus/releases. The release tarball puts `nexus` at its root.

The archived `nexus` is an optimized Linux x86_64 executable.
It requires glibc 2.39 or newer. It does not need Rust installed to run.

From the extracted project directory:

```bash
chmod +x bin/nexus
./bin/nexus
./bin/nexus --doctor
```

On older distributions, ARM CPUs or other libc implementations, install Rust
1.88 or newer and build locally:

```bash
cargo run --locked --release
```

Start as your normal user. Monitoring is local-only by default. The UI asks
before changes and before enabling external requests. NetworkManager actions
use its authorization policy; supported privileged runtime changes use
`pkexec` after confirmation. Optional tools are listed by `--doctor`.

The source, runtime requirements, features and validation limits are described
in the main README and `docs/FEATURES.md` / `docs/VALIDATION.md`.
