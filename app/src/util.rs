pub fn log(msg: &str) {
    use std::io::Write;
    let p = std::env::temp_dir().join("bongowhale.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "[{}] {}", now_ms(), msg);
    }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn now_ms() -> u64 {
    unsafe {
        use windows_sys::Win32::System::SystemInformation::GetTickCount64;
        GetTickCount64()
    }
}

pub fn loword(v: usize) -> u32 {
    (v & 0xFFFF) as u32
}

#[allow(dead_code)]
pub fn hiword(v: usize) -> u32 {
    ((v >> 16) & 0xFFFF) as u32
}

/// Small deterministic RNG so we do not need a crate for blink timing.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + self.next_u32() % (hi - lo)
    }
}
