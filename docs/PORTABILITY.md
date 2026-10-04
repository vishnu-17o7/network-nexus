# Platform support

## Release 0.5.1

| Platform | Status | Distribution |
| --- | --- | --- |
| Ubuntu 22.04 / 24.04, x86_64 | Build, Rust tests, PTY checks and Debian installation are CI targets | `.deb` and `.tar.gz` |
| Other glibc Linux, x86_64 | Archive targets glibc 2.35+; distro-specific desktop/network tools need validation | `.tar.gz`, or source build |
| Older glibc Linux | No compatibility claim for the prebuilt GNU binary | Build from source |
| ARM64 Linux / 64-bit Raspberry Pi OS Bookworm or newer | Native ARM64 build/test; exact packages checked on Debian 12/13 ARM64; physical Pi hardware unverified | ARM64 `.deb` and `.tar.gz` |
| 32-bit Raspberry Pi OS (`armhf`), original Pi Zero / Pi 1 | No 32-bit release | Requires a separate compatible build |
| Alpine / musl Linux | No static-musl release or CI coverage yet | Separate musl build and testing required |
| macOS, Intel / Apple Silicon | Native backend not implemented | No native release |
| Windows, x64 / ARM64 | Native backend not implemented | No native release |

The release workflow builds in an Ubuntu 22.04 container and measures imported
GLIBC symbol versions from the resulting executable. It rejects any requirement
newer than 2.35. Merely lowering the Debian dependency without rebuilding would
not fix an incompatible executable.

Ubuntu 22.04 and 24.04 CI jobs also install their generated Debian packages and
run the installed executable. Native ARM64 jobs test the release package in
Debian 12 and 13 userspaces, including the terminal interaction suites. A successful container check does not establish
that every Wi-Fi driver, NetworkManager policy or desktop permission works.

## Broader Linux distribution support

Most of the implementation is already shared across Linux distributions. Native
ARM64 release builds are included. Further coverage should include 32-bit Pi OS
and a static
`x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl` build, followed by
runtime checks on representative glibc and musl distributions. Static linking
reduces libc dependencies; it does not bundle kernel features, CA certificates
or optional utilities such as NetworkManager, Tailscale and iproute2.

A standalone archive is usable across package-manager families when its runtime
requirements are met. Native RPM / other distro packages are additional packaging
work. No single executable covers different operating systems and CPU architectures.

## Windows and macOS

The Ratatui UI, charts, application models, history, HTTP/TLS diagnostics and
Pi-hole API can largely be shared. System networking needs separate implementations.

The current `NetworkBackend` trait only abstracts snapshots. Linux references
also exist in command discovery, diagnostics, interface/vendor lookup and
configuration controls. Porting requires platform modules with compile-time
`#[cfg(...)]` selection, platform-specific commands/APIs and capability reporting
for each action. Unsupported actions should be clearly unavailable in the UI.

- **Windows:** IP Helper APIs for adapters, counters, routes and connections;
  suitable Windows facilities for DNS/Wi-Fi and elevated configuration changes.
- **macOS:** system networking APIs and SystemConfiguration for interface/service
  configuration; macOS-specific route/counter discovery, Wi-Fi access and
  authorization handling.
- **Both:** native build/test runners, real terminal tests, platform permissions,
  cancellation and restoration checks, and installable signed releases where needed.

Monitoring and common diagnostics are a smaller first port than complete
configuration parity. Wi-Fi, firewall and interface changes require the most
platform-specific validation. A Windows executable that launches inside WSL
would inspect the Linux environment; it is not a substitute for a native Windows backend.

Smooth charts depend on the terminal's graphics protocol, separately from OS
support. Text graphs remain necessary for terminals without supported graphics.

## References

- [Raspberry Pi OS architecture](https://www.raspberrypi.com/documentation/computers/os.html)
- [Raspberry Pi 64-bit models](https://www.raspberrypi.com/news/raspberry-pi-os-64-bit/)
- [Rust platform targets](https://doc.rust-lang.org/rustc/platform-support.html)
- [Rust C runtime linkage](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes)
- [Windows adapter enumeration](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getadaptersaddresses)
- [Apple SystemConfiguration](https://developer.apple.com/documentation/systemconfiguration/scnetworkconfiguration)
