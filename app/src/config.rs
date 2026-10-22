use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    /// 0.5 - 2.0, relative to the 256px design size.
    pub scale: f32,
    /// 0.2 - 1.0 window opacity.
    pub opacity: f32,
    /// Offset from the anchor corner of the chosen monitor, in physical pixels.
    pub offset_x: i32,
    pub offset_y: i32,
    /// 0 = primary, otherwise the monitor index in EnumDisplayMonitors order.
    pub monitor: usize,
    /// Which corner to anchor to: "bottom-right", "bottom-left", "top-right", "top-left".
    pub anchor: String,
    pub always_on_top: bool,
    pub click_through: bool,
    pub keyboard_hook: bool,
    pub paused: bool,
    pub sleep_after_secs: u64,
    pub start_with_windows: bool,
    /// Play the tap animation on mouse clicks as well.
    pub react_to_mouse: bool,
    /// Fade the pet out and let clicks through while the pointer rests on it.
    pub hide_on_hover: bool,
    /// How long the pointer must rest before the fade starts.
    pub hover_hide_delay_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            scale: 1.0,
            opacity: 1.0,
            offset_x: 24,
            offset_y: 24,
            monitor: 0,
            anchor: "bottom-right".into(),
            always_on_top: true,
            // 关掉"完全穿透"：分层窗口本身就按逐像素 alpha 命中，
            // 点到透明处会穿透，点到角色才响应。
            click_through: false,
            keyboard_hook: false,
            paused: false,
            sleep_after_secs: 120,
            start_with_windows: false,
            react_to_mouse: true,
            hide_on_hover: true,
            hover_hide_delay_ms: 1200,
        }
    }
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("BongoWhale")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        let p = config_path();
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(config_path(), s);
        }
    }

    pub fn clamp(&mut self) {
        self.scale = self.scale.clamp(0.5, 2.5);
        self.opacity = self.opacity.clamp(0.2, 1.0);
        if self.sleep_after_secs < 15 {
            self.sleep_after_secs = 15;
        }
        self.hover_hide_delay_ms = self.hover_hide_delay_ms.clamp(200, 10_000);
    }
}
