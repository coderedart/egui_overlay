mod helpers;
use bytemuck::cast_slice;
use egui::ahash::HashMap;
use egui::TextureId;
use egui::TexturesDelta;
pub use glow;
use glow::{Context as GlowContext, HasContext, *};
use helpers::*;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// This is a simple macro tha checks for any opengl errors, and logs them.
/// But this flushes all commands and forces synchronization with driver, which will slow down your program.
/// So, by default, we only check for errors if `check_gl_error` feature is enabled. otherwise, this does nothing.
///
/// OpenGL also supports "debug callbacks" feature, where it will call our callback when it has any logs. see [GlowConfig::enable_debug] for that
#[macro_export]
macro_rules! glow_error {
    ($glow_context: ident) => {
        #[cfg(feature = "check_gl_error")]
        {
            let error_code = $glow_context.get_error();
            if error_code != glow::NO_ERROR {
                tracing::error!("glow error: {} at line {}", error_code, line!());
            }
        }
    };
}
/// All shaders are targeting #version 300 es
pub const EGUI_VS: &str = include_str!("../egui.vert");
/// output will be in linear space, so make suer to enable framebuffer srgb
pub const EGUI_LINEAR_OUTPUT_FS: &str = include_str!("../egui_linear_output.frag");
/// the output will be in srgb space, so make sure to disable framebuffer srgb.
pub const EGUI_SRGB_OUTPUT_FS: &str = include_str!("../egui_srgb_output.frag");

/// these are config to be provided to browser when requesting a webgl context
///
/// refer to `WebGL context attributes:` config in the link: <https://developer.mozilla.org/en-US/docs/Web/API/HTMLCanvasElement/getContext>
///
/// alternatively, the spec lists all attributes here <https://registry.khronos.org/webgl/specs/latest/1.0/#5.2>
///
/// ```js
/// WebGLContextAttributes {
///     boolean alpha = true;
///     boolean depth = true;
///     boolean stencil = false;
///     boolean antialias = true;
///     boolean premultipliedAlpha = true;
///     boolean preserveDrawingBuffer = false;
///     WebGLPowerPreference powerPreference = "default";
///     boolean failIfMajorPerformanceCaveat = false;
///     boolean desynchronized = false;
/// };
///
/// ```
///
/// we will only support WebGL2 for now. WebGL2 is available in 90+ % of all active devices according to <https://caniuse.com/?search=webgl2>.
#[derive(Debug, Clone, Default)]
pub struct WebGlConfig {
    pub alpha: Option<bool>,
    pub depth: Option<bool>,
    pub stencil: Option<bool>,
    pub antialias: Option<bool>,
    pub premultiplied_alpha: Option<bool>,
    pub preserve_drawing_buffer: Option<bool>,
    /// possible values are "default", "high-performance", "low-power"
    /// `None`: default.
    /// `Some(true)`: lower power
    /// `Some(false)`: high performance
    pub low_power: Option<bool>,
    pub fail_if_major_performance_caveat: Option<bool>,
    pub desynchronized: Option<bool>,
}

pub struct GlowBackend {
    /// This is the glow context containing opengl function pointers.
    /// clone and use it however you want to
    pub glow_context: Arc<GlowContext>,
    /// size of the framebuffer
    /// call resize framebuffer so that we can resize viewport
    pub framebuffer_size: [u32; 2],
    pub painter: Painter,
}

impl Drop for GlowBackend {
    fn drop(&mut self) {
        unsafe { self.painter.destroy(&self.glow_context) };
    }
}

/// Configuration for Glow context when you are creating one
#[derive(Debug, Default)]
pub struct GlowConfig {
    pub webgl_config: WebGlConfig,
    /// This will set the debug callbacks, which will be used by gl drivers to log any gl errors via [tracing].
    /// default is true, as it can be helpful to figure out any errors.
    /// After creating opengl context, you might want to enable synchronous debug callbacks
    /// `glow_context.enable(glow::DEBUG_OUTPUT_SYNCHRONOUS);`
    /// This will make sure that if an opengl fn causes an error, it will be immediately calling the callback and logging the error.
    /// If you are fine with delaying the debug callbacks, for the sake of performance, then make sure to *disable* it
    /// `glow_context.disable(glow::DEBUG_OUTPUT_SYNCHRONOUS);`
    ///
    /// For more information, read <https://www.khronos.org/opengl/wiki/Debug_Output>
    ///
    /// It is always possible to just set this to false, and set the debugging yourself after creating glow context.
    pub enable_debug: bool,
}

impl GlowBackend {
    pub fn new(
        config: GlowConfig,
        get_proc_address: impl FnMut(&str) -> *const std::ffi::c_void,
        framebuffer_size: [u32; 2],
    ) -> Self {
        let glow_context: Arc<glow::Context> =
            unsafe { create_glow_context(get_proc_address, config) };

        if glow_context.supported_extensions().contains("EXT_sRGB")
            || glow_context.supported_extensions().contains("GL_EXT_sRGB")
            || glow_context
                .supported_extensions()
                .contains("GL_ARB_framebuffer_sRGB")
        {
            warn!("srgb support detected by egui glow");
        } else {
            warn!("no srgb support detected by egui glow");
        }

        let painter = unsafe { Painter::new(&glow_context) };
        Self {
            glow_context,
            painter,
            framebuffer_size,
        }
    }

    pub fn prepare_frame(&mut self, _latest_framebuffer_size_getter: impl FnMut() -> [u32; 2]) {
        unsafe {
            self.glow_context.disable(glow::SCISSOR_TEST);
            self.glow_context
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }
    }

    pub fn resize_framebuffer(&mut self, fb_size: [u32; 2]) {
        self.framebuffer_size = fb_size;
        self.painter.screen_size_physical = fb_size;
        unsafe {
            self.glow_context
                .viewport(0, 0, fb_size[0] as i32, fb_size[1] as i32);
        }
    }

    pub fn render_egui(
        &mut self,
        meshes: Vec<egui::ClippedPrimitive>,
        textures_delta: egui::TexturesDelta,
        logical_screen_size: [f32; 2],
    ) {
        unsafe {
            self.painter.prepare_render(
                &self.glow_context,
                meshes,
                textures_delta,
                logical_screen_size,
            );
            self.painter.render_egui(&self.glow_context);
        }
    }
}
pub struct GpuTexture {
    pub handle: glow::NativeTexture,
    pub width: u32,
    pub height: u32,
    pub sampler: NativeSampler,
}

/// Egui Painter using glow::Context
/// Assumptions:
/// 1. srgb framebuffer
/// 2. opengl 3+ on desktop and webgl2 only on web.
/// 3.
pub struct Painter {
    /// Most of these objects are created at startup
    pub linear_sampler: Sampler,
    pub nearest_sampler: Sampler,
    pub font_sampler: Sampler,
    pub managed_textures: HashMap<u64, GpuTexture>,
    pub egui_program: Program,
    pub vao: VertexArray,
    pub vbo: Buffer,
    pub ebo: Buffer,
    pub u_screen_size: UniformLocation,
    pub u_sampler: UniformLocation,
    pub clipped_primitives: Vec<egui::ClippedPrimitive>,
    pub textures_to_delete: Vec<TextureId>,
    /// updated every frame from the egui gfx output struct
    pub logical_screen_size: [f32; 2],
    /// must update on framebuffer resize.
    pub screen_size_physical: [u32; 2],
}

impl Painter {
    /// # Safety
    /// well, its opengl.. so anything can go wrong. but basicaly, make sure that this opengl context is valid/current
    /// and manually call [`Self::destroy`] before dropping this.
    pub unsafe fn new(gl: &glow::Context) -> Self {
        info!("creating glow egui painter");
        unsafe {
            info!("GL Version: {}", gl.get_parameter_string(glow::VERSION));
            info!("GL Renderer: {}", gl.get_parameter_string(glow::RENDERER));
            info!("Gl Vendor: {}", gl.get_parameter_string(glow::VENDOR));
            if gl.version().major > 1 {
                info!(
                    "GLSL version: {}",
                    gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION)
                );
            }
            glow_error!(gl);
            // compile shaders
            let egui_program = create_program_from_src(
                gl,
                EGUI_VS,
                if cfg!(target_arch = "wasm32") {
                    // on wasm, we always assume srgb framebuffer
                    EGUI_LINEAR_OUTPUT_FS
                } else {
                    EGUI_SRGB_OUTPUT_FS
                },
            );
            // shader verification
            glow_error!(gl);
            let u_screen_size = gl
                .get_uniform_location(egui_program, "u_screen_size")
                .expect("failed to find u_screen_size");
            debug!("location of uniform u_screen_size is {u_screen_size:?}");
            let u_sampler = gl
                .get_uniform_location(egui_program, "u_sampler")
                .expect("failed to find u_sampler");
            debug!("location of uniform u_sampler is {u_sampler:?}");
            gl.use_program(Some(egui_program));
            let (vao, vbo, ebo) = create_egui_vao_buffers(gl, egui_program);
            debug!("created egui vao, vbo, ebo");
            let (linear_sampler, nearest_sampler, font_sampler) = create_samplers(gl);
            debug!("created linear and nearest samplers");
            Self {
                managed_textures: Default::default(),
                egui_program,
                vao,
                vbo,
                ebo,
                linear_sampler,
                nearest_sampler,
                font_sampler,
                u_screen_size,
                u_sampler,
                clipped_primitives: Vec::new(),
                textures_to_delete: Vec::new(),
                logical_screen_size: [0.0; 2],
                screen_size_physical: [0; 2],
            }
        }
    }
    /// uploads data to opengl buffers / textures
    /// # Safety
    /// make sure that there's no opengl issues and context is still current
    pub unsafe fn prepare_render(
        &mut self,
        glow_context: &glow::Context,
        meshes: Vec<egui::ClippedPrimitive>,
        textures_delta: TexturesDelta,
        logical_screen_size: [f32; 2],
    ) {
        self.textures_to_delete = textures_delta.free;
        self.clipped_primitives = meshes;
        self.logical_screen_size = logical_screen_size;
        glow_error!(glow_context);

        // update textures
        for (texture_id, delta) in textures_delta.set {
            let sampler = match delta.options.minification {
                egui::TextureFilter::Nearest => self.nearest_sampler,
                egui::TextureFilter::Linear => self.linear_sampler,
            };
            match texture_id {
                TextureId::Managed(managed) => {
                    glow_context.bind_texture(
                        glow::TEXTURE_2D,
                        Some(match self.managed_textures.entry(managed) {
                            std::collections::hash_map::Entry::Occupied(o) => o.get().handle,
                            std::collections::hash_map::Entry::Vacant(v) => {
                                let handle = glow_context
                                    .create_texture()
                                    .expect("failed to create texture");
                                v.insert(GpuTexture {
                                    handle,
                                    width: 0,
                                    height: 0,
                                    sampler: if managed == 0 {
                                        // special sampler for font that would clamp to edge
                                        self.font_sampler
                                    } else {
                                        sampler
                                    },
                                })
                                .handle
                            }
                        }),
                    );
                }
                TextureId::User(_) => todo!(),
            }
            glow_error!(glow_context);

            let (pixels, size): (Vec<u8>, [usize; 2]) = match delta.image {
                egui::ImageData::Color(c) => (
                    c.pixels.iter().flat_map(egui::Color32::to_array).collect(),
                    c.size,
                ),
                egui::ImageData::Font(font_image) => (
                    font_image
                        .srgba_pixels(None)
                        .flat_map(|c| c.to_array())
                        .collect(),
                    font_image.size,
                ),
            };
            if let Some(pos) = delta.pos {
                glow_context.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    pos[0] as i32,
                    pos[1] as i32,
                    size[0] as i32,
                    size[1] as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(&pixels),
                )
            } else {
                match texture_id {
                    TextureId::Managed(key) => {
                        let gpu_tex = self
                            .managed_textures
                            .get_mut(&key)
                            .expect("failed to find texture with key");
                        gpu_tex.width = size[0] as u32;
                        gpu_tex.height = size[1] as u32;
                    }
                    TextureId::User(_) => todo!(),
                }
                glow_context.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::SRGB8_ALPHA8 as i32,
                    size[0] as i32,
                    size[1] as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    Some(&pixels),
                );
            }
            glow_error!(glow_context);
        }
    }
    /// # Safety
    /// uses a bunch of unsfae opengl functions, any of which might segfault.
    pub unsafe fn render_egui(&mut self, glow_context: &glow::Context) {
        let screen_size_physical = self.screen_size_physical;
        let screen_size_logical = self.logical_screen_size;
        let scale = screen_size_physical[0] as f32 / screen_size_logical[0];

        // setup egui configuration
        glow_context.enable(glow::SCISSOR_TEST);
        glow_context.disable(glow::DEPTH_TEST);
        glow_error!(glow_context);
        #[cfg(not(target_arch = "wasm32"))]
        glow_context.disable(glow::FRAMEBUFFER_SRGB);

        glow_error!(glow_context);
        glow_context.active_texture(glow::TEXTURE0);
        glow_error!(glow_context);

        glow_context.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        glow_context.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
        glow_context.bind_vertex_array(Some(self.vao));
        glow_context.enable(glow::BLEND);
        glow_context.blend_equation_separate(glow::FUNC_ADD, glow::FUNC_ADD);
        glow_context.blend_func_separate(
            // egui outputs colors with premultiplied alpha:
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
            // Less important, but this is technically the correct alpha blend function
            // when you want to make use of the framebuffer alpha (for screenshots, compositing, etc).
            glow::ONE_MINUS_DST_ALPHA,
            glow::ONE,
        );
        glow_context.use_program(Some(self.egui_program));
        glow_context.active_texture(glow::TEXTURE0);
        glow_context.uniform_1_i32(Some(&self.u_sampler), 0);
        glow_context.uniform_2_f32_slice(Some(&self.u_screen_size), &screen_size_logical);
        for clipped_primitive in &self.clipped_primitives {
            if let Some(scissor_rect) = scissor_from_clip_rect_opengl(
                &clipped_primitive.clip_rect,
                scale,
                screen_size_physical,
            ) {
                glow_context.scissor(
                    scissor_rect[0] as i32,
                    scissor_rect[1] as i32,
                    scissor_rect[2] as i32,
                    scissor_rect[3] as i32,
                );
            } else {
                continue;
            }
            match clipped_primitive.primitive {
                egui::epaint::Primitive::Mesh(ref mesh) => {
                    glow_context.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
                    glow_context.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
                    glow_context.buffer_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        cast_slice(&mesh.vertices),
                        glow::STREAM_DRAW,
                    );
                    glow_context.buffer_data_u8_slice(
                        glow::ELEMENT_ARRAY_BUFFER,
                        cast_slice(&mesh.indices),
                        glow::STREAM_DRAW,
                    );
                    glow_error!(glow_context);
                    match mesh.texture_id {
                        TextureId::Managed(managed) => {
                            let managed_tex = self
                                .managed_textures
                                .get(&managed)
                                .expect("managed texture cannot be found");
                            glow_context.bind_texture(glow::TEXTURE_2D, Some(managed_tex.handle));

                            glow_context.bind_sampler(0, Some(managed_tex.sampler));
                        }
                        TextureId::User(_) => todo!(),
                    }
                    glow_error!(glow_context);

                    let indices_len: i32 = mesh
                        .indices
                        .len()
                        .try_into()
                        .expect("failed to fit indices length into i32");

                    glow_error!(glow_context);
                    glow_context.draw_elements(glow::TRIANGLES, indices_len, glow::UNSIGNED_INT, 0);

                    glow_error!(glow_context);
                }

                egui::epaint::Primitive::Callback(_) => todo!(),
            }
        }
        glow_error!(glow_context);
        let textures_to_delete = std::mem::take(&mut self.textures_to_delete);
        for tid in textures_to_delete {
            match tid {
                TextureId::Managed(managed) => {
                    glow_context.delete_texture(
                        self.managed_textures
                            .remove(&managed)
                            .expect("can't find texture to delete")
                            .handle,
                    );
                }
                TextureId::User(_) => todo!(),
            }
        }
        glow_error!(glow_context);
    }
    /// # Safety
    /// This must be called only once.
    /// must not use it again because this destroys all the opengl objects.
    pub unsafe fn destroy(&mut self, glow_context: &glow::Context) {
        tracing::warn!("destroying egui glow painter");
        glow_context.delete_sampler(self.linear_sampler);
        glow_context.delete_sampler(self.nearest_sampler);
        for (_, texture) in std::mem::take(&mut self.managed_textures) {
            glow_context.delete_texture(texture.handle);
        }
        glow_context.delete_program(self.egui_program);
        glow_context.delete_vertex_array(self.vao);
        glow_context.delete_buffer(self.vbo);
        glow_context.delete_buffer(self.ebo);
    }
}

/// **NOTE**:
/// egui coordinates are in logical window space with top left being [0, 0].
/// In opengl, bottom left is [0, 0].
/// so, we need to use bottom left clip-rect coordinate as x,y instead of top left.
/// 1. bottom left corner's y coordinate is simply top left corner's y added with clip rect height
/// 2. but this `y` is represents top border + y units. in opengl, we need units from bottom border  
/// 3. we know that for any point y, distance between top and y + distance between bottom and y gives us total height
/// 4. so, height - y units from top gives us y units from bottom.
/// math is suprisingly hard to write down.. just draw it on a paper, it makes sense.
pub fn scissor_from_clip_rect_opengl(
    clip_rect: &egui::Rect,
    scale: f32,
    physical_framebuffer_size: [u32; 2],
) -> Option<[u32; 4]> {
    scissor_from_clip_rect(clip_rect, scale, physical_framebuffer_size).map(|mut arr| {
        arr[1] = physical_framebuffer_size[1] - (arr[1] + arr[3]);
        arr
    })
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
fn scissor_from_clip_rect(
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
