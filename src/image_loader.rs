use crate::utils::modulo;
use crate::SUPPORTED_IMAGE_FORMATS;
use anyhow::{anyhow, Result};
use rand::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use stopwatch::Stopwatch;
use winit::dpi::PhysicalSize;

const MAX_DEPTH_SCAN: usize = 999;

#[derive(Debug, Clone)]
pub struct ImageCache {
    pub path: Option<PathBuf>,
    pub image: image::RgbaImage,
    pub emsg: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Size2d<T> {
    pub width: T,
    pub height: T,
}

impl From<PhysicalSize<u32>> for Size2d<u32> {
    fn from(size: PhysicalSize<u32>) -> Self {
        Self {
            width: size.width,
            height: size.height,
        }
    }
}

impl From<Size2d<u32>> for PhysicalSize<u32> {
    fn from(size: Size2d<u32>) -> Self {
        Self {
            width: size.width,
            height: size.height,
        }
    }
}

pub struct ImageLoader {
    pub cache: HashMap<usize, ImageCache>,
    pub preload_queue: VecDeque<usize>,
    pub scanned_paths: Vec<PathBuf>,
    pub scan_subfolders: bool,
    pub current_path: Option<PathBuf>,
    pub current_index: usize,
    pub supported_extensions: Vec<OsString>,
    pub cache_extent: usize,
    pub max_cache_size: usize,
    pub texture_size: Size2d<u32>,
    pub resize_filter: image::imageops::FilterType,
}

impl ImageLoader {
    pub fn new(
        scan_subfolders: bool,
        texture_size: Size2d<u32>,
        resize_filter: image::imageops::FilterType,
        cache_extent: usize,
    ) -> Self {
        let supported_extensions: Vec<OsString> = SUPPORTED_IMAGE_FORMATS
            .iter()
            .flat_map(|v| v.extensions_str())
            .map(OsString::from)
            .collect();

        ImageLoader {
            cache: HashMap::new(),
            preload_queue: VecDeque::new(),
            scanned_paths: Vec::new(),
            scan_subfolders,
            current_path: None,
            current_index: 0,
            supported_extensions,
            cache_extent,
            max_cache_size: (cache_extent * 2) + 1,
            texture_size,
            resize_filter,
        }
    }

    pub fn append_path(&mut self, path: PathBuf) {
        let mut new_paths = {
            let mut out: Vec<PathBuf> = vec![];
            if path.is_dir() {
                self.scan_recursively(&mut out, &path, 0);
            } else if path.is_file() && self.is_supported_ext(&path) {
                out.push(path);
            }
            out
        };
        self.scanned_paths.append(&mut new_paths);
    }

    pub fn shuffle_paths(&mut self) {
        self.scanned_paths.shuffle(&mut rand::thread_rng());
    }

    pub fn limit_cache(&mut self) -> Result<()> {
        let mut cache_count = self.cache.len();
        while cache_count > self.max_cache_size {
            let max_dist = self
                .cache
                .keys()
                .cloned()
                .map(|k| self.index_distance(&self.current_index, &k))
                .max()
                .ok_or_else(|| anyhow!("cannot get a max distance in cache."))?;

            let max_dist_key = self
                .cache
                .keys()
                .cloned()
                .find(|k| self.index_distance(&self.current_index, k) == max_dist)
                .ok_or_else(|| anyhow!("cannot get a key"))?;

            self.cache.remove(&max_dist_key);
            //log::info!("remove_cache: key={}, dist={}", max_dist_key, max_dist);
            cache_count -= 1;
        }
        //log::info!("cache_count: {}", cache_count);

        Ok(())
    }

    fn index_distance(&self, a: &usize, b: &usize) -> usize {
        if a == b {
            return 0;
        }

        // ex. (len=10, high=7, low=1) => min(forwad=6, back=4)
        let len = self.scanned_paths.len();
        let (high, low) = if a > b { (a, b) } else { (b, a) };

        let foward = high - low;
        if foward <= (len / 2) {
            foward
        } else {
            len - high + low // back
        }
    }

    fn get_next_index(&self, amount: i32) -> Option<usize> {
        let len = self.scanned_paths.len() as i32;
        if len <= 1 {
            return None;
        }

        let mut index = self.current_index as i32 + amount;
        if index < 0 || index >= len {
            index = modulo(index, len);
        }

        Some(index as usize)
    }

    pub fn next_index(&mut self, amount: i32) {
        if let Some(index) = self.get_next_index(amount) {
            self.current_index = index;
        }
    }

    pub fn is_last(&self) -> bool {
        self.current_index == self.scanned_paths.len() - 1
    }

    fn ensure_cache(&mut self, index: &usize) -> Result<()> {
        if !self.cache.contains_key(index) {
            let mut emsg = None;
            let (image, path) = match &self.scanned_paths.get(*index) {
                Some(path) => {
                    match Self::open_and_resize_image(
                        index,
                        path,
                        &self.texture_size,
                        self.resize_filter,
                    ) {
                        Ok(image) => (image, Some((**path).clone())),
                        Err(err) => {
                            log::error!("{}", err);
                            emsg = Some(err.to_string());
                            (image::RgbaImage::new(1, 1), Some((**path).clone()))
                        }
                    }
                }
                None => (image::RgbaImage::new(1, 1), None),
            };

            self.cache.insert(*index, ImageCache { path, image, emsg });
        };

        Ok(())
    }

    pub fn force_reload_cache(&mut self, index: &usize) -> Result<()> {
        self.cache.remove(index);
        self.ensure_cache(index)
    }

    pub fn get_current(&mut self) -> Result<&ImageCache> {
        let index = self.current_index;
        self.ensure_cache(&index)?;

        let image_cache = self
            .cache
            .get(&index)
            .ok_or_else(|| anyhow!("faild to load an image cache."))?;

        // Update preload queue: i+1, i-1, i+2, i-2, ...,
        self.preload_queue.clear();
        for i in 1..=self.cache_extent {
            let amount = i as i32;

            if let Some(idx) = self.get_next_index(amount) {
                self.preload_queue.push_back(idx);
            }
            if let Some(idx) = self.get_next_index(-amount) {
                self.preload_queue.push_back(idx);
            }
        }

        Ok(image_cache)
    }

    fn is_supported_ext(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension() {
            let ext = ext.to_ascii_lowercase();
            let is_supported = self
                .supported_extensions
                .iter()
                .any(|fmt_ext| ext == *fmt_ext);
            return is_supported;
        }

        false
    }

    pub fn scan_input_paths(&mut self, paths: &[PathBuf]) {
        self.scanned_paths = {
            let mut out: Vec<PathBuf> = vec![];
            for path in paths {
                if path.is_dir() {
                    self.scan_recursively(&mut out, path, 0);
                } else if path.is_file() && self.is_supported_ext(path) {
                    out.push(path.clone());
                }
            }
            out
        };
    }

    pub fn scan_recursively(&self, out: &mut Vec<PathBuf>, dir: &Path, depth: usize) {
        if self.scan_subfolders {
            if depth > MAX_DEPTH_SCAN {
                return;
            }
        } else if depth > 0 {
            return;
        }

        if let Ok(dir) = fs::read_dir(dir) {
            let mut paths: Vec<_> = dir.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            alphanumeric_sort::sort_path_slice(&mut paths);

            for path in paths {
                if path.is_dir() {
                    self.scan_recursively(out, &path, depth + 1);
                } else if path.is_file() && self.is_supported_ext(&path) {
                    out.push(path);
                }
            }
        }
    }

    pub fn open_and_resize_image(
        index: &usize,
        path: &Path,
        size: &Size2d<u32>,
        filter_type: image::imageops::FilterType,
    ) -> Result<image::RgbaImage> {
        let mut sw = Stopwatch::new();

        let file = std::fs::File::open(path)?;

        sw.restart();
        let mut img = image::open(path)?;
        let time_image_open = sw.elapsed_ms();

        sw.restart();
        if let Some(orientation) = Self::get_exif_orientation(&file) {
            img = match orientation {
                1 => img,
                2 => img.fliph(),
                3 => img.rotate180(),
                4 => img.flipv(),
                5 => img.flipv().rotate90(),
                6 => img.rotate90(),
                7 => img.flipv().rotate270(),
                8 => img.rotate270(),
                _ => img,
            }
        }
        let time_exif_orientation = sw.elapsed_ms();

        sw.restart();
        let img = img.resize(size.width, size.height, filter_type).to_rgba8();
        let time_resize = sw.elapsed_ms();

        log::info!(
            "image[{}] open: {} ms, exif: {} ms, resize: {} ms",
            index,
            time_image_open,
            time_exif_orientation,
            time_resize
        );

        Ok(img)
    }

    /// Get the Exif Orientation value
    fn get_exif_orientation(file: &fs::File) -> Option<u16> {
        let mut bufreader = std::io::BufReader::new(file);
        let exifreader = exif::Reader::new();
        if let Ok(exif) = exifreader.read_from_container(&mut bufreader) {
            if let Some(orient) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
                if let exif::Value::Short(v) = &orient.value {
                    if let Some(v) = v.first() {
                        return Some(*v);
                    }
                }
            }
        }

        None
    }
}
