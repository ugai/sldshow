use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const CONF_FILE_EXTENSION: &str = "sldshow";

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    pub window: Window,
    pub viewer: Viewer,
    pub transition: Transition,
    pub style: Style,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Window {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub always_on_top: bool,
    pub titlebar: bool,
    pub resizable: bool,
    pub monitor_index: usize,
    pub cursor_auto_hide: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fullscreen: false,
            always_on_top: false,
            titlebar: false,
            resizable: false,
            monitor_index: 0,
            cursor_auto_hide: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Viewer {
    pub image_paths: Vec<String>,
    pub timer: u32,
    pub scan_subfolders: bool,
    pub shuffle: bool,
    pub pause_at_last: bool,
    pub resize_filter: ResizeFilterType,
    pub stop_screensaver: bool,
    pub cache_extent: usize,
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            image_paths: Vec::new(),
            timer: 10,
            scan_subfolders: false,
            shuffle: false,
            pause_at_last: false,
            resize_filter: ResizeFilterType::Linear,
            stop_screensaver: false,
            cache_extent: 3,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Transition {
    pub time: f32,
    pub fps: f32,
    pub random: bool,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            time: 0.5,
            fps: 30.0,
            random: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Style {
    pub bg_color: [u8; 4],
    pub text_color: [u8; 4],
    pub show_image_path: bool,
    pub font_name: Option<String>,
    pub font_size_osd: f32,
    pub font_size_image_path: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            bg_color: [0, 0, 0, 255],
            text_color: [255, 255, 255, 255],
            show_image_path: false,
            font_name: None,
            font_size_osd: 18.0,
            font_size_image_path: 12.0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ResizeFilterType {
    Nearest,
    Linear,
    Cubic,
    Gaussian,
    Lanczos3,
}

pub fn get_config(path: &Path) -> Result<Config> {
    let config_data = &fs::read_to_string(path)?;
    let config: Config = toml::from_str(config_data)?;

    Ok(config)
}
