#!/usr/bin/env python3
"""Build a .deb and binary archive, without installing or changing networking."""
import argparse, gzip, hashlib, os, re, shutil, subprocess, tarfile, tempfile
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # Ubuntu 22.04 system Python 3.10
from pathlib import Path

root = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, default=root / 'target/release/nexus')
p.add_argument('--out', type=Path, default=root / 'dist')
p.add_argument('--max-glibc', help='Fail if the executable requires a newer glibc, e.g. 2.35')
args = p.parse_args()
binary = args.binary.resolve()
if not binary.is_file():
    raise SystemExit('Build first: cargo build --locked --release')
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
if not re.fullmatch(r'\d+\.\d+\.\d+(?:[+~.-][a-zA-Z0-9.+~-]+)?', version):
    raise SystemExit('Invalid package version')
header = subprocess.check_output(['readelf', '-h', str(binary)], text=True)
arch = 'amd64' if 'Advanced Micro Devices X86-64' in header else 'arm64' if 'AArch64' in header else None
if arch is None:
    raise SystemExit('Packaging currently supports amd64 and arm64 ELF binaries')
symbols = subprocess.check_output(['readelf', '--version-info', str(binary)], text=True)
glibc = max(set(re.findall(r'GLIBC_(\d+\.\d+(?:\.\d+)?)', symbols)), key=lambda v: tuple(map(int, v.split('.'))))
if args.max_glibc:
    if not re.fullmatch(r'\d+\.\d+(?:\.\d+)?', args.max_glibc):
        raise SystemExit('Invalid --max-glibc version')
    if tuple(map(int, glibc.split('.'))) > tuple(map(int, args.max_glibc.split('.'))):
        raise SystemExit(f'Executable requires GLIBC {glibc}; release limit is {args.max_glibc}')
args.out.mkdir(parents=True, exist_ok=True)
out = args.out.resolve()
with tempfile.TemporaryDirectory(prefix='nexus-package-') as tmp:
    stage = Path(tmp)
    stage.chmod(0o755)
    (stage / 'DEBIAN').mkdir()
    (stage / 'usr/bin').mkdir(parents=True)
    doc = stage / 'usr/share/doc/nexus-net'; doc.mkdir(parents=True)
    shutil.copy2(binary, stage / 'usr/bin/nexus'); (stage / 'usr/bin/nexus').chmod(0o755)
    shutil.copy2(root / 'README.md', doc / 'README.md')
    shutil.copy2(root / 'LICENSE', doc / 'copyright')
    shutil.copy2(root / 'config.example.toml', doc / 'config.example.toml')
    changelog = f'nexus-net ({version}) unstable; urgency=medium\n\n  * Ubuntu 22.04-compatible release baseline and verified Debian installation.\n\n -- NEXUS contributors <noreply@users.noreply.github.com>  Sat, 03 Oct 2026 00:00:00 +0000\n'
    (doc / 'changelog.gz').write_bytes(gzip.compress(changelog.encode(), mtime=0))
    man = stage / 'usr/share/man/man1'; man.mkdir(parents=True)
    (man / 'nexus.1.gz').write_bytes(gzip.compress((root / 'packaging/nexus.1').read_bytes(), mtime=0))
    desktop = stage / 'usr/share/applications'; desktop.mkdir(parents=True)
    shutil.copy2(root / 'packaging/nexus-net.desktop', desktop / 'nexus-net.desktop')
    size = sum(f.stat().st_size for f in stage.rglob('*') if f.is_file()) // 1024 + 1
    (stage / 'DEBIAN/control').write_text(f'''Package: nexus-net
Version: {version}
Section: net
Priority: optional
Architecture: {arch}
Maintainer: NEXUS contributors <noreply@users.noreply.github.com>
Installed-Size: {size}
Depends: libc6 (>= {glibc}), libgcc-s1 (>= 3.0), ca-certificates
Recommends: iproute2, iputils-ping, dnsutils, curl, openssl
Suggests: network-manager, iw, ethtool, traceroute, mtr-tiny, iperf3, nethogs, nftables, policykit-1
Description: Terminal-native Linux network control center
 Inspect networking, diagnose DNS/TLS/HTTP failures, monitor live traffic,
 and preview supported network configuration changes from a full-screen TUI.
 Monitoring works without root wherever Linux permissions permit.
''')
    for file in stage.rglob('*'):
        if file.is_file() and file != stage / 'usr/bin/nexus': file.chmod(0o644)
    subprocess.run(['dpkg-deb', '--root-owner-group', '--build', str(stage), str(out / f'nexus-net_{version}_{arch}.deb')], check=True)
archive = out / f'nexus-{version}-linux-{arch}.tar.gz'
with tarfile.open(archive, 'w:gz') as tar:
    tar.add(binary, arcname='nexus'); tar.add(root / 'LICENSE', arcname='LICENSE'); tar.add(root / 'README.md', arcname='README.md')
files = [out / f'nexus-net_{version}_{arch}.deb', archive]
(out / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(f.read_bytes()).hexdigest()}  {f.name}\n' for f in files))
print(f'Built {arch}; minimum GLIBC {glibc}. No installation or network configuration performed.')
