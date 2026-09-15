//! Process metadata: executable path, friendly description, icon, liveness.

use std::ffi::c_void;
use std::mem::size_of;
use std::path::Path;

use base64::Engine as _;
use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows_sys::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};

/// Shell icon extraction is not safe to run concurrently from arbitrary
/// threads (it needs COM and shares shell state), so serialize it.
static ICON_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

#[derive(Clone, Debug)]
pub struct ProcessInfo {
    /// Lower-cased executable path, or "system" / "unknown".
    pub key: String,
    pub name: String,
    pub description: String,
    pub exe: String,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn exe_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

pub fn is_alive(pid: u32) -> bool {
    if pid == 0 || pid == 4 {
        return true;
    }
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok != 0 && code == STILL_ACTIVE as u32
    }
}

pub fn info(pid: u32) -> ProcessInfo {
    match pid {
        0 => ProcessInfo {
            key: "unknown".into(),
            name: "Desconocido".into(),
            description: "Tráfico sin proceso identificado".into(),
            exe: String::new(),
        },
        4 => ProcessInfo {
            key: "system".into(),
            name: "System".into(),
            description: "Núcleo de Windows (SMB, actualizaciones, etc.)".into(),
            exe: String::new(),
        },
        _ => match exe_path(pid) {
            Some(exe) => {
                let name = Path::new(&exe)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| exe.clone());
                let description = file_description(&exe).unwrap_or_default();
                ProcessInfo { key: exe.to_lowercase(), name, description, exe }
            }
            None => ProcessInfo {
                key: format!("pid:{pid}"),
                name: format!("PID {pid}"),
                description: "Proceso protegido".into(),
                exe: String::new(),
            },
        },
    }
}

/// `FileDescription` from the executable's version resource.
pub fn file_description(exe: &str) -> Option<String> {
    unsafe {
        let path = wide(exe);
        let size = GetFileVersionInfoSizeW(path.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr() as *mut c_void) == 0 {
            return None;
        }
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        let q = wide("\\VarFileInfo\\Translation");
        if VerQueryValueW(data.as_ptr() as *const c_void, q.as_ptr(), &mut ptr, &mut len) == 0
            || len < 4
        {
            return None;
        }
        let words = std::slice::from_raw_parts(ptr as *const u16, (len / 2) as usize);
        let mut candidates: Vec<(u16, u16)> = words.chunks(2).map(|c| (c[0], c[1])).collect();
        candidates.push((0x0409, 0x04b0));
        candidates.push((0x0409, 0x04e4));
        for (lang, cp) in candidates {
            let q = wide(&format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"));
            let mut p: *mut c_void = std::ptr::null_mut();
            let mut l: u32 = 0;
            if VerQueryValueW(data.as_ptr() as *const c_void, q.as_ptr(), &mut p, &mut l) != 0
                && l > 1
            {
                let s = std::slice::from_raw_parts(p as *const u16, l as usize);
                let s = String::from_utf16_lossy(s);
                let s = s.trim_end_matches('\0').trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }
}

/// Extracts the executable's icon as a `data:image/png;base64,...` URL.
pub fn icon_data_url(exe: &str) -> Option<String> {
    let _guard = ICON_LOCK.lock();
    unsafe {
        let hr = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let result = icon_data_url_inner(exe);
        if hr >= 0 {
            CoUninitialize();
        }
        result
    }
}

unsafe fn icon_data_url_inner(exe: &str) -> Option<String> {
    {
        let path = wide(exe);
        let mut sfi: SHFILEINFOW = std::mem::zeroed();
        let r = SHGetFileInfoW(
            path.as_ptr(),
            0,
            &mut sfi,
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if r == 0 || sfi.hIcon.is_null() {
            return None;
        }
        let result = icon_to_png(sfi.hIcon);
        DestroyIcon(sfi.hIcon);
        result.map(|png| {
            format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
        })
    }
}

unsafe fn icon_to_png(hicon: *mut c_void) -> Option<Vec<u8>> {
    let mut ii: ICONINFO = std::mem::zeroed();
    if GetIconInfo(hicon, &mut ii) == 0 {
        return None;
    }
    let color = ii.hbmColor;
    let mask = ii.hbmMask;

    let mut bm: BITMAP = std::mem::zeroed();
    let src = if color.is_null() { mask } else { color };
    if GetObjectW(src, size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut c_void) == 0 {
        cleanup(color, mask);
        return None;
    }
    let w = bm.bmWidth.max(1) as u32;
    let mut h = bm.bmHeight.max(1) as u32;
    if color.is_null() {
        h /= 2; // monochrome icons stack XOR/AND masks vertically
    }

    let hdc = GetDC(std::ptr::null_mut());
    let mut rgba = dib_bits(hdc, if color.is_null() { mask } else { color }, w, h);
    let has_alpha = rgba.as_ref().map(|p| p.chunks(4).any(|px| px[3] != 0)).unwrap_or(false);
    if let Some(px) = rgba.as_mut() {
        if !has_alpha && !mask.is_null() {
            if let Some(m) = dib_bits(hdc, mask, w, h) {
                for (p, mp) in px.chunks_mut(4).zip(m.chunks(4)) {
                    p[3] = if mp[0] == 0 { 255 } else { 0 };
                }
            } else {
                for p in px.chunks_mut(4) {
                    p[3] = 255;
                }
            }
        }
        // BGRA -> RGBA
        for p in px.chunks_mut(4) {
            p.swap(0, 2);
        }
    }
    ReleaseDC(std::ptr::null_mut(), hdc);
    cleanup(color, mask);

    let px = rgba?;
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&px).ok()?;
    }
    Some(out)
}

unsafe fn cleanup(color: *mut c_void, mask: *mut c_void) {
    if !color.is_null() {
        DeleteObject(color);
    }
    if !mask.is_null() {
        DeleteObject(mask);
    }
}

unsafe fn dib_bits(hdc: *mut c_void, hbm: *mut c_void, w: u32, h: u32) -> Option<Vec<u8>> {
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w as i32,
        biHeight: -(h as i32), // top-down
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB as u32,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let lines = GetDIBits(hdc, hbm, 0, h, buf.as_mut_ptr() as *mut c_void, &mut bmi, DIB_RGB_COLORS);
    if lines == 0 {
        None
    } else {
        Some(buf)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn extracts_explorer_icon() {
        let exe = r"C:\Windows\explorer.exe";
        println!("desc = {:?}", super::file_description(exe));
        let icon = super::icon_data_url(exe);
        println!("icon = {:?}", icon.as_ref().map(|s| s.len()));
        assert!(icon.is_some());
    }
}
