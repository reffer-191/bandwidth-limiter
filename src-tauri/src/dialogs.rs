//! Classic Win32 open/save file dialogs (comdlg32) — enough for importing
//! and exporting rule files without pulling in a dialog plugin.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `filter` is a display name for the JSON pattern, `default_name` the
/// suggested file name. Returns None when the user cancels.
fn run(owner: HWND, save: bool, filter: &str, default_name: &str) -> Option<String> {
    let mut file = [0u16; 1024];
    for (i, c) in default_name.encode_utf16().take(1000).enumerate() {
        file[i] = c;
    }
    // "Name\0*.json\0All files\0*.*\0\0"
    let mut filt: Vec<u16> = Vec::new();
    filt.extend(filter.encode_utf16());
    filt.push(0);
    filt.extend("*.json".encode_utf16());
    filt.push(0);
    filt.extend("*.*".encode_utf16());
    filt.push(0);
    filt.extend("*.*".encode_utf16());
    filt.push(0);
    filt.push(0);
    let ext = wide("json");
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = filt.as_ptr();
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrDefExt = ext.as_ptr();
    ofn.Flags = if save { OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST } else { OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST };
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } };
    if ok == 0 {
        return None;
    }
    let len = file.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&file[..len]))
}

pub fn save_file(owner: HWND, filter: &str, default_name: &str) -> Option<String> {
    run(owner, true, filter, default_name)
}

pub fn open_file(owner: HWND, filter: &str) -> Option<String> {
    run(owner, false, filter, "")
}
