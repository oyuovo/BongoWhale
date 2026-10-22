use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE: &str = "BongoWhale";

fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub fn is_enabled() -> bool {
    unsafe {
        use windows_sys::Win32::System::Registry::RegGetValueW;
        let sub = crate::util::wide(RUN_KEY);
        let name = crate::util::wide(VALUE);
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let rc = RegGetValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            name.as_ptr(),
            0x2, // RRF_RT_REG_SZ
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
        );
        rc == 0
    }
}

pub fn set(enabled: bool) {
    unsafe {
        let sub = crate::util::wide(RUN_KEY);
        let mut key: HKEY = std::ptr::null_mut();
        let rc = RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_SET_VALUE, &mut key);
        if rc != ERROR_SUCCESS {
            return;
        }
        let name = crate::util::wide(VALUE);
        if enabled {
            let val = crate::util::wide(&format!("\"{}\"", exe_path()));
            let bytes = std::slice::from_raw_parts(val.as_ptr() as *const u8, val.len() * 2);
            RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, bytes.as_ptr(), bytes.len() as u32);
        } else {
            RegDeleteValueW(key, name.as_ptr());
        }
        RegCloseKey(key);
    }
}
