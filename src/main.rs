#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Hide console window at Windows

mod config;
mod image_loader;
mod logger;
mod state;
mod texture;
mod utils;

#[cfg(windows)]
mod common_win32;

use crate::image_loader::{ImageCache, ImageLoader, Size2d};
use crate::logger::ResultLogging;
use crate::state::{FullscreenController, State};
use crate::utils::*;
use anyhow::Result;
use futures::executor::block_on;
use image::ImageFormat;
use std::io::Cursor;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{Event, KeyboardInput, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const APP_NAME: &str = "sldshow";

const CURSOR_SLEEP_START_TIME: u64 = 3;
const OSD_MESSAGE_DISPLAY_TIME: u64 = 3;
const FILE_DROP_TIMEOUT: f32 = 0.5;
const TIMER_VALUE_INCREMENT: u32 = 5;
const FULLSCREEN_CHANGE_INTERVAL: Duration = Duration::from_millis(300);
const MULTITOUCH_INTERVAL: Duration = Duration::from_millis(50);
const TOUCH_DRAG_START_DISTANCE: f64 = 5.0;

pub const SUPPORTED_IMAGE_FORMATS: [ImageFormat; 12] = [
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Gif,
    ImageFormat::WebP,
    ImageFormat::Pnm,
    ImageFormat::Tiff,
    ImageFormat::Tga,
    ImageFormat::Dds,
    ImageFormat::Bmp,
    ImageFormat::Ico,
    ImageFormat::Hdr,
    ImageFormat::Farbfeld,
];

/// Winit custom events
#[derive(Debug)]
pub enum CustomEvent {
    NextImage,
    TransitionStart,
    TransitionUpdate,
    MouseCursorSleep,
    MouseCursorAwake,
    ClearOsdMessage,
}

#[derive(Debug)]
pub enum TimerState {
    Play,
    Pause,
    Change(u32),
}

#[derive(Debug, PartialEq)]
enum DragState {
    None,
    Awake,
    Dragging,
}

#[derive(Debug)]
enum Nav {
    None,
    Next,
    Prev,
    Next10,
    Prev10,
    First,
    Last,
}

fn main() -> Result<()> {
    if let Err(err) = logger::init_logger() {
        eprintln!("logger init failed: {}", err);
    }

    let conf_path = get_config_file_path();
    let conf = conf_path
        .as_ref()
        .and_then(|p| config::get_config(p).ok())
        .unwrap_or_default();

    log::info!("{:#?}", conf);

    // Change the current working directory to the location of the config file
    // to support loading relative image paths
    if let Some(conf_dir) = conf_path.as_ref().and_then(|p| p.parent()) {
        std::env::set_current_dir(conf_dir).log_info();
    }

    let window_size = Size2d {
        width: conf.window.width,
        height: conf.window.height,
        scale_factor: None,
    };
    let resize_filter = convert_filter_type(&conf.viewer.resize_filter);

    // Stop screensaver
    if conf.viewer.stop_screensaver {
        #[cfg(windows)]
        common_win32::stop_screensaver();

        #[cfg(not(windows))]
        log::warn!("'stop_screensaver' option not supported ");
    }

    // Create taskbar icon
    let icon = {
        let image =
            image::io::Reader::new(Cursor::new(include_bytes!("../assets/icon/icon_32.png")))
                .with_guessed_format()?
                .decode()?
                .to_rgba8();
        let (width, height) = image.dimensions();
        winit::window::Icon::from_rgba(image.into_vec(), width, height).ok()
    };

    let window_title = match &conf_path {
        Some(path) => format!("{} - {}", path.display(), APP_NAME),
        None => APP_NAME.to_owned(),
    };

    // Create main window
    let event_loop: EventLoop<CustomEvent> = EventLoop::with_user_event();
    let builder = WindowBuilder::new()
        .with_title(window_title)
        .with_window_icon(icon)
        .with_inner_size(PhysicalSize::from(window_size))
        .with_always_on_top(conf.window.always_on_top)
        .with_transparent(conf.style.bg_color[3] < 255)
        .with_resizable(conf.window.resizable)
        .with_decorations(conf.window.titlebar);
    let main_window = Rc::new(builder.build(&event_loop)?);
    let inner_size = Size2d::from(main_window.inner_size());
    let mut texture_size = inner_size;
    texture_size.scale_factor = main_window.scale_factor().into();

    // Set main window position
    if let Some(target_monitor) = main_window
        .available_monitors()
        .nth(conf.window.monitor_index)
    {
        set_window_to_center(&main_window, &target_monitor);
    } else if let Some(primary_monitor) = main_window.primary_monitor() {
        set_window_to_center(&main_window, &primary_monitor);
    }

    let mut fullscreen_controller = FullscreenController {
        active: false,
        size: None,
        last_time: Instant::now(),
        rate_limit: FULLSCREEN_CHANGE_INTERVAL,
        window: main_window.clone(),
    };
    if conf.window.fullscreen {
        fullscreen_controller.enable();
    }

    // Create ImageLoader
    let image_loader = Arc::new(Mutex::new(ImageLoader::new(
        conf.viewer.scan_subfolders,
        texture_size,
        resize_filter,
        conf.viewer.cache_extent,
    )));

    // Scan image paths
    {
        let input_paths: Vec<_> = conf.viewer.image_paths.iter().map(PathBuf::from).collect();
        let mut loader = image_loader.lock().unwrap();
        loader.scan_input_paths(&input_paths);
        if conf.viewer.shuffle {
            loader.shuffle_paths();
        }
    }

    // Create channels for message passing
    let (tx_slideshow_timer, rx_slideshow_timer) = mpsc::channel::<TimerState>();
    let (tx_osd_message_timer, rx_osd_message_timer) = mpsc::channel::<()>();
    let (tx_mouse_cursor_watcher, rx_mouse_cursor_watcher) = mpsc::channel::<()>();
    let (tx_transition_throttle, rx_transition_throttle) = mpsc::channel::<Instant>();

    // Create main application state
    let mut state = block_on(State::new(
        &main_window,
        image_loader.clone(),
        conf.clone(),
        fullscreen_controller,
        tx_slideshow_timer,
        tx_osd_message_timer,
        event_loop.create_proxy(),
    ))?;

    // Window states
    let mut always_on_top = conf.window.always_on_top;
    let mut titlebar = conf.window.titlebar;

    // Input states
    let double_click_duration = get_double_click_duration();
    let mut last_mouse_left_pressed_time = Instant::now();
    let mut last_touch_pressed_time = Instant::now();
    let mut touch_finger_count = 0;
    let mut last_touch_finger_count = touch_finger_count;
    let mut last_touch_finger_id = 0;
    let mut multifinger_touch = false;
    let mut drag_finger = false;
    let mut drag_state = DragState::None;
    let mut drag_pos: Option<PhysicalPosition<f64>> = None;
    let mut last_file_drop_event_time = Instant::now();
    let mut modifiers_state = winit::event::ModifiersState::default();

    //---------
    // Threads
    //---------

    // Slideshow timer
    let timer = conf.viewer.timer;
    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        let mut dur = Duration::from_secs(timer as u64);
        let mut paused = timer == 0;

        loop {
            let recv = rx_slideshow_timer.recv_timeout(dur);
            match recv {
                Ok(state) => match state {
                    TimerState::Change(secs) => dur = Duration::from_secs(secs as u64),
                    TimerState::Pause => paused = true,
                    _ => {}
                },
                // Wait completed
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    proxy.send_event(CustomEvent::NextImage).log_err()
                }
                _ => (),
            };

            while paused | dur.is_zero() {
                let recv = rx_slideshow_timer.recv();
                if let Ok(state) = recv {
                    match state {
                        TimerState::Change(secs) => dur = Duration::from_secs(secs as u64),
                        TimerState::Play => paused = false,
                        _ => (),
                    }
                }
            }
        }
    });

    // OSD display timer
    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        const DURATION: Duration = Duration::from_secs(OSD_MESSAGE_DISPLAY_TIME);

        loop {
            if let Err(mpsc::RecvTimeoutError::Timeout) =
                rx_osd_message_timer.recv_timeout(DURATION)
            {
                // Wait completed
                proxy.send_event(CustomEvent::ClearOsdMessage).log_err()
            }
        }
    });

    // Mouse cursor autohide timer
    if conf.window.cursor_auto_hide {
        let proxy = event_loop.create_proxy();
        std::thread::spawn(move || {
            let dur = Duration::from_secs(CURSOR_SLEEP_START_TIME);
            let mut sleeping = false;

            loop {
                match rx_mouse_cursor_watcher.recv_timeout(dur) {
                    // Awake
                    Ok(_) if sleeping => {
                        proxy.send_event(CustomEvent::MouseCursorAwake).log_err();
                        sleeping = false;
                    }
                    // Sleep
                    Err(mpsc::RecvTimeoutError::Timeout) if !sleeping => {
                        proxy.send_event(CustomEvent::MouseCursorSleep).log_err();
                        sleeping = true;
                    }
                    _ => (),
                };
            }
        });
    } else {
        std::thread::spawn(move || loop {
            let _ = rx_mouse_cursor_watcher.recv();
        });
    }

    // Fps throttling for the transition effect
    let proxy = event_loop.create_proxy();
    let fps = conf.transition.fps;
    std::thread::spawn(move || {
        let msec_per_frame: u64 = ((1.0 / fps) * 1000.0) as u64;
        let dur = Duration::from_millis(msec_per_frame);

        loop {
            let res = rx_transition_throttle.recv();
            match &res {
                Ok(t) => std::thread::sleep(dur.saturating_sub(t.elapsed())),
                Err(_) => std::thread::sleep(dur),
            }

            if res.is_ok() {
                proxy.send_event(CustomEvent::TransitionUpdate).log_err();
            }
        }
    });

    // Image loader thread
    std::thread::spawn(move || {
        let dur = Duration::from_millis(100);
        let texture_size = &texture_size.clone();
        let mut idx: usize;
        let mut load_needed: bool;
        let mut prev_load_needed: bool = false;
        let mut path: Option<PathBuf>;

        loop {
            // dequeue
            {
                let mut loader = image_loader.lock().unwrap();
                match loader.preload_queue.pop_front() {
                    Some(index) => {
                        idx = index;
                        load_needed = !loader.cache.contains_key(&index);
                        path = Some(loader.scanned_paths.get(index).unwrap().to_path_buf());
                    }
                    None => {
                        idx = 0;
                        load_needed = false;
                        path = None
                    }
                }
            }

            // load image
            if load_needed {
                let mut emsg = None;
                let image = match &path {
                    Some(path) => {
                        match ImageLoader::open_and_resize_image(
                            &idx,
                            path,
                            texture_size,
                            resize_filter,
                        ) {
                            Ok(image) => image,
                            Err(err) => {
                                log::error!("{}", err);
                                emsg = Some(err.to_string());
                                image::RgbaImage::new(1, 1)
                            }
                        }
                    }
                    None => image::RgbaImage::new(1, 1),
                };

                {
                    let mut loader = image_loader.lock().unwrap();
                    loader.cache.insert(idx, ImageCache { path, image, emsg });
                }
            }

            // limit queue size
            if prev_load_needed && !load_needed {
                if let Ok(mut loader) = image_loader.lock() {
                    loader.limit_cache().log_err();
                }
            }

            prev_load_needed = load_needed;

            if !load_needed {
                std::thread::sleep(dur);
            }
        }
    });

    //-----------
    // Main Loop
    //-----------

    event_loop.run(move |event, _, control_flow| {
        use winit::event::{
            ElementState::{Pressed, Released},
            MouseButton, TouchPhase,
        };

        *control_flow = ControlFlow::Wait;

        match &event {
            Event::UserEvent(event) => match event {
                CustomEvent::NextImage => {
                    if state.pause_at_last && state.image_loader.lock().unwrap().is_last() {
                        state.paused = true;
                        return;
                    }

                    state.next_image(1).log_err();
                }
                CustomEvent::TransitionStart => {
                    state.transition.active = true;
                    state.transition.last_time = Instant::now();
                    state
                        .event_proxy
                        .send_event(CustomEvent::TransitionUpdate)
                        .log_err();
                }
                CustomEvent::TransitionUpdate => {
                    let is_end = state.update_transition();
                    if is_end {
                        state.graphics.redraw_image();
                    } else {
                        tx_transition_throttle.send(Instant::now()).log_err();
                    };
                }
                CustomEvent::MouseCursorAwake => main_window.set_cursor_visible(true),
                CustomEvent::MouseCursorSleep => main_window.set_cursor_visible(false),
                CustomEvent::ClearOsdMessage => state.graphics.update_message(""),
            },
            Event::WindowEvent { event, window_id } if window_id == &main_window.id() => {
                use winit::event::{
                    MouseScrollDelta,
                    VirtualKeyCode::{
                        Back, Comma, Down, End, Escape, Home, Key0, Key1, Key2, LBracket, Left,
                        PageDown, PageUp, Pause, Period, RBracket, Return, Right, Space, Up, C, D,
                        F, F11, L, M, O, P, Q, T,
                    },
                };

                let mut gfx = &mut state.graphics;
                let mut nav = Nav::None;

                match event {
                    WindowEvent::ModifiersChanged(newstate) => {
                        modifiers_state = *newstate;
                    }
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(virtual_code),
                                state: press_state,
                                ..
                            },
                        ..
                    } => {
                        match press_state {
                            Pressed => match virtual_code {
                                LBracket => {
                                    // Decrease the display time
                                    state.current_timer_secs = state
                                        .current_timer_secs
                                        .saturating_sub(TIMER_VALUE_INCREMENT);
                                    state
                                        .tx_slideshow_timer
                                        .send(TimerState::Change(state.current_timer_secs))
                                        .log_err();
                                    if state.paused {
                                        state.tx_slideshow_timer.send(TimerState::Pause).log_err();
                                    }
                                    gfx.update_message(&format!(
                                        "Timer: {}",
                                        state.current_timer_secs
                                    ));
                                }
                                RBracket => {
                                    // Increase the display time
                                    state.current_timer_secs = state
                                        .current_timer_secs
                                        .saturating_add(TIMER_VALUE_INCREMENT);
                                    state
                                        .tx_slideshow_timer
                                        .send(TimerState::Change(state.current_timer_secs))
                                        .log_err();
                                    if state.paused {
                                        state.tx_slideshow_timer.send(TimerState::Pause).log_err();
                                    }
                                    gfx.update_message(&format!(
                                        "Timer: {}",
                                        state.current_timer_secs
                                    ));
                                }
                                _ => {}
                            },
                            Released => match virtual_code {
                                Q | Escape => *control_flow = ControlFlow::Exit,
                                Key0 if modifiers_state.alt() => {
                                    main_window.set_inner_size(PhysicalSize::new(
                                        gfx.texture_size.width / 2,
                                        gfx.texture_size.height / 2,
                                    ));
                                    gfx.update_message("Window Scale: 0.5");
                                }
                                Key1 if modifiers_state.alt() => {
                                    main_window.set_inner_size(gfx.texture_size);
                                    gfx.update_message("Window Scale: 1.0");
                                }
                                Key2 if modifiers_state.alt() => {
                                    main_window.set_inner_size(PhysicalSize::new(
                                        gfx.texture_size.width * 2,
                                        gfx.texture_size.height * 2,
                                    ));
                                    gfx.update_message("Window Scale: 2.0");
                                }
                                M | Down if modifiers_state.alt() => {
                                    main_window.set_minimized(true)
                                }
                                F | F11 => {
                                    state.fullscreen_ctrl.toggle();
                                    state.draw_current_image().log_err();
                                }
                                Return if modifiers_state.alt() => {
                                    state.fullscreen_ctrl.toggle();
                                    state.draw_current_image().log_err();
                                }
                                T => {
                                    always_on_top = !always_on_top;
                                    main_window.set_always_on_top(always_on_top);
                                    gfx.update_message(&format!(
                                        "Always on top: {}",
                                        yes_no(always_on_top)
                                    ));
                                }
                                D => {
                                    titlebar = !titlebar;
                                    let inner_size = main_window.inner_size();
                                    main_window.set_decorations(titlebar);
                                    main_window.set_inner_size(inner_size);
                                    state
                                        .graphics
                                        .update_message(&format!("Titlebar: {}", yes_no(titlebar)));
                                }
                                Right | Down | PageDown | Period | Return => {
                                    nav = if modifiers_state.shift() {
                                        Nav::Next10
                                    } else {
                                        Nav::Next
                                    };
                                }
                                Left | Up | PageUp | Comma => {
                                    nav = if modifiers_state.shift() {
                                        Nav::Prev10
                                    } else {
                                        Nav::Prev
                                    };
                                }
                                Home => nav = Nav::First,
                                End => nav = Nav::Last,
                                Space | P => {
                                    // Toggle Pause
                                    if state.paused {
                                        state.tx_slideshow_timer.send(TimerState::Play).log_err();
                                        gfx.update_message("Play");
                                    } else {
                                        state.tx_slideshow_timer.send(TimerState::Pause).log_err();
                                        gfx.update_message("Pause");
                                    }
                                    state.paused = !state.paused;
                                }
                                Pause => {
                                    // Pause
                                    state.paused = true;
                                    state.tx_slideshow_timer.send(TimerState::Pause).log_err();
                                    gfx.update_message("Pause");
                                }
                                L => {
                                    state.pause_at_last = !state.pause_at_last;
                                    gfx.update_message(&format!(
                                        "Pause at last: {}",
                                        yes_no(state.pause_at_last)
                                    ));
                                }
                                O => {
                                    let (index, count) = {
                                        let loader = state.image_loader.lock().unwrap();
                                        (loader.current_index, loader.scanned_paths.len())
                                    };
                                    state.graphics.update_message(&format!(
                                        "Pos: {}/{}",
                                        index + 1,
                                        count
                                    ));
                                }
                                Back => {
                                    // Reset the display time to the default value
                                    state.current_timer_secs = state.default_timer_secs;
                                    state
                                        .tx_slideshow_timer
                                        .send(TimerState::Change(state.current_timer_secs))
                                        .log_err();
                                    if state.paused {
                                        state.tx_slideshow_timer.send(TimerState::Pause).log_err();
                                    }
                                    gfx.update_message(&format!(
                                        "Timer: {} (reset)",
                                        state.current_timer_secs
                                    ));
                                }
                                C if modifiers_state.ctrl() => {
                                    let loader = state.image_loader.lock().unwrap();
                                    if let Some(path) = &loader.current_path {
                                        if path_copy_to_clipboard(path) {
                                            gfx.update_message(&format!(
                                                "File path copied\n'{}'",
                                                path.display()
                                            ));
                                        }
                                    }
                                }
                                _ => {}
                            },
                        }
                    }
                    WindowEvent::MouseInput {
                        state: clickstate,
                        button,
                        ..
                    } => match button {
                        MouseButton::Left => match clickstate {
                            Pressed => {
                                if drag_state == DragState::None {
                                    drag_state = DragState::Awake;
                                }

                                if last_mouse_left_pressed_time.elapsed() <= double_click_duration {
                                    state.fullscreen_ctrl.toggle();
                                    state.draw_current_image().log_err();
                                }

                                last_mouse_left_pressed_time = Instant::now();
                            }
                            Released => {
                                if drag_state != DragState::Dragging {
                                    nav = if modifiers_state.shift() {
                                        Nav::Next10
                                    } else {
                                        Nav::Next
                                    };
                                }

                                drag_state = DragState::None;
                                drag_pos = None;
                            }
                        },
                        MouseButton::Right if clickstate == &Released => {
                            nav = if modifiers_state.shift() {
                                Nav::Prev10
                            } else {
                                Nav::Prev
                            }
                        }
                        MouseButton::Middle if clickstate == &Released => {
                            *control_flow = ControlFlow::Exit
                        }
                        _ => {}
                    },
                    WindowEvent::MouseWheel { delta, .. } => {
                        let up = match delta {
                            MouseScrollDelta::LineDelta(_, y) => *y > 0.0,
                            MouseScrollDelta::PixelDelta(v) => v.y > 0.0,
                        };

                        nav = if up {
                            if modifiers_state.shift() {
                                Nav::Prev10
                            } else {
                                Nav::Prev
                            }
                        } else if modifiers_state.shift() {
                            Nav::Next10
                        } else {
                            Nav::Next
                        };
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        tx_mouse_cursor_watcher.send(()).unwrap();

                        match drag_state {
                            DragState::Awake => {
                                drag_state = DragState::Dragging;
                                drag_pos = Some(*position);
                                drag_finger = false;
                            }
                            DragState::Dragging if !drag_finger => {
                                if main_window.fullscreen().is_some() {
                                    state.fullscreen_ctrl.toggle();
                                    state.draw_current_image().log_err();

                                    let s = main_window.inner_size();
                                    drag_pos = Some(PhysicalPosition {
                                        x: (s.width / 2) as f64,
                                        y: (s.height / 2) as f64,
                                    });
                                }

                                if let Ok(mut window_pos) = main_window.outer_position() {
                                    if let Some(drag_pos) = drag_pos {
                                        window_pos.x += (position.x - drag_pos.x) as i32;
                                        window_pos.y += (position.y - drag_pos.y) as i32;
                                        main_window.set_outer_position(window_pos);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    WindowEvent::Touch(touch) => {
                        if TouchPhase::Started == touch.phase {
                            touch_finger_count += 1;
                        }

                        let new_fingers = touch_finger_count - last_touch_finger_count;

                        match touch.phase {
                            TouchPhase::Started if new_fingers == 1 => {
                                // Multi-finger tapping
                                if last_touch_pressed_time.elapsed() <= MULTITOUCH_INTERVAL {
                                    state.fullscreen_ctrl.toggle();
                                    state.draw_current_image().log_err();
                                    multifinger_touch = true;
                                } else {
                                    multifinger_touch = false;
                                }
                                last_touch_pressed_time = Instant::now();
                            }
                            TouchPhase::Moved if new_fingers == 0 => match drag_state {
                                DragState::None => {
                                    drag_state = DragState::Awake;
                                    drag_pos = Some(touch.location);
                                }
                                DragState::Awake if !state.fullscreen_ctrl.active => {
                                    if let Some(drag_pos_in) = drag_pos {
                                        let delta = distance(&touch.location, &drag_pos_in);
                                        if delta > TOUCH_DRAG_START_DISTANCE {
                                            drag_state = DragState::Dragging;
                                            drag_pos = Some(touch.location);
                                            drag_finger = true;
                                        } else {
                                            drag_state = DragState::None;
                                            drag_pos = None;
                                        }
                                    }
                                }
                                DragState::Dragging
                                    if touch.id == last_touch_finger_id
                                        && !state.fullscreen_ctrl.active =>
                                {
                                    if let Ok(mut window_pos) = main_window.outer_position() {
                                        if let Some(drag_pos) = drag_pos {
                                            window_pos.x += (touch.location.x - drag_pos.x) as i32;
                                            window_pos.y += (touch.location.y - drag_pos.y) as i32;
                                            main_window.set_outer_position(window_pos);
                                        }
                                    }
                                }
                                _ => {}
                            },
                            TouchPhase::Ended | TouchPhase::Cancelled => {
                                touch_finger_count -= 1; // Sometimes not called and may cause leaks

                                if drag_state != DragState::Dragging && !multifinger_touch {
                                    let size = main_window.inner_size();
                                    let loc = touch.location;
                                    let touch_right = loc.x >= (size.width / 2) as f64;
                                    nav = if touch_right {
                                        if modifiers_state.shift() {
                                            Nav::Next10
                                        } else {
                                            Nav::Next
                                        }
                                    } else if modifiers_state.shift() {
                                        Nav::Prev10
                                    } else {
                                        Nav::Prev
                                    }
                                }

                                drag_state = DragState::None;
                                drag_pos = None;
                            }
                            _ => {}
                        }

                        last_touch_finger_id = touch.id;
                        last_touch_finger_count = touch_finger_count;
                    }
                    WindowEvent::DroppedFile(path) => {
                        let mut new = false;

                        if let Ok(loader) = &mut state.image_loader.lock() {
                            if last_file_drop_event_time.elapsed().as_secs_f32() > FILE_DROP_TIMEOUT
                            {
                                loader.scanned_paths.clear();
                                new = true;
                            } else if loader.scanned_paths.is_empty() {
                                new = true;
                            }

                            loader.append_path(path.clone());

                            if new {
                                loader.current_index = 0;
                                loader.cache.clear();
                                loader.force_reload_cache(&0).log_err();
                            }
                        }

                        if new {
                            state.draw_current_image().log_err();
                        }

                        last_file_drop_event_time = Instant::now();
                    }
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(physical_size) => {
                        gfx.resize(*physical_size);
                        state.draw_current_image().log_err();
                    }
                    WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        new_inner_size,
                    } => {
                        gfx.dpi_scale_factor = *scale_factor;
                        gfx.resize(**new_inner_size);
                        state.draw_current_image().log_err();
                    }
                    _ => {}
                };

                match nav {
                    Nav::Next => state.next_image(1).log_err(),
                    Nav::Prev => state.next_image(-1).log_err(),
                    Nav::Next10 => state.next_image(10).log_err(),
                    Nav::Prev10 => state.next_image(-10).log_err(),
                    Nav::First => state.first_image().log_err(),
                    Nav::Last => state.last_image().log_err(),
                    _ => {}
                };
            }
            Event::MainEventsCleared => main_window.request_redraw(),
            Event::RedrawRequested(_) => {
                let current_path = {
                    let loader = state.image_loader.lock().unwrap();
                    loader.current_path.clone()
                };

                use wgpu::SwapChainError::{Lost, OutOfMemory, Outdated};
                match state.graphics.render(&current_path) {
                    Ok(_) => {}
                    Err(Lost | Outdated) => state.graphics.resize(state.graphics.inner_size),
                    Err(OutOfMemory) => *control_flow = ControlFlow::Exit,
                    Err(e) => log::error!("{:}", e),
                }
            }
            _ => (),
        }
    });
}
