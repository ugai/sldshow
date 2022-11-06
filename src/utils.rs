#[cfg(windows)]
use crate::common_win32;

use crate::config::{ResizeFilterType, CONF_FILE_EXTENSION};
use copypasta::{ClipboardContext, ClipboardProvider};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use winit::{dpi::PhysicalPosition, monitor::MonitorHandle, window::Window};

pub const fn convert_filter_type(src: &ResizeFilterType) -> image::imageops::FilterType {
    match src {
        ResizeFilterType::Nearest => image::imageops::FilterType::Nearest,
        ResizeFilterType::Linear => image::imageops::FilterType::Triangle,
        ResizeFilterType::Cubic => image::imageops::FilterType::CatmullRom,
        ResizeFilterType::Gaussian => image::imageops::FilterType::Gaussian,
        ResizeFilterType::Lanczos3 => image::imageops::FilterType::Lanczos3,
    }
}

pub fn rgba_u8_to_f32(input: [u8; 4]) -> [f32; 4] {
    let mut output = [0.0; 4];
    for i in 0..4 {
        output[i] = (input[i] as f32) / 255.0;
    }
    output
}

pub fn get_double_click_duration() -> Duration {
    #[cfg(windows)]
    let double_click_time = common_win32::get_double_click_time_ms();
    #[cfg(not(windows))]
    let double_click_time = 500;

    Duration::from_millis(double_click_time as u64)
}

pub fn set_window_to_center(window: &Window, monitor_handle: &MonitorHandle) {
    let window_size = window.outer_size();
    let monitor_topleft = monitor_handle.position();
    let monitor_size = monitor_handle.size();
    let pos = PhysicalPosition {
        x: monitor_topleft.x + ((monitor_size.width.saturating_sub(window_size.width)) as i32 / 2),
        y: monitor_topleft.y
            + ((monitor_size.height.saturating_sub(window_size.height)) as i32 / 2),
    };
    window.set_outer_position(pos);
}

/// Return "yes" if true, "no" otherwise
pub fn yes_no(yes: bool) -> &'static str {
    if yes {
        "yes"
    } else {
        "no"
    }
}

/// Get the config file path
pub fn get_config_file_path() -> Option<PathBuf> {
    // From args
    let mut args = std::env::args_os();
    if let Some(arg_conf_path) = args.nth(1).and_then(|s| PathBuf::from(s).into()) {
        if arg_conf_path.is_file() {
            if let Some(ext) = arg_conf_path.extension() {
                if ext == CONF_FILE_EXTENSION {
                    return Some(arg_conf_path);
                }
            }
        }
    }

    // From home dir ('~/.sldshow')
    if let Some(home_dir) = dirs::home_dir() {
        let user_conf_path = home_dir.with_file_name(".sldshow");
        if user_conf_path.is_file() {
            return Some(user_conf_path);
        }
    }

    None
}

pub fn path_copy_to_clipboard(path: &Path) -> bool {
    match ClipboardContext::new() {
        Ok(mut ctx) => {
            if let Some(spath) = path.to_str() {
                match ctx.set_contents(spath.to_owned()) {
                    Ok(_) => return true,
                    Err(err) => log::error!("{}", err),
                }
            }
        }
        Err(err) => log::error!("{}", err),
    }

    false
}

pub fn modulo<T>(a: T, b: T) -> T
where
    T: std::ops::Add<Output = T> + std::ops::Rem<Output = T> + Copy,
{
    ((a % b) + b) % b
}

pub fn distance(a: &PhysicalPosition<f64>, b: &PhysicalPosition<f64>) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}
