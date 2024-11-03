mod painter;
mod surface;
use std::sync::Arc;
use tracing::{debug, info};
use wgpu::*;

pub use painter::*;
pub use surface::SurfaceManager;
pub use wgpu;

pub struct WgpuConfig {
    pub backends: Backends,
    pub power_preference: PowerPreference,
    pub device_descriptor: DeviceDescriptor<'static>,
    /// If not empty, We will try to iterate over this vector and use the first format that is supported by the surface.
    /// If this is empty or none of the formats in this vector are supported, we will just use the first supported format of the surface.
    pub surface_formats_priority: Vec<TextureFormat>,
    /// we will try to use this config if supported. otherwise, the surface recommended options will be used.   
    pub surface_config: SurfaceConfiguration,
    pub transparent_surface: Option<bool>,
}
impl Default for WgpuConfig {
    fn default() -> Self {
        Self {
            backends: Backends::all(),
            power_preference: PowerPreference::default(),
            device_descriptor: DeviceDescriptor {
                label: Some("my wgpu device"),
                required_features: Default::default(),
                required_limits: Limits::downlevel_defaults(),
                memory_hints: MemoryHints::default(),
            },
            surface_config: SurfaceConfiguration {
                usage: TextureUsages::RENDER_ATTACHMENT,
                format: TextureFormat::Bgra8UnormSrgb,
                width: 0,
                height: 0,
                present_mode: PresentMode::Fifo,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
            surface_formats_priority: vec![],
            transparent_surface: Some(true),
        }
    }
}
/// This provides a Gfx backend for egui using wgpu as the backend
/// If you are making your own wgpu integration, then you can reuse the `EguiPainter` instead which contains only egui render specific data.
pub struct WgpuBackend {
    /// wgpu instance
    pub instance: Arc<Instance>,
    /// wgpu adapter
    pub adapter: Arc<Adapter>,
    /// wgpu device.
    pub device: Arc<Device>,
    /// wgpu queue. if you have commands that you would like to submit, instead push them into `Self::command_encoders`
    pub queue: Arc<Queue>,
    /// contains egui specific wgpu data like textures or buffers or pipelines etc..
    pub painter: EguiPainter,
    pub surface_manager: SurfaceManager,
    /// this is where we store our command encoders. we will create one during the `prepare_frame` fn.
    /// users can just use this. or create new encoders, and push them into this vec.
    /// `wgpu::Queue::submit` is very expensive, so we will submit ALL command encoders at the same time during the `present_frame` method
    /// just before presenting the swapchain image (surface texture).
    pub command_encoders: Vec<CommandEncoder>,
}
impl Drop for WgpuBackend {
    fn drop(&mut self) {
        tracing::warn!("dropping wgpu backend");
    }
}
impl WgpuBackend {
    /// Both surface target and window are basically the same.
    /// But we try to create the surface *twice*. First, with just instance, and if surface
    pub async fn new_async(
        config: WgpuConfig,
        window: Option<Box<dyn WindowHandle>>,
        latest_fb_size: [u32; 2],
    ) -> Self {
        let WgpuConfig {
            power_preference,
            device_descriptor,
            surface_formats_priority,
            surface_config,
            backends,
            transparent_surface,
        } = config;
        debug!("using wgpu backends: {:?}", backends);
        let instance = Arc::new(Instance::new(InstanceDescriptor {
            backends,
            dx12_shader_compiler: Default::default(),
            flags: InstanceFlags::from_build_config(),
            gles_minor_version: Gles3MinorVersion::Automatic,
        }));
        debug!("iterating over all adapters");
        #[cfg(not(target_arch = "wasm32"))]
        for adapter in instance.enumerate_adapters(Backends::all()) {
            debug!("adapter: {:#?}", adapter.get_info());
        }

        let surface = window.map(|w| {
            tracing::debug!("creating a surface");
            instance
                .create_surface(SurfaceTarget::Window(w))
                .expect("failed to create surface")
        });

        info!("is surfaced created at startup?: {}", surface.is_some());

        debug!("using power preference: {:?}", config.power_preference);
        let adapter = Arc::new(
            instance
                .request_adapter(&RequestAdapterOptions {
                    power_preference,
                    force_fallback_adapter: false,
                    compatible_surface: surface.as_ref(),
                })
                .await
                .expect("failed to get adapter"),
        );

        info!("chosen adapter details: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&device_descriptor, Default::default())
            .await
            .expect("failed to create wgpu device");

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_manager = SurfaceManager::new(
            None,
            transparent_surface,
            latest_fb_size,
            &instance,
            &adapter,
            &device,
            surface,
            surface_formats_priority,
            surface_config,
        );

        debug!("device features: {:#?}", device.features());
        debug!("device limits: {:#?}", device.limits());

        let painter = EguiPainter::new(&device, surface_manager.surface_config.format);

        Self {
            instance,
            adapter,
            device,
            queue,
            painter,
            command_encoders: Vec::new(),
            surface_manager,
        }
    }
}
impl WgpuBackend {
    pub fn new(
        config: WgpuConfig,
        window: Option<Box<dyn WindowHandle>>,
        latest_fb_size: [u32; 2],
    ) -> Self {
        pollster::block_on(Self::new_async(config, window, latest_fb_size))
    }

    pub fn resume(
        &mut self,
        window: Option<Box<dyn WindowHandle>>,
        latest_fb_size: [u32; 2],
        transparent: Option<bool>,
    ) {
        self.surface_manager.reconfigure_surface(
            window,
            transparent,
            latest_fb_size,
            &self.instance,
            &self.adapter,
            &self.device,
        );
        self.painter.on_resume(
            &self.device,
            self.surface_manager
                .surface_config
                .view_formats
                .first()
                .copied()
                .unwrap(),
        );
    }

    pub fn prepare_frame(&mut self, latest_framebuffer_size_getter: impl FnMut() -> [u32; 2]) {
        self.surface_manager
            .create_current_surface_texture_view(latest_framebuffer_size_getter, &self.device);
        if let Some(view) = self.surface_manager.surface_view.as_ref() {
            let mut ce = self
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: "surface clear ce".into(),
                });
            ce.begin_render_pass(&RenderPassDescriptor {
                label: "surface clear rpass".into(),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.command_encoders.push(ce);
        }
    }

    pub fn render_egui(
        &mut self,
        meshes: Vec<egui::ClippedPrimitive>,
        textures_delta: egui::TexturesDelta,
        logical_screen_size: [f32; 2],
    ) {
        let mut command_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("egui command encoder"),
            });
        let draw_calls = self.painter.upload_egui_data(
            &self.device,
            &self.queue,
            meshes,
            textures_delta,
            logical_screen_size,
            [
                self.surface_manager.surface_config.width,
                self.surface_manager.surface_config.height,
            ],
            &mut command_encoder,
        );
        {
            let mut egui_pass = command_encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("egui render pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: self
                        .surface_manager
                        .surface_view
                        .as_ref()
                        .expect("failed ot get surface view for egui render pass creation"),
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.painter
                .draw_egui_with_renderpass(&mut egui_pass, draw_calls);
        }
        self.command_encoders.push(command_encoder);
    }

    pub fn present(&mut self) {
        assert!(self.surface_manager.surface_view.is_some());
        self.queue.submit(
            std::mem::take(&mut self.command_encoders)
                .into_iter()
                .map(|encoder| encoder.finish()),
        );
        {
            self.surface_manager
                .surface_view
                .take()
                .expect("failed to get surface view to present");
        }
        self.surface_manager
            .surface_current_image
            .take()
            .expect("failed to surface texture to preset")
            .present();
    }

    pub fn resize_framebuffer(&mut self, latest_fb_size: [u32; 2]) {
        self.surface_manager
            .resize_framebuffer(&self.device, latest_fb_size);
    }

    pub fn suspend(&mut self) {
        self.surface_manager.suspend();
    }
}
/// input: clip rectangle in logical pixels, scale and framebuffer size in physical pixels
/// we will get [x, y, width, height] of the scissor rectangle.
///
/// internally, it will
/// 1. multiply clip rect and scale  to convert the logical rectangle to a physical rectangle in framebuffer space.
/// 2. clamp the rectangle between 0..width and 0..height of the frambuffer. make sure that width/height are positive/zero.
/// 3. return Some only if width/height of scissor region are not zero.
///
/// This fn is for wgpu/metal/directx.
pub fn scissor_from_clip_rect(
    clip_rect: &egui::Rect,
    scale: f32,
    physical_framebuffer_size: [u32; 2],
) -> Option<[u32; 4]> {
    // copy paste from official egui impl because i have no idea what this is :D

    // first, we turn the clip rectangle into physical framebuffer coordinates
    // clip_min is top left point and clip_max is bottom right.
    let clip_min_x = scale * clip_rect.min.x;
    let clip_min_y = scale * clip_rect.min.y;
    let clip_max_x = scale * clip_rect.max.x;
    let clip_max_y = scale * clip_rect.max.y;

    // round to integers
    let clip_min_x = clip_min_x.round() as i32;
    let clip_min_y = clip_min_y.round() as i32;
    let clip_max_x = clip_max_x.round() as i32;
    let clip_max_y = clip_max_y.round() as i32;

    // clamp top_left of clip rect to be within framebuffer bounds
    let clip_min_x = clip_min_x.clamp(0, physical_framebuffer_size[0] as i32);
    let clip_min_y = clip_min_y.clamp(0, physical_framebuffer_size[1] as i32);
    // clamp bottom right of clip rect to be between top_left of clip rect and framebuffer bottom right bounds
    let clip_max_x = clip_max_x.clamp(clip_min_x, physical_framebuffer_size[0] as i32);
    let clip_max_y = clip_max_y.clamp(clip_min_y, physical_framebuffer_size[1] as i32);
    // x,y are simply top left coords
    let x = clip_min_x as u32;
    let y = clip_min_y as u32;
    // width height by subtracting bottom right with top left coords.
    let width = (clip_max_x - clip_min_x) as u32;
    let height = (clip_max_y - clip_min_y) as u32;
    // return only if scissor width/height are not zero. otherwise, no need for a scissor rect at all
    (width != 0 && height != 0).then_some([x, y, width, height])
}
