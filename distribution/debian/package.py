#!/usr/bin/env python3
"""
Builds standard Debian .deb package for ferris-cli
"""
import os
import tarfile
import hashlib
import tempfile
import subprocess
import shutil

VERSION = "0.2.0"
ARCH = "amd64"
PKG_NAME = f"ferris-cli_{VERSION}_{ARCH}"
OUTPUT_DEB = f"{PKG_NAME}.deb"

PROJECT_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))
BINARY_PATH = os.path.join(PROJECT_ROOT, "server/public/downloads/ferris-linux")
LICENSE_PATH = os.path.join(PROJECT_ROOT, "LICENSE-MIT")
README_PATH = os.path.join(PROJECT_ROOT, "README.md")

if not os.path.exists(BINARY_PATH):
    raise SystemExit(f"Binary not found: {BINARY_PATH}")

with tempfile.TemporaryDirectory() as tmpdir:
    # 1. debian-binary
    deb_binary_path = os.path.join(tmpdir, "debian-binary")
    with open(deb_binary_path, "wb") as f:
        f.write(b"2.0\n")

    # 2. Prepare data directory
    data_dir = os.path.join(tmpdir, "data")
    usr_bin = os.path.join(data_dir, "usr/bin")
    doc_dir = os.path.join(data_dir, "usr/share/doc/ferris-cli")
    os.makedirs(usr_bin, exist_ok=True)
    os.makedirs(doc_dir, exist_ok=True)

    dest_bin = os.path.join(usr_bin, "ferris")
    shutil.copy2(BINARY_PATH, dest_bin)
    os.chmod(dest_bin, 0o755)

    dest_lic = os.path.join(doc_dir, "copyright")
    shutil.copy2(LICENSE_PATH, dest_lic)
    os.chmod(dest_lic, 0o644)

    # Calculate md5sums for data files
    md5sums = []
    installed_size = 0
    for root, dirs, files in os.walk(data_dir):
        for f in files:
            full_p = os.path.join(root, f)
            rel_p = os.path.relpath(full_p, data_dir)
            installed_size += os.path.getsize(full_p)
            with open(full_p, "rb") as fp:
                digest = hashlib.md5(fp.read()).hexdigest()
            md5sums.append(f"{digest}  {rel_p}\n")

    # 3. Create data.tar.gz
    data_tar_path = os.path.join(tmpdir, "data.tar.gz")
    with tarfile.open(data_tar_path, "w:gz") as tar:
        for root, dirs, files in os.walk(data_dir):
            for name in dirs + files:
                full_p = os.path.join(root, name)
                arc_p = "./" + os.path.relpath(full_p, data_dir)
                tarinfo = tar.gettarinfo(full_p, arc_p)
                tarinfo.uid = 0
                tarinfo.gid = 0
                tarinfo.uname = "root"
                tarinfo.gname = "root"
                if os.path.isfile(full_p):
                    with open(full_p, "rb") as f:
                        tar.addfile(tarinfo, f)
                else:
                    tar.addfile(tarinfo)

    # 4. Prepare control directory
    control_dir = os.path.join(tmpdir, "control_dir")
    os.makedirs(control_dir, exist_ok=True)

    control_content = f"""Package: ferris-cli
Version: {VERSION}
Architecture: {ARCH}
Maintainer: iskurama <iskurama@ferris.uz>
Installed-Size: {installed_size // 1024}
Section: utils
Priority: optional
Homepage: https://ferris.uz
Description: Sovereign Rust developer platform CLI
 Ferris CLI provides commands for compiling, running, publishing crates,
 managing sovereign workspaces, and interacting with ferris.uz.
"""
    with open(os.path.join(control_dir, "control"), "w", encoding="utf-8") as f:
        f.write(control_content)

    with open(os.path.join(control_dir, "md5sums"), "w", encoding="utf-8") as f:
        f.writelines(md5sums)

    # 5. Create control.tar.gz
    control_tar_path = os.path.join(tmpdir, "control.tar.gz")
    with tarfile.open(control_tar_path, "w:gz") as tar:
        for name in ["control", "md5sums"]:
            full_p = os.path.join(control_dir, name)
            arc_p = "./" + name
            tarinfo = tar.gettarinfo(full_p, arc_p)
            tarinfo.uid = 0
            tarinfo.gid = 0
            tarinfo.uname = "root"
            tarinfo.gname = "root"
            with open(full_p, "rb") as f:
                tar.addfile(tarinfo, f)

    # 6. Create final deb via ar
    out_dir = os.path.dirname(os.path.abspath(__file__))
    final_deb_path = os.path.join(out_dir, OUTPUT_DEB)
    
    # Using 'ar' command
    cmd = ["ar", "-rcD", final_deb_path, "debian-binary", "control.tar.gz", "data.tar.gz"]
    subprocess.check_call(cmd, cwd=tmpdir)
    print(f"Successfully generated Debian package: {final_deb_path}")
