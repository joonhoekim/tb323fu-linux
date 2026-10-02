// SPDX-License-Identifier: MIT
//! Android boot images of this tablet: put a new kernel into the user's own
//! stock image (the copy in `boot_b`), and look into a kernel for its version
//! banner. A port of `tools/boot-repack-kernel.py` (same layout rules, same
//! refusals); the tests check the same identity the Python tool's
//! `--self-test` checks: repacking the stock kernel gives the stock image.
//!
//! Layout (header v4, no ramdisk):
//!
//! ```text
//! [0, 4096)             header, only kernel_size changed
//! [4096, ...)           the kernel, zero-padded to a page
//! [payload_end, +16K)   the stock GKI boot signature, copied verbatim
//! [vbmeta_offset, ...)  the stock vbmeta blob, copied verbatim
//! ...zeros...           up to the stock image size
//! [end - 64, end)       AVB footer, offsets moved to the new layout
//! ```

use std::io::Read;

pub type Res<T> = Result<T, String>;

const PAGE: usize = 4096;
const FOOTER_SIZE: usize = 64;
const BOOT_MAGIC: &[u8] = b"ANDROID!";
const OFF_KERNEL_SIZE: usize = 8;
const OFF_RAMDISK_SIZE: usize = 12;
const OFF_HEADER_VERSION: usize = 40;
/// Upper bound for a decompressed kernel (the raw development Image is 57 MB).
const MAX_KERNEL: u64 = 160 << 20;

fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}
fn be32(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(d[at..at + 4].try_into().unwrap())
}
fn be64(d: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(d[at..at + 8].try_into().unwrap())
}
fn pad(n: usize) -> usize {
    n.div_ceil(PAGE) * PAGE
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KernelFormat {
    Raw,
    Gzip,
}

/// A raw arm64 `Image` or a gzip one; nothing else boots here (the bootloader
/// has a gzip decompressor and no LZ4 one).
pub fn kernel_format(k: &[u8]) -> Res<KernelFormat> {
    if k.len() >= 64 && &k[56..60] == b"ARM\x64" {
        return Ok(KernelFormat::Raw);
    }
    if k.len() >= 2 && k[..2] == [0x1f, 0x8b] {
        return Ok(KernelFormat::Gzip);
    }
    if k.len() >= 4 && (k[..4] == [0x02, 0x21, 0x4c, 0x18] || k[..4] == [0x04, 0x22, 0x4d, 0x18]) {
        return Err("the kernel is LZ4-compressed; this bootloader has no LZ4 decompressor".into());
    }
    Err("the kernel is neither a raw arm64 Image nor gzip-compressed".into())
}

/// The kernel as the CPU sees it (gzip undone), bounded.
pub fn kernel_raw(k: &[u8]) -> Res<Vec<u8>> {
    match kernel_format(k)? {
        KernelFormat::Raw => Ok(k.to_vec()),
        KernelFormat::Gzip => {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(k)
                .take(MAX_KERNEL + 1)
                .read_to_end(&mut out)
                .map_err(|e| format!("gzip: {e}"))?;
            if out.len() as u64 > MAX_KERNEL {
                return Err("decompressed kernel too large".into());
            }
            if kernel_format(&out)? != KernelFormat::Raw {
                return Err("the gzip stream does not hold a raw arm64 Image".into());
            }
            Ok(out)
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let first = needle[0];
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        match hay[i..hay.len() - needle.len() + 1].iter().position(|&b| b == first) {
            Some(p) => {
                let at = i + p;
                if &hay[at..at + needle.len()] == needle {
                    return Some(at);
                }
                i = at + 1;
            }
            None => return None,
        }
    }
    None
}

/// The kernel's `linux_banner` for `release` ("Linux version <release> (...) ... #7 SMP ...",
/// without the newline) -- the same text as `/proc/version` of that kernel.
/// An Image carries the banner twice: a placeholder from `init/version.o`
/// with an empty build number ("# SMP PREEMPT ") and the real one linked in
/// last ("#7 SMP PREEMPT <date>"); the one with a build number wins.
pub fn banner(raw: &[u8], release: &str) -> Option<String> {
    let needle = format!("Linux version {release} (");
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(i) = find(&raw[from..], needle.as_bytes()) {
        let at = from + i;
        if let Some(end) = raw[at..].iter().take(512).position(|&b| b == b'\n' || b == 0) {
            if let Ok(s) = String::from_utf8(raw[at..at + end].to_vec()) {
                found.push(s);
            }
        }
        from = at + 1;
    }
    let numbered = |s: &String| s.split(" #").skip(1).any(|t| t.starts_with(|c: char| c.is_ascii_digit()));
    found.iter().find(|s| numbered(s)).or(found.last()).cloned()
}

/// The release and banner of a kernel whose release is not known yet (a file
/// someone picked): the first "Linux version <release> (" whose banner has a
/// build number, else the first well-formed one.
pub fn find_banner(raw: &[u8]) -> Option<(String, String)> {
    let needle = b"Linux version ";
    let mut first = None;
    let mut from = 0;
    while let Some(i) = find(&raw[from..], needle) {
        let at = from + i;
        from = at + 1;
        let rest = &raw[at + needle.len()..];
        let Some(sp) = rest.iter().take(128).position(|&b| b == b' ') else { continue };
        let rel = &rest[..sp];
        if rel.is_empty() || !rest[sp..].starts_with(b" (") || !rel.iter().all(|b| b.is_ascii_graphic()) {
            continue;
        }
        let Ok(rel) = String::from_utf8(rel.to_vec()) else { continue };
        if let Some(b) = banner(raw, &rel) {
            if b.split(" #").skip(1).any(|t| t.starts_with(|c: char| c.is_ascii_digit())) {
                return Some((rel, b));
            }
            first.get_or_insert((rel, b));
        }
    }
    first
}

/// Whether the kernel's built-in initramfs carries the shared modules image
/// of `release` (`/lib/modules/<release>.sqfs`, kernel/initramfs/build.sh -m).
/// Looks for that cpio file name in the Image itself (an uncompressed
/// initramfs) and in every gzip stream inside it (the initramfs is gzip; the
/// others are small, e.g. the IKCONFIG copy). `None`: no initramfs found at all.
pub fn has_modules_image(raw: &[u8], release: &str) -> Option<bool> {
    let name = format!("lib/modules/{release}.sqfs\0");
    let name = name.as_bytes();
    if find(raw, name).is_some() {
        return Some(true);
    }
    let mut saw_cpio = find(raw, b"070701").is_some();
    let mut from = 0;
    let mut tried = 0;
    while let Some(i) = find(&raw[from..], &[0x1f, 0x8b, 0x08]) {
        let at = from + i;
        from = at + 1;
        tried += 1;
        if tried > 256 {
            break;
        }
        let mut dec = flate2::read::GzDecoder::new(&raw[at..]);
        let mut buf = vec![0u8; 1 << 16];
        let mut tail: Vec<u8> = Vec::new();
        let mut total: u64 = 0;
        loop {
            let n = match dec.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            total += n as u64;
            let mut win = std::mem::take(&mut tail);
            win.extend_from_slice(&buf[..n]);
            if total <= (1 << 16) as u64 && win.starts_with(b"070701") {
                saw_cpio = true;
            }
            if find(&win, name).is_some() {
                return Some(true);
            }
            let keep = name.len().min(win.len());
            tail = win[win.len() - keep..].to_vec();
            if total > MAX_KERNEL {
                break;
            }
        }
    }
    if saw_cpio { Some(false) } else { None }
}

/// Whether the raw kernel carries exactly this banner (a line of /proc/version).
pub fn has_banner(raw: &[u8], proc_version: &str) -> bool {
    let v = proc_version.trim_end();
    !v.is_empty() && find(raw, format!("{v}\n").as_bytes()).is_some()
}

/// The parts of a stock boot image the repack copies or replaces.
pub struct StockBoot<'a> {
    data: &'a [u8],
    pub kernel_size: usize,
    footer: &'a [u8],
    footer_major: u32,
    footer_minor: u32,
    pub vbmeta_offset: usize,
    vbmeta: &'a [u8],
    boot_signature: &'a [u8],
}

impl<'a> StockBoot<'a> {
    pub fn parse(data: &'a [u8]) -> Res<Self> {
        if data.len() < 2 * PAGE || &data[..8] != BOOT_MAGIC {
            return Err("not an Android boot image".into());
        }
        let hv = le32(data, OFF_HEADER_VERSION);
        if hv != 4 {
            return Err(format!("header version {hv} not supported (want 4)"));
        }
        let kernel_size = le32(data, OFF_KERNEL_SIZE) as usize;
        let ramdisk = le32(data, OFF_RAMDISK_SIZE);
        if ramdisk != 0 {
            return Err(format!("image has a {ramdisk}-byte ramdisk; not supported"));
        }
        if PAGE + kernel_size > data.len() {
            return Err("kernel_size runs past the image".into());
        }
        kernel_format(&data[PAGE..PAGE + kernel_size]).map_err(|e| format!("stock kernel: {e}"))?;
        let footer_at = data.len() - FOOTER_SIZE;
        let footer = &data[footer_at..];
        if &footer[..4] != b"AVBf" {
            return Err("no AVB footer at the end of the image".into());
        }
        let (footer_major, footer_minor) = (be32(footer, 4), be32(footer, 8));
        let original = be64(footer, 12) as usize;
        let vbmeta_offset = be64(footer, 20) as usize;
        let vbmeta_size = be64(footer, 28) as usize;
        if vbmeta_offset.checked_add(vbmeta_size).is_none_or(|e| e > footer_at) {
            return Err("vbmeta blob outside the image".into());
        }
        let vbmeta = &data[vbmeta_offset..vbmeta_offset + vbmeta_size];
        if vbmeta.len() < 4 || &vbmeta[..4] != b"AVB0" {
            return Err("vbmeta blob does not start with AVB0".into());
        }
        let payload_end = pad(PAGE + kernel_size);
        if original != vbmeta_offset {
            return Err(format!("footer original_image_size={original} is not vbmeta_offset={vbmeta_offset}; layout not supported"));
        }
        if original < payload_end {
            return Err(format!("payload ends at {payload_end}, past the vbmeta blob"));
        }
        let boot_signature = &data[payload_end..original];
        if !boot_signature.is_empty() && &boot_signature[..4.min(boot_signature.len())] != b"AVB0" {
            return Err(format!("the {} bytes after the kernel are not an AVB0 block", boot_signature.len()));
        }
        Ok(StockBoot { data, kernel_size, footer, footer_major, footer_minor, vbmeta_offset, vbmeta, boot_signature })
    }

    pub fn kernel(&self) -> &'a [u8] {
        &self.data[PAGE..PAGE + self.kernel_size]
    }

    /// A full-size image with `kernel` in place of the stock one (the stock GKI
    /// boot signature kept, as the Python tool does by default).
    pub fn repack(&self, kernel: &[u8]) -> Res<Vec<u8>> {
        kernel_format(kernel)?;
        let mut out = Vec::with_capacity(self.data.len());
        out.extend_from_slice(&self.data[..PAGE]);
        out[OFF_KERNEL_SIZE..OFF_KERNEL_SIZE + 4].copy_from_slice(&(kernel.len() as u32).to_le_bytes());
        out.extend_from_slice(kernel);
        out.resize(pad(out.len()), 0);
        out.extend_from_slice(self.boot_signature);
        out.resize(pad(out.len()), 0);
        let vbmeta_offset = out.len();
        out.extend_from_slice(self.vbmeta);
        if out.len() > self.data.len() - FOOTER_SIZE {
            return Err(format!("the repacked image ({} bytes + footer) does not fit the {}-byte partition", out.len(), self.data.len()));
        }
        out.resize(self.data.len() - FOOTER_SIZE, 0);
        out.extend_from_slice(b"AVBf");
        out.extend_from_slice(&self.footer_major.to_be_bytes());
        out.extend_from_slice(&self.footer_minor.to_be_bytes());
        out.extend_from_slice(&(vbmeta_offset as u64).to_be_bytes());
        out.extend_from_slice(&(vbmeta_offset as u64).to_be_bytes());
        out.extend_from_slice(&(self.vbmeta.len() as u64).to_be_bytes());
        out.extend_from_slice(&self.footer[36..]);
        debug_assert_eq!(out.len(), self.data.len());
        Ok(out)
    }
}

/// The kernel field of a (repacked) boot image.
pub fn boot_kernel(img: &[u8]) -> Res<&[u8]> {
    if img.len() < 2 * PAGE || &img[..8] != BOOT_MAGIC {
        return Err("not an Android boot image".into());
    }
    if le32(img, OFF_HEADER_VERSION) != 4 {
        return Err("not a header v4 boot image".into());
    }
    let n = le32(img, OFF_KERNEL_SIZE) as usize;
    img.get(PAGE..PAGE + n).ok_or_else(|| "kernel_size runs past the image".into())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A fake raw arm64 Image of `n` bytes carrying a version banner.
    pub fn fake_kernel(n: usize, banner: &str) -> Vec<u8> {
        let mut k = vec![0u8; n.max(4096)];
        k[56..60].copy_from_slice(b"ARM\x64");
        for (i, b) in k.iter_mut().enumerate().skip(64) {
            *b = (i * 7 % 251) as u8 | 1; // no zero bytes, no newlines by accident below
            if *b == b'\n' {
                *b = b'.';
            }
        }
        // the placeholder banner of init/version.o comes first in a real Image
        if let Some((head, _)) = banner.split_once(" #") {
            let ph = format!("{head} # SMP PREEMPT \n");
            k[300..300 + ph.len()].copy_from_slice(ph.as_bytes());
        }
        let line = format!("{banner}\n");
        let at = 1000;
        k[at..at + line.len()].copy_from_slice(line.as_bytes());
        k[at + line.len()] = 0;
        k
    }

    /// A stock-shaped boot image: header v4, kernel, 16 KiB GKI signature,
    /// vbmeta, zeros, AVB footer -- `size` bytes in all.
    pub fn fake_stock(size: usize, kernel: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; PAGE];
        d[..8].copy_from_slice(BOOT_MAGIC);
        d[OFF_KERNEL_SIZE..OFF_KERNEL_SIZE + 4].copy_from_slice(&(kernel.len() as u32).to_le_bytes());
        d[OFF_HEADER_VERSION..OFF_HEADER_VERSION + 4].copy_from_slice(&4u32.to_le_bytes());
        d[64..80].copy_from_slice(b"stock cmdline...");
        d.extend_from_slice(kernel);
        d.resize(pad(d.len()), 0);
        let mut sig = vec![0x5au8; 16384];
        sig[..4].copy_from_slice(b"AVB0");
        d.extend_from_slice(&sig);
        let vb_off = d.len();
        let mut vb = vec![0xa5u8; 2048];
        vb[..4].copy_from_slice(b"AVB0");
        d.extend_from_slice(&vb);
        d.resize(size - FOOTER_SIZE, 0);
        d.extend_from_slice(b"AVBf");
        d.extend_from_slice(&1u32.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&(vb_off as u64).to_be_bytes());
        d.extend_from_slice(&(vb_off as u64).to_be_bytes());
        d.extend_from_slice(&(vb.len() as u64).to_be_bytes());
        d.extend_from_slice(&[0u8; 28]);
        d
    }

    #[test]
    fn identity_and_new_kernel() {
        let k = fake_kernel(300_001, "Linux version 6.6.0-stock (b@h) (clang) #1 SMP PREEMPT");
        let stock = fake_stock(4 << 20, &k);
        let s = StockBoot::parse(&stock).unwrap();
        assert_eq!(s.repack(s.kernel()).unwrap(), stock, "self-test: stock kernel gives the stock image");

        let nk = fake_kernel(1_234_567, "Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang 19) #1 SMP PREEMPT Thu Oct 2");
        let out = s.repack(&nk).unwrap();
        assert_eq!(out.len(), stock.len());
        assert_eq!(boot_kernel(&out).unwrap(), &nk[..]);
        let again = StockBoot::parse(&out).unwrap();
        assert_eq!(again.kernel(), &nk[..]);
        assert_eq!(again.repack(s.kernel()).unwrap(), stock, "back to the stock kernel");
        let raw = kernel_raw(boot_kernel(&out).unwrap()).unwrap();
        assert_eq!(banner(&raw, "7.3.0-rc4-tb323fu-t28").unwrap(),
            "Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang 19) #1 SMP PREEMPT Thu Oct 2");
        assert!(banner(&raw, "7.3.0-rc4-tb323fu-t27").is_none());
        assert!(has_banner(&raw, "Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang 19) #1 SMP PREEMPT Thu Oct 2\n"));
        assert!(!has_banner(&raw, "Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang 19) #2 SMP PREEMPT Thu Oct 2"));
    }

    /// Real images (not in the repository): TB323FU_REAL_STOCK (a stock boot
    /// image), TB323FU_REAL_KERNEL (an Image or Image.gz) and TB323FU_REAL_OUT
    /// (what tools/boot-repack-kernel.py made of the two) must agree byte for
    /// byte. `cargo test -- --ignored real_images`
    #[test]
    #[ignore]
    fn real_images() {
        let get = |k: &str| std::fs::read(std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"))).unwrap();
        let (stock, kernel, want) = (get("TB323FU_REAL_STOCK"), get("TB323FU_REAL_KERNEL"), get("TB323FU_REAL_OUT"));
        let s = StockBoot::parse(&stock).unwrap();
        assert_eq!(s.repack(s.kernel()).unwrap(), stock, "self-test on the real stock image");
        let out = s.repack(&kernel).unwrap();
        assert!(out == want, "differs from the Python tool's image");
        let raw = kernel_raw(&kernel).unwrap();
        // the release from the first well-formed banner, and banner() finds it
        let text = String::from_utf8_lossy(&raw);
        let rel = text.match_indices("Linux version ").find_map(|(i, _)| {
            let mut w = text[i + 14..].split(' ');
            let (r, by) = (w.next()?, w.next()?);
            by.starts_with('(').then(|| r.to_string())
        }).expect("no banner");
        let b = banner(&raw, &rel).expect("banner() finds the release");
        assert!(b.contains(" #") && !b.contains(" # "), "the numbered banner, not the placeholder");
        eprintln!("banner: {b}");
    }

    /// A fake Image with a gzip'd newc cpio built in (`files`: names), as
    /// CONFIG_INITRAMFS_SOURCE puts it.
    pub fn fake_kernel_with_initramfs(n: usize, banner: &str, files: &[&str]) -> Vec<u8> {
        use std::io::Write;
        let mut cpio = Vec::new();
        for f in files.iter().chain(["TRAILER!!!"].iter()) {
            let name = format!("{f}\0");
            cpio.extend_from_slice(format!("070701{:08X}{:0>80}{:08X}{:08X}", 1, 0, name.len(), 0).as_bytes());
            cpio.extend_from_slice(name.as_bytes());
            while cpio.len() % 4 != 0 {
                cpio.push(0);
            }
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&cpio).unwrap();
        gz.write_all(&vec![0u8; 300_000]).unwrap();
        let gz = gz.finish().unwrap();
        let mut k = fake_kernel(n, banner);
        let at = 8192;
        k[at..at + gz.len()].copy_from_slice(&gz);
        k
    }

    #[test]
    fn banner_and_modules_image_of_an_unknown_file() {
        let v = "Linux version 7.3.0-rc4-tb323fu-t30-jh (u@h) (clang 19) #8 SMP PREEMPT Fri Oct  3";
        let k = fake_kernel_with_initramfs(200_000, v, &["init", "lib/modules/7.3.0-rc4-tb323fu-t30-jh", "lib/modules/7.3.0-rc4-tb323fu-t30-jh.sqfs"]);
        let (rel, b) = find_banner(&k).unwrap();
        assert_eq!((rel.as_str(), b.as_str()), ("7.3.0-rc4-tb323fu-t30-jh", v), "the numbered banner, not the placeholder");
        assert_eq!(has_modules_image(&k, &rel), Some(true));
        assert_eq!(has_modules_image(&k, "7.3.0-other"), Some(false));
        let own = fake_kernel_with_initramfs(200_000, v, &["init", "lib/modules/qcom_q6v5_pas.ko"]);
        assert_eq!(has_modules_image(&own, &rel), Some(false), "an initramfs without the modules image");
        assert_eq!(has_modules_image(&fake_kernel(200_000, v), &rel), None, "no initramfs at all");
        assert!(find_banner(&fake_kernel(5000, "no banner here")).is_none());
    }

    /// A real kernel (TB323FU_REAL_KERNEL, an Image or Image.gz): its banner
    /// is found without knowing the release, and the shared modules image is
    /// found in its initramfs exactly when TB323FU_REAL_SHARED=1.
    /// `cargo test -- --ignored real_kernel_inspect`
    #[test]
    #[ignore]
    fn real_kernel_inspect() {
        let k = std::fs::read(std::env::var("TB323FU_REAL_KERNEL").expect("TB323FU_REAL_KERNEL not set")).unwrap();
        let raw = kernel_raw(&k).unwrap();
        let t = std::time::Instant::now();
        let (rel, b) = find_banner(&raw).expect("no banner");
        let m = has_modules_image(&raw, &rel);
        eprintln!("release {rel}\nbanner  {b}\nmodules {m:?} ({} ms)", t.elapsed().as_millis());
        assert!(b.contains(" #") && !b.contains(" # "), "the numbered banner");
        assert_eq!(m, Some(std::env::var("TB323FU_REAL_SHARED").as_deref() == Ok("1")));
    }

    #[test]
    fn gzip_and_refusals() {
        use std::io::Write;
        let k = fake_kernel(200_000, "Linux version 7.3.0-x (a@b) (c) #3 SMP");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&k).unwrap();
        let gz = gz.finish().unwrap();
        assert_eq!(kernel_format(&gz).unwrap(), KernelFormat::Gzip);
        assert_eq!(kernel_raw(&gz).unwrap(), k);
        assert!(kernel_format(&[0x04, 0x22, 0x4d, 0x18, 0, 0]).unwrap_err().contains("LZ4"));
        assert!(kernel_format(b"hello world").is_err());

        let stock = fake_stock(1 << 20, &k);
        let s = StockBoot::parse(&stock).unwrap();
        let big = fake_kernel(2 << 20, "Linux version big (a@b) (c) #1");
        assert!(s.repack(&big).unwrap_err().contains("does not fit"));
        let mut bad = stock.clone();
        bad[OFF_HEADER_VERSION] = 3;
        assert!(StockBoot::parse(&bad).is_err());
        let mut bad = stock.clone();
        let n = bad.len();
        bad[n - 64] = b'x';
        assert!(StockBoot::parse(&bad).err().unwrap().contains("AVB footer"));
    }
}
