//
// Copyright 2019, Google Inc.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
//    * Redistributions of source code must retain the above copyright
// notice, this list of conditions and the following disclaimer.
//    * Redistributions in binary form must reproduce the above
// copyright notice, this list of conditions and the following disclaimer
// in the documentation and/or other materials provided with the
// distribution.
//    * Neither the name of Google Inc. nor the names of its
// contributors may be used to endorse or promote products derived from
// this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// Alternatively, this software may be distributed under the terms of the
// GNU General Public License ("GPL") version 2 as published by the Free
// Software Foundation.
//

use std::ffi::OsStr;
use std::fs;
use std::io::Result as IoResult;
use std::path::Path;
use std::process::{Command, Stdio};

use super::utils;

fn dmidecode_dispatch<S: AsRef<OsStr>>(args: &[S]) -> IoResult<String> {
    let output = Command::new("/usr/sbin/dmidecode")
        .args(args)
        .stdin(Stdio::null())
        .output()?;

    if !output.status.success() {
        return Err(utils::translate_command_error(&output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn get_command_output<S: AsRef<OsStr>>(cmd: &str, args: &[S]) -> Option<String> {
    let output = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if output.status.success() {
        let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !val.is_empty() {
            return Some(val);
        }
    }
    None
}

#[cfg(target_os = "android")]
fn get_android_prop(key: &str) -> Option<String> {
    use std::ffi::{c_char, c_void, CStr, CString};

    extern "C" {
        fn __system_property_read_callback(
            pi: *const libc::prop_info,
            callback: extern "C" fn(
                cookie: *mut c_void,
                name: *const c_char,
                value: *const c_char,
                serial: u32,
            ),
            cookie: *mut c_void,
        );
    }

    extern "C" fn read_cb(
        cookie: *mut c_void,
        _name: *const c_char,
        value: *const c_char,
        _serial: u32,
    ) {
        if value.is_null() {
            return;
        }
        // SAFETY: `cookie` points to `out: &mut String` on the caller's stack,
        // and `value` is a valid NUL-terminated string provided by bionic.
        unsafe {
            let out = &mut *(cookie as *mut String);
            *out = CStr::from_ptr(value).to_string_lossy().trim().to_string();
        }
    }

    let c_key = CString::new(key).ok()?;
    let mut out = String::new();
    // SAFETY: `c_key` is a valid NUL-terminated string, `pi` is checked for
    // null before calling `__system_property_read_callback`, and `out` outlives
    // the synchronous `read_cb` invocation.
    unsafe {
        let pi = libc::__system_property_find(c_key.as_ptr());
        if pi.is_null() {
            return None;
        }
        __system_property_read_callback(
            pi,
            read_cb,
            (&mut out as *mut String).cast::<c_void>(),
        );
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(not(target_os = "android"))]
fn get_android_prop(_key: &str) -> Option<String> {
    None
}

fn get_crossystem_prop(key: &str) -> Option<String> {
    get_command_output("/usr/bin/crossystem", &[key])
        .or_else(|| get_command_output("/vendor/bin/crossystem", &[key]))
}

pub fn os_release() -> IoResult<String> {
    let content = fs::read_to_string("/proc/sys/kernel/osrelease")?;
    let trimmed = content.trim();
    if !trimmed.is_empty() {
        return Ok(trimmed.to_string());
    }
    Err(std::io::ErrorKind::NotFound.into())
}

fn parse_lsb_release(content: &str) -> Option<String> {
    for l in content.lines() {
        if l.starts_with("CHROMEOS_RELEASE_DESCRIPTION") {
            return Some(l.to_string());
        }
    }
    None
}

fn parse_os_release_pretty_name(content: &str) -> Option<String> {
    let mut name = None;
    for l in content.lines() {
        let trimmed = l.trim();
        if trimmed.starts_with("PRETTY_NAME=") {
            return Some(trimmed.to_string());
        } else if trimmed.starts_with("NAME=") && name.is_none() {
            name = Some(trimmed.to_string());
        }
    }
    name
}

fn format_android_release_description(
    desc: Option<&str>,
    version_release: Option<&str>,
    build_id: Option<&str>,
    fingerprint: Option<&str>,
) -> Option<String> {
    let desc = desc.map(str::trim).filter(|s| !s.is_empty());
    let version_release = version_release.map(str::trim).filter(|s| !s.is_empty());
    let build_id = build_id.map(str::trim).filter(|s| !s.is_empty());
    let fingerprint = fingerprint.map(str::trim).filter(|s| !s.is_empty());

    if desc.is_none() && version_release.is_none() && build_id.is_none() && fingerprint.is_none() {
        return None;
    }

    let mut parts = Vec::new();
    if let Some(d) = desc {
        parts.push(d.to_string());
    }
    let mut extra = Vec::new();
    if let Some(vr) = version_release {
        extra.push(format!("Android {}", vr));
    }
    if let Some(id) = build_id {
        extra.push(id.to_string());
    }
    if !extra.is_empty() {
        parts.push(format!("({})", extra.join(", ")));
    } else if let Some(fp) = fingerprint {
        parts.push(format!("({})", fp));
    }

    Some(format!("ANDROID_RELEASE_DESCRIPTION={}", parts.join(" ")))
}

pub fn release_description() -> IoResult<String> {
    // Tier 1: /etc/lsb-release
    if let Ok(content) = fs::read_to_string("/etc/lsb-release") {
        if let Some(desc) = parse_lsb_release(&content) {
            return Ok(desc);
        }
    }

    // Tier 2: Android system properties
    let desc = get_android_prop("ro.build.description");
    let version = get_android_prop("ro.build.version.release");
    let build_id = get_android_prop("ro.build.id");
    let fingerprint = get_android_prop("ro.build.fingerprint");
    if let Some(android_desc) = format_android_release_description(
        desc.as_deref(),
        version.as_deref(),
        build_id.as_deref(),
        fingerprint.as_deref(),
    ) {
        return Ok(android_desc);
    }

    // Tier 3: Linux /etc/os-release
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        if let Some(pretty) = parse_os_release_pretty_name(&content) {
            return Ok(pretty);
        }
    }

    Err(std::io::ErrorKind::NotFound.into())
}

fn read_sysfs_attr(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_dmi_sysfs_system_info(dir: &Path) -> Option<String> {
    let fields = [
        ("Manufacturer", "sys_vendor"),
        ("Product Name", "product_name"),
        ("Version", "product_version"),
        ("Serial Number", "product_serial"),
        ("UUID", "product_uuid"),
        ("SKU Number", "product_sku"),
        ("Family", "product_family"),
    ];

    let mut lines = Vec::new();
    for (label, filename) in fields {
        if let Some(val) = read_sysfs_attr(dir, filename) {
            lines.push(format!("\t{}: {}", label, val));
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("System Information\n{}", lines.join("\n")))
    }
}

fn parse_device_tree_system_info(dir: &Path) -> Option<String> {
    let mut lines = Vec::new();
    if let Ok(content) = fs::read(dir.join("model")) {
        let model = String::from_utf8_lossy(&content)
            .trim_matches('\0')
            .trim()
            .to_string();
        if !model.is_empty() {
            lines.push(format!("\tModel: {}", model));
        }
    }
    if let Ok(content) = fs::read(dir.join("compatible")) {
        let compat = String::from_utf8_lossy(&content)
            .split('\0')
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        if !compat.is_empty() {
            lines.push(format!("\tCompatible: {}", compat));
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("System Information\n{}", lines.join("\n")))
    }
}

fn format_android_system_info(
    manufacturer: Option<&str>,
    model: Option<&str>,
    board: Option<&str>,
    device: Option<&str>,
    sku: Option<&str>,
) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(m) = manufacturer.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("\tManufacturer: {}", m));
    }
    if let Some(m) = model.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("\tProduct Name: {}", m));
    }
    if let Some(b) = board.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("\tBoard: {}", b));
    }
    if let Some(d) = device.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("\tDevice: {}", d));
    }
    if let Some(s) = sku.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("\tSKU: {}", s));
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("System Information\n{}", lines.join("\n")))
    }
}

pub fn system_info() -> IoResult<String> {
    // Tier 1: dmidecode
    if let Ok(info) = dmidecode_dispatch(&["-q", "-t1"]) {
        let trimmed = info.trim();
        if !trimmed.is_empty() {
            return Ok(info);
        }
    }

    // Tier 2: Sysfs DMI (/sys/class/dmi/id)
    if let Some(info) = parse_dmi_sysfs_system_info(Path::new("/sys/class/dmi/id")) {
        return Ok(info);
    }

    // Tier 3: Device Tree (/proc/device-tree)
    if let Some(info) = parse_device_tree_system_info(Path::new("/proc/device-tree")) {
        return Ok(info);
    }

    // Tier 4: Android system properties fallback
    let manufacturer = get_android_prop("ro.product.manufacturer");
    let model = get_android_prop("ro.product.model");
    let board = get_android_prop("ro.product.board");
    let device = get_android_prop("ro.product.device");
    let sku = get_android_prop("ro.boot.hardware.sku");
    if let Some(info) = format_android_system_info(
        manufacturer.as_deref(),
        model.as_deref(),
        board.as_deref(),
        device.as_deref(),
        sku.as_deref(),
    ) {
        return Ok(info);
    }

    Err(std::io::ErrorKind::NotFound.into())
}

fn parse_dmi_sysfs_bios_info(dir: &Path) -> Option<String> {
    let fields = [
        ("Vendor", "bios_vendor"),
        ("Version", "bios_version"),
        ("Release Date", "bios_date"),
        ("BIOS Revision", "bios_release"),
    ];

    let mut lines = Vec::new();
    for (label, filename) in fields {
        if let Some(val) = read_sysfs_attr(dir, filename) {
            lines.push(format!("\t{}: {}", label, val));
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("BIOS Information\n{}", lines.join("\n")))
    }
}

fn format_fallback_bios_info(
    ro_fwid: Option<&str>,
    fwid: Option<&str>,
    bootloader: Option<&str>,
) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(ro) = ro_fwid.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("\tRO FWID: {}", ro));
    }
    if let Some(rw) = fwid.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("\tRW FWID: {}", rw));
    }
    if let Some(bl) = bootloader.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("\tBootloader: {}", bl));
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("BIOS Information\n{}", lines.join("\n")))
    }
}

pub fn bios_info() -> IoResult<String> {
    // Tier 1: dmidecode
    if let Ok(info) = dmidecode_dispatch(&["-q", "-t0"]) {
        let trimmed = info.trim();
        if !trimmed.is_empty() {
            return Ok(info);
        }
    }

    // Tier 2: Sysfs DMI (/sys/class/dmi/id)
    if let Some(info) = parse_dmi_sysfs_bios_info(Path::new("/sys/class/dmi/id")) {
        return Ok(info);
    }

    // Tier 3: crossystem / system properties fallback
    let ro_fwid = get_crossystem_prop("ro_fwid");
    let fwid = get_crossystem_prop("fwid");
    let bootloader = get_android_prop("ro.boot.bootloader");
    if let Some(info) = format_fallback_bios_info(
        ro_fwid.as_deref(),
        fwid.as_deref(),
        bootloader.as_deref(),
    ) {
        return Ok(info);
    }

    Err(std::io::ErrorKind::NotFound.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "flashrom_test_{}_{}",
                prefix,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_parse_lsb_release() {
        let content = concat!(
            "CHROMEOS_RELEASE_BOARD=brya\n",
            "CHROMEOS_RELEASE_DESCRIPTION=16000.0.0 (Official Build) dev-channel\n",
            "CHROMEOS_RELEASE_NAME=Chrome OS\n",
        );
        assert_eq!(
            parse_lsb_release(content),
            Some("CHROMEOS_RELEASE_DESCRIPTION=16000.0.0 (Official Build) dev-channel".to_string())
        );

        assert_eq!(parse_lsb_release("SOMETHING_ELSE=1\n"), None);
    }

    #[test]
    fn test_parse_os_release_pretty_name() {
        let content = "NAME=\"Ubuntu\"\nVERSION=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n";
        assert_eq!(
            parse_os_release_pretty_name(content),
            Some("PRETTY_NAME=\"Ubuntu 24.04 LTS\"".to_string())
        );

        let content_name_only = "NAME=\"Debian GNU/Linux\"\nVERSION_ID=\"12\"\n";
        assert_eq!(
            parse_os_release_pretty_name(content_name_only),
            Some("NAME=\"Debian GNU/Linux\"".to_string())
        );

        assert_eq!(parse_os_release_pretty_name("FOO=bar\n"), None);
    }

    #[test]
    fn test_format_android_release_description() {
        let desc = format_android_release_description(
            Some("brya-userdebug 15 AP4A.241205.013 test-keys"),
            Some("15"),
            Some("AP4A.241205.013"),
            None,
        );
        let expected = concat!(
            "ANDROID_RELEASE_DESCRIPTION=brya-userdebug 15 AP4A.241205.013 test-keys ",
            "(Android 15, AP4A.241205.013)",
        );
        assert_eq!(desc, Some(expected.to_string()));

        let desc_minimal = format_android_release_description(None, Some("16"), None, None);
        assert_eq!(
            desc_minimal,
            Some("ANDROID_RELEASE_DESCRIPTION=(Android 16)".to_string())
        );

        let fp = "google/brya/brya:15/AP4A.241205.013/1:userdebug/test-keys";
        assert_eq!(
            format_android_release_description(None, None, None, Some(fp)),
            Some(format!("ANDROID_RELEASE_DESCRIPTION=({})", fp))
        );
        assert_eq!(
            format_android_release_description(None, Some("15"), None, Some(fp)),
            Some("ANDROID_RELEASE_DESCRIPTION=(Android 15)".to_string())
        );

        assert_eq!(format_android_release_description(None, None, None, None), None);
    }

    #[test]
    fn test_parse_dmi_sysfs_system_info() {
        let temp_dir = TempDir::new("dmi_sys");
        assert_eq!(parse_dmi_sysfs_system_info(temp_dir.path()), None);

        File::create(temp_dir.path().join("sys_vendor"))
            .unwrap()
            .write_all(b"Google\n")
            .unwrap();
        File::create(temp_dir.path().join("product_name"))
            .unwrap()
            .write_all(b"Brya\n")
            .unwrap();
        File::create(temp_dir.path().join("product_version"))
            .unwrap()
            .write_all(b"rev1\n")
            .unwrap();
        File::create(temp_dir.path().join("product_sku"))
            .unwrap()
            .write_all(b"sku123\n")
            .unwrap();

        let info = parse_dmi_sysfs_system_info(temp_dir.path()).unwrap();
        assert!(info.starts_with("System Information"));
        assert!(info.contains("Manufacturer: Google"));
        assert!(info.contains("Product Name: Brya"));
        assert!(info.contains("Version: rev1"));
        assert!(info.contains("SKU Number: sku123"));
    }

    #[test]
    fn test_parse_dmi_sysfs_bios_info() {
        let temp_dir = TempDir::new("dmi_bios");
        assert_eq!(parse_dmi_sysfs_bios_info(temp_dir.path()), None);

        File::create(temp_dir.path().join("bios_vendor"))
            .unwrap()
            .write_all(b"coreboot\n")
            .unwrap();
        File::create(temp_dir.path().join("bios_version"))
            .unwrap()
            .write_all(b"Google_Brya.14505.0.0\n")
            .unwrap();
        File::create(temp_dir.path().join("bios_date"))
            .unwrap()
            .write_all(b"01/01/2026\n")
            .unwrap();

        let info = parse_dmi_sysfs_bios_info(temp_dir.path()).unwrap();
        assert!(info.starts_with("BIOS Information"));
        assert!(info.contains("Vendor: coreboot"));
        assert!(info.contains("Version: Google_Brya.14505.0.0"));
        assert!(info.contains("Release Date: 01/01/2026"));
    }

    #[test]
    fn test_parse_device_tree_system_info() {
        let temp_dir = TempDir::new("dt");
        assert_eq!(parse_device_tree_system_info(temp_dir.path()), None);

        File::create(temp_dir.path().join("model"))
            .unwrap()
            .write_all(b"Google Corsola\0")
            .unwrap();
        File::create(temp_dir.path().join("compatible"))
            .unwrap()
            .write_all(b"google,corsola\0mediatek,mt8186\0")
            .unwrap();

        let info = parse_device_tree_system_info(temp_dir.path()).unwrap();
        assert!(info.starts_with("System Information"));
        assert!(info.contains("Model: Google Corsola"));
        assert!(info.contains("Compatible: google,corsola, mediatek,mt8186"));
    }

    #[test]
    fn test_format_android_system_info() {
        assert_eq!(
            format_android_system_info(None, None, None, None, None),
            None
        );

        let info = format_android_system_info(
            Some("Google"),
            Some("Pixel Tablet"),
            Some("tangorpro_board"),
            Some("tangorpro"),
            Some("sku1"),
        )
        .unwrap();
        assert!(info.starts_with("System Information"));
        assert!(info.contains("Manufacturer: Google"));
        assert!(info.contains("Product Name: Pixel Tablet"));
        assert!(info.contains("Board: tangorpro_board"));
        assert!(info.contains("Device: tangorpro"));
        assert!(info.contains("SKU: sku1"));
    }

    #[test]
    fn test_format_fallback_bios_info() {
        assert_eq!(format_fallback_bios_info(None, None, None), None);

        let info = format_fallback_bios_info(
            Some("Google_Brya.14505.0.0"),
            Some("Google_Brya.14505.1.0"),
            Some("bootloader_v1"),
        )
        .unwrap();
        assert!(info.starts_with("BIOS Information"));
        assert!(info.contains("RO FWID: Google_Brya.14505.0.0"));
        assert!(info.contains("RW FWID: Google_Brya.14505.1.0"));
        assert!(info.contains("Bootloader: bootloader_v1"));

        let corsola_info = format_fallback_bios_info(
            Some("Google_Corsola.15194.0.0"),
            Some("Google_Corsola.15194.1.0"),
            None,
        )
        .unwrap();
        assert_eq!(
            corsola_info,
            "BIOS Information\n\tRO FWID: Google_Corsola.15194.0.0\n\tRW FWID: Google_Corsola.15194.1.0"
        );
    }
}
