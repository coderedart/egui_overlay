use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use tracing::{debug, info};
use wgpu::*;
pub struct SurfaceManager {
    /// we create a view for the swapchain image and set it to this field during the `prepare_frame` fn.
    /// users can assume that it will *always* be available during the `UserApp::run` fn. but don't keep any references as
    /// it will be taken and submitted during the `present_frame` method after rendering is done.
    /// surface is always cleared by wgpu, so no need to wipe it again.
    pub surface_view: Option<TextureView>,
    /// once we acquire a swapchain image (surface texture), we will put it here. surface_view will be created from this
    pub surface_current_image: Option<SurfaceTexture>,
    /// this is the window surface
    pub surface: Option<Surface>,
    /// this configuration needs to be updated with the latest resize
    pub surface_config: SurfaceConfiguration,
    /// Surface manager will iterate over this and find the first format that is supported by surface.
    /// if we find one, we will set surface configuration to that format.
    /// if we don't find one, we will just use the first surface format support.
    /// so, if you don't care about the surface format, just set this to an empty vector.
    surface_formats_priority: Vec<TextureFormat>,
}
impl Drop for SurfaceManager {
    fn drop(&mut self) {
        tracing::warn!("dropping wgpu surface");
    }
}
impl SurfaceManager {
    pub fn new(
        window: Option<impl HasRawWindowHandle + HasRawDisplayHandle>,
        transparent: Option<bool>,
        latest_fb_size: [u32; 2],
        instance: &Instance,
        adapter: &Adapter,
        device: &Device,
        surface: Option<Surface>,
        surface_formats_priority: Vec<TextureFormat>,
        surface_config: SurfaceConfiguration,
    ) -> Self {
        let mut surface_manager = Self {
            surface_view: None,
            surface_current_image: None,
            surface,
            surface_config,
            surface_formats_priority,
        };
        surface_manager.reconfigure_surface(
            window,
            transparent,
            latest_fb_size,
            instance,
            adapter,
            device,
        );
        surface_manager
    }
    pub fn create_current_surface_texture_view(
        &mut self,
        latest_fb_size: [u32; 2],
        device: &Device,
    ) {
        if let Some(surface) = self.surface.as_ref() {
            let current_surface_image = surface.get_current_texture().unwrap_or_else(|_| {
                self.surface_config.width = latest_fb_size[0];
                self.surface_config.height = latest_fb_size[1];
                surface.configure(device, &self.surface_config);
                surface.get_current_texture().unwrap_or_else(|e| {
                    panic!("failed to get surface even after reconfiguration. {e}")
                })
            });
            if current_surface_image.suboptimal {
                tracing::warn!("current surface image is suboptimal. ");
            }
            let surface_view = current_surface_image
                .texture
                .create_view(&TextureViewDescriptor {
                    label: Some("surface view"),
                    format: Some(self.surface_config.format),
                    dimension: Some(TextureViewDimension::D2),
                    aspect: TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: 0,
                    array_layer_count: None,
                });

            self.surface_view = Some(surface_view);
            self.surface_current_image = Some(current_surface_image);
        } else {
            tracing::warn!(
                "skipping acquiring the currnet surface image because there's no surface"
            );
        }
    }
    /// This basically checks if the surface needs creating. and then if needed, creates surface if window exists.
    /// then, it does all the work of configuring the surface.
    /// this is used during resume events to create a surface.
    pub fn reconfigure_surface(
        &mut self,
        window: Option<impl HasRawWindowHandle + HasRawDisplayHandle>,
        transparent: Option<bool>,
        latest_fb_size: [u32; 2],
        instance: &Instance,
        adapter: &Adapter,
        device: &Device,
    ) {
        if let Some(window) = &window {
            if self.surface.is_none() {
                self.surface = Some(unsafe {
                    tracing::debug!("creating a surface with {:?}", window.raw_window_handle());
                    instance
                        .create_surface(window)
                        .expect("failed to create surface")
                });
            }

            let capabilities = self.surface.as_ref().unwrap().get_capabilities(adapter);
            let supported_formats = capabilities.formats;
            debug!(
                "supported alpha modes: {:#?}",
                &capabilities.alpha_modes[..]
            );

            if transparent.unwrap_or_default() {
                use CompositeAlphaMode::*;
                let alpha_modes: Vec<CompositeAlphaMode> =
                    capabilities.alpha_modes.iter().copied().collect();
                tracing::info!(?alpha_modes, "supported alpha modes");
                {
                    self.surface_config.alpha_mode = if alpha_modes.contains(&Inherit) {
                        Inherit
                    } else if alpha_modes.contains(&PreMultiplied) {
                        PreMultiplied
                    } else if alpha_modes.contains(&PostMultiplied) {
                        PostMultiplied
                    } else {
                        Auto
                    };
                }
            }
            debug!("supported formats of the surface: {supported_formats:#?}");

            let mut compatible_format_found = false;
            for sfmt in self.surface_formats_priority.iter() {
                debug!("checking if {sfmt:?} is supported");
                if supported_formats.contains(sfmt) {
                    debug!("{sfmt:?} is supported. setting it as surface format");
                    self.surface_config.format = *sfmt;
                    compatible_format_found = true;
                    break;
                }
            }
            if !compatible_format_found {
                if !self.surface_formats_priority.is_empty() {
                    tracing::warn!(
                        "could not find compatible surface format from user provided formats. choosing first supported format instead"
                    );
                }
                self.surface_config.format = supported_formats
                    .iter()
                    .find(|f| f.is_srgb())
                    .copied()
                    .unwrap_or_else(|| {
                        supported_formats
                            .first()
                            .copied()
                            .expect("surface has zero supported texture formats")
                    })
            }
            let view_format = if self.surface_config.format.is_srgb() {
                self.surface_config.format
            } else {
                tracing::warn!(
                    "surface format is not srgb: {:?}",
                    self.surface_config.format
                );
                match self.surface_config.format {
                    TextureFormat::Rgba8Unorm => TextureFormat::Rgba8UnormSrgb,
                    TextureFormat::Bgra8Unorm => TextureFormat::Bgra8UnormSrgb,
                    _ => self.surface_config.format,
                }
            };
            self.surface_config.view_formats = vec![view_format];

            #[cfg(target_os = "emscripten")]
            {
                self.surface_config.view_formats = vec![];
            }

            debug!(
                "using format: {:#?} for surface configuration",
                self.surface_config.format
            );
            self.resize_framebuffer(device, latest_fb_size);
        }
    }

    pub fn resize_framebuffer(&mut self, device: &Device, latest_fb_size: [u32; 2]) {
        self.surface_config.width = latest_fb_size[0];
        self.surface_config.height = latest_fb_size[1];
        info!(
            "reconfiguring surface with config: {:#?}",
            &self.surface_config
        );
        self.surface
            .as_ref()
            .unwrap()
            .configure(device, &self.surface_config);
    }
    pub fn suspend(&mut self) {
        self.surface = None;
        self.surface_current_image = None;
        self.surface_view = None;
    }
}
