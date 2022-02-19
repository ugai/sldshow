use crate::config;
use crate::image_loader::ImageLoader;
use crate::logger::ResultLogging;
use crate::texture;
use crate::utils::*;
use crate::CustomEvent;
use crate::TimerState;
use anyhow::{anyhow, Result};
use font_kit::{
    family_name::FamilyName, handle::Handle, properties::Properties, source::SystemSource,
};
use futures::task::SpawnExt;
use image::Pixel;
use rand::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use wgpu::util::DeviceExt;
use wgpu_glyph::{
    ab_glyph, GlyphBrushBuilder, HorizontalAlign, Layout, Section, Text, VerticalAlign,
};
use winit::window::Fullscreen;
use winit::{dpi::PhysicalSize, event_loop::EventLoopProxy, window::Window};

const TRANSITION_MAX_MODE_IDX: i32 = 21; // See the transition shader file
const FONT_SIZE_DROP_HERE_TEXT: f32 = 20.0;

type IsTransitionEnd = bool;

#[derive(Debug)]
pub struct FullscreenController {
    pub active: bool,
    pub size: Option<PhysicalSize<u32>>,
    pub last_time: Instant,
    pub rate_limit: Duration,
    pub window: Rc<Window>,
}

impl FullscreenController {
    pub fn toggle(&mut self) {
        match self.window.fullscreen() {
            None => self.enable(),
            Some(_) => self.disable(),
        }
    }

    pub fn enable(&mut self) {
        if self.limit_reached() {
            return;
        }
        const FULLSCREEN_TYPE: Option<Fullscreen> = Some(Fullscreen::Borderless(None));
        self.window.set_fullscreen(FULLSCREEN_TYPE);
        self.active = true;
        self.size = self.window.current_monitor().and_then(|f| f.size().into());
        self.last_time = Instant::now();
    }

    pub fn disable(&mut self) {
        if self.limit_reached() {
            return;
        }
        self.window.set_fullscreen(None);
        self.active = false;
        self.size = None;
        self.last_time = Instant::now();
    }

    fn limit_reached(&self) -> bool {
        self.last_time.elapsed() <= self.rate_limit
    }
}

#[rustfmt::skip]
const QUAD_VERTICES: &[Vertex] = &[
    Vertex { position: [-1.0, 1.0, 0.0], tex_coords: [0.0, 0.0] },
    Vertex { position: [-1.0, -1.0, 0.0], tex_coords: [0.0, 1.0] },
    Vertex { position: [1.0, 1.0, 0.0], tex_coords: [1.0, 0.0] },
    Vertex { position: [1.0, -1.0, 0.0], tex_coords: [1.0, 1.0] },
];
const QUAD_INDICES: &[u16] = &[0, 1, 2, 2, 1, 3];

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub blend: f32,
    pub flip: f32,
    pub mode: i32,
    pub resized_window_scale: [f32; 2],
    pub bg: [f32; 4],
}

impl Uniforms {
    fn new() -> Self {
        Self {
            blend: 1.0,
            flip: 0.0,
            mode: 0,
            resized_window_scale: [1.0, 1.0],
            bg: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

pub struct TransitionState {
    pub active: bool,
    pub direction: f32,
    pub last_time: Instant,
    pub time: f32,
    pub random: bool,
}

pub struct GraphicsState {
    pub surface: wgpu::Surface,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub sc_desc: wgpu::SwapChainDescriptor,
    pub swap_chain: wgpu::SwapChain,
    pub inner_size: winit::dpi::PhysicalSize<u32>,
    pub texture_size: winit::dpi::PhysicalSize<u32>,
    pub render_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub diffuse_image_temp: image::RgbaImage,
    pub diffuse_textures: [texture::Texture; 2],
    pub diffuse_bind_group: wgpu::BindGroup,
    pub uniforms: Uniforms,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub bg_color: image::Rgba<u8>,
    pub text_color: [f32; 4],
    pub show_image_path: bool,
    pub font_size_osd: f32,
    pub font_size_image_path: f32,
    pub glyph_brush: wgpu_glyph::GlyphBrush<()>,
    pub main_texture_index: usize,
    pub dpi_scale_factor: f64,
    pub message: Option<String>,
    pub tx_osd_message_timer: mpsc::Sender<()>,
    minimized: bool,
}

impl GraphicsState {
    pub async fn new(
        window: &Window,
        conf: &config::Config,
        tx_osd_message_timer: mpsc::Sender<()>,
    ) -> Result<Self> {
        let inner_size = window.inner_size();
        let dpi_scale_factor = window.scale_factor();

        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);

        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| anyhow!("failed to retrieve a device (wgpu::Adapter)."))?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None, // Trace path
            )
            .await?;

        let render_format = adapter
            .get_swap_chain_preferred_format(&surface)
            .ok_or_else(|| anyhow!("failed to get a texture format."))?;

        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: render_format,
            width: inner_size.width,
            height: inner_size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        let bg_color: image::Rgba<u8> = image::Rgba(conf.style.bg_color);

        let font = Self::load_font(conf.style.font_name.as_deref())?;
        let glyph_brush = GlyphBrushBuilder::using_font(font).build(&device, render_format);

        let diffuse_image_temp =
            image::ImageBuffer::from_pixel(inner_size.width, inner_size.height, bg_color);

        let diffuse_textures = [
            texture::Texture::from_image(&device, &queue, &diffuse_image_temp, Some("Texture A"))?,
            texture::Texture::from_image(&device, &queue, &diffuse_image_temp, Some("Texture B"))?,
        ];

        let mut uniforms = Uniforms::new();
        uniforms.blend = 1.0;
        for (i, v) in bg_color.channels().iter().enumerate() {
            uniforms.bg[i] = (*v as f32 / 255.0).clamp(0.0, 1.0);
        }
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                            filtering: true,
                        },
                        count: None,
                    },
                ],
                label: Some("Texture Bind Group Layout"),
            });

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_textures[0].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&diffuse_textures[1].view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&diffuse_textures[0].sampler),
                },
            ],
            label: Some("Diffuse Bind Group"),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("Uniform Bind Group Layout"),
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("Uniform Bind Group"),
        });

        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            flags: wgpu::ShaderFlags::all(),
            source: wgpu::ShaderSource::Wgsl(include_str!("transition.wgsl").into()),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: sc_desc.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrite::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                clamp_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsage::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsage::INDEX,
        });
        let num_indices = QUAD_INDICES.len() as u32;

        Ok(GraphicsState {
            surface,
            device,
            queue,
            sc_desc,
            swap_chain,
            inner_size,
            texture_size: inner_size,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            diffuse_image_temp,
            diffuse_textures,
            diffuse_bind_group,
            uniforms,
            uniform_buffer,
            uniform_bind_group,
            bg_color,
            show_image_path: conf.style.show_image_path,
            font_size_osd: conf.style.font_size_osd,
            font_size_image_path: conf.style.font_size_image_path,
            text_color: rgba_u8_to_f32(conf.style.text_color),
            glyph_brush,
            main_texture_index: 0,
            dpi_scale_factor,
            message: None,
            tx_osd_message_timer,
            minimized: false,
        })
    }

    pub fn render(&mut self, path: &Option<PathBuf>) -> Result<(), wgpu::SwapChainError> {
        if self.minimized {
            return Ok(());
        }

        let frame = self.swap_chain.get_current_frame()?.output;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::default(),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        {
            let mut staging_belt = wgpu::util::StagingBelt::new(1024);
            let mut local_pool = futures::executor::LocalPool::new();
            let local_spawner = local_pool.spawner();

            {
                let scale_factor = self.dpi_scale_factor as f32;
                if let Some(path) = path.as_ref().and_then(|p| p.to_str()) {
                    if self.show_image_path {
                        // Image file path
                        //   position: top-left
                        self.glyph_brush.queue(Section {
                            screen_position: (4.0, 2.0),
                            bounds: (self.inner_size.width as f32, self.inner_size.height as f32),
                            text: vec![Text::new(path)
                                .with_color(self.text_color)
                                .with_scale(self.font_size_image_path * scale_factor)],
                            ..Section::default()
                        });
                    }
                } else {
                    // Drop here message
                    //   position: center
                    self.glyph_brush.queue(Section {
                        screen_position: (
                            self.inner_size.width as f32 / 2.0,
                            self.inner_size.height as f32 / 2.0,
                        ),
                        bounds: (self.inner_size.width as f32, self.inner_size.height as f32),
                        text: vec![Text::new("drop image files here.")
                            .with_color(self.text_color)
                            .with_scale(FONT_SIZE_DROP_HERE_TEXT * scale_factor)],
                        layout: Layout::default()
                            .h_align(HorizontalAlign::Center)
                            .v_align(VerticalAlign::Center),
                    });
                }

                // Latest message
                //   position: top-right
                if let Some(message) = &self.message {
                    let offset = (self.font_size_osd / 2.0) * scale_factor;
                    self.glyph_brush.queue(Section {
                        screen_position: (self.inner_size.width as f32 - offset, offset),
                        bounds: (self.inner_size.width as f32, self.inner_size.height as f32),
                        text: vec![Text::new(message)
                            .with_color(self.text_color)
                            .with_scale(self.font_size_osd * scale_factor)],
                        layout: Layout::default()
                            .h_align(HorizontalAlign::Right)
                            .v_align(VerticalAlign::Top),
                    })
                }
            }

            self.glyph_brush
                .draw_queued(
                    &self.device,
                    &mut staging_belt,
                    &mut encoder,
                    &frame.view,
                    self.inner_size.width,
                    self.inner_size.height,
                )
                .expect("Draw queued");
            staging_belt.finish();

            self.queue.submit(std::iter::once(encoder.finish()));

            // Recall unused staging buffers
            local_spawner
                .spawn(staging_belt.recall())
                .expect("Recall staging belt");
            local_pool.run_until_stalled();
        }

        Ok(())
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        // Window minimized
        if new_size.width == 0 || new_size.height == 0 {
            self.minimized = true;
            return;
        } else if self.minimized {
            self.minimized = false;
        }

        self.inner_size = new_size;
        self.sc_desc.width = new_size.width;
        self.sc_desc.height = new_size.height;
        self.swap_chain = self.device.create_swap_chain(&self.surface, &self.sc_desc);
    }

    pub fn redraw_image(&mut self) {
        self.diffuse_textures[self.main_texture_index]
            .write_queue(&self.queue, &self.diffuse_image_temp);
    }

    fn load_font(font_name: Option<&str>) -> Result<ab_glyph::FontArc> {
        let source = SystemSource::new();
        let mut handle: Option<Handle> = None;

        if let Some(font_name) = font_name {
            if let Ok(family) = source.select_family_by_name(font_name) {
                if let [v, ..] = family.fonts() {
                    handle = v.clone().into();
                }
            }

            if handle.is_none() {
                log::info!("Font '{}' not found!", font_name);
            }
        };

        if handle.is_none() {
            // default font
            handle = source
                .select_best_match(&[FamilyName::SansSerif], &Properties::new())
                .ok();
        }

        let font_data = handle
            .unwrap()
            .load()?
            .copy_font_data()
            .ok_or_else(|| anyhow!("faild to load a font."))?;
        let font = ab_glyph::FontArc::try_from_vec(font_data.to_vec())?;

        Ok(font)
    }

    pub fn update_message(&mut self, message: &str) {
        self.message = Some(message.to_string());
        self.tx_osd_message_timer.send(()).log_err();
    }
}

pub struct State {
    pub graphics: GraphicsState,
    pub transition: TransitionState,
    pub image_loader: Arc<Mutex<ImageLoader>>,
    pub default_timer_secs: u32,
    pub current_timer_secs: u32,
    pub paused: bool,
    pub pause_at_last: bool,
    pub fullscreen_ctrl: FullscreenController,
    pub tx_slideshow_timer: mpsc::Sender<TimerState>,
    pub event_proxy: EventLoopProxy<CustomEvent>,
    pub rng: rand::rngs::ThreadRng,
}

impl State {
    pub async fn new(
        window: &Window,
        image_loader: Arc<Mutex<ImageLoader>>,
        conf: config::Config,
        fullscreen_ctrl: FullscreenController,
        tx_slideshow_timer: mpsc::Sender<TimerState>,
        tx_osd_message_timer: mpsc::Sender<()>,
        event_proxy: EventLoopProxy<CustomEvent>,
    ) -> Result<Self> {
        let graphics = GraphicsState::new(window, &conf, tx_osd_message_timer).await?;

        let transition = TransitionState {
            active: false,
            direction: 0.0,
            last_time: Instant::now(),
            time: conf.transition.time,
            random: conf.transition.random,
        };

        let rng = rand::thread_rng();

        let mut instance = Self {
            graphics,
            transition,
            image_loader,
            default_timer_secs: conf.viewer.timer,
            current_timer_secs: conf.viewer.timer,
            paused: conf.viewer.timer == 0,
            pause_at_last: conf.viewer.pause_at_last,
            fullscreen_ctrl,
            tx_slideshow_timer,
            event_proxy,
            rng,
        };

        instance.draw_current_image().log_err();

        Ok(instance)
    }

    pub fn update_transition(&mut self) -> IsTransitionEnd {
        let trans = &mut self.transition;
        let gfx = &mut self.graphics;

        if trans.active && trans.direction != 0.0 {
            let mut is_end = true;

            let delta_time = trans.last_time.elapsed().as_micros() as f32 / 1_000_000.0;
            trans.last_time = Instant::now();

            {
                let amount = if trans.time > 0.0 {
                    let amount = (1.0 / trans.time) * delta_time;
                    if amount > 0.0 {
                        amount
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };

                let mut b = gfx.uniforms.blend;
                if trans.direction > 0.0 {
                    b += amount;

                    if b >= 1.0 {
                        b = 1.0;
                    } else {
                        is_end = false;
                    }
                } else if trans.direction < 0.0 {
                    b -= amount;

                    if b <= 0.0 {
                        b = 0.0;
                    } else {
                        is_end = false;
                    }
                }
                gfx.uniforms.blend = b;
            }

            gfx.queue.write_buffer(
                &gfx.uniform_buffer,
                0,
                bytemuck::cast_slice(&[gfx.uniforms]),
            );

            return is_end;
        }

        true
    }

    pub fn next_image(&mut self, amount: i32) -> Result<()> {
        {
            let mut loader = self.image_loader.lock().unwrap();
            loader.next_index(amount);
        }

        self.draw_current_image()
    }

    pub fn first_image(&mut self) -> Result<()> {
        {
            let mut loader = self.image_loader.lock().unwrap();
            loader.current_index = 0;
        }

        self.draw_current_image()
    }

    pub fn last_image(&mut self) -> Result<()> {
        {
            let mut loader = self.image_loader.lock().unwrap();
            loader.current_index = loader.scanned_paths.len() - 1;
        }

        self.draw_current_image()
    }

    pub fn draw_current_image(&mut self) -> Result<()> {
        let trans = &mut self.transition;
        let gfx = &mut self.graphics;

        if !self.paused {
            self.tx_slideshow_timer.send(TimerState::Play)?;
        }

        {
            // Write background pixels
            for (_, _, pixel) in gfx.diffuse_image_temp.enumerate_pixels_mut() {
                *pixel = gfx.bg_color;
            }

            let mut loader = self.image_loader.lock().unwrap();
            let image_cache = loader.get_current()?;
            let src_image = &image_cache.image;

            if let Some(emsg) = &image_cache.emsg {
                if let Some(path) = &image_cache.path {
                    gfx.update_message(&format!("load error:\n{:?}\n{}", path, emsg));
                } else {
                    gfx.update_message(&format!("load error:\n{}", emsg));
                }
            }

            // Write image pixels
            let src_height = src_image.height();
            let src_width = src_image.width();
            let dst_width = gfx.texture_size.width;
            let dst_height = gfx.texture_size.height;
            let pad_left = dst_width.saturating_sub(src_width) / 2;
            let pad_top = dst_height.saturating_sub(src_height) / 2;
            for (src_x, src_y, pixel) in src_image.enumerate_pixels() {
                let dst_x = pad_left + src_x;
                let dst_y = pad_top + src_y;
                if dst_x < dst_width && dst_y < dst_height {
                    gfx.diffuse_image_temp.put_pixel(dst_x, dst_y, *pixel);
                }
            }

            loader.current_path = image_cache.path.clone();
        }

        gfx.redraw_image();

        let is_primary = gfx.main_texture_index == 0;
        gfx.uniforms.blend = if is_primary { 1.0 } else { 0.0 };
        gfx.uniforms.flip = if is_primary { 0.0 } else { 1.0 };

        if trans.random {
            gfx.uniforms.mode = self.rng.gen_range(0..=TRANSITION_MAX_MODE_IDX);
        }

        {
            let screen_size = if self.fullscreen_ctrl.active {
                self.fullscreen_ctrl.size.unwrap_or(gfx.inner_size)
            } else {
                gfx.inner_size
            };

            let width_scale = screen_size.width as f32 / gfx.texture_size.width as f32;
            let heigh_scale = screen_size.height as f32 / gfx.texture_size.height as f32;
            let ratio = width_scale / heigh_scale;

            gfx.uniforms.resized_window_scale = if ratio > 1.0 {
                [ratio, 1.0]
            } else if ratio < 1.0 {
                [1.0, 1.0 / ratio]
            } else {
                [1.0, 1.0]
            };
        }

        gfx.queue.write_buffer(
            &gfx.uniform_buffer,
            0,
            bytemuck::cast_slice(&[gfx.uniforms]),
        );

        // Start transition
        trans.direction = if is_primary { -1.0 } else { 1.0 };
        self.event_proxy.send_event(CustomEvent::TransitionStart)?;

        gfx.main_texture_index = if is_primary { 1 } else { 0 };

        Ok(())
    }
}
