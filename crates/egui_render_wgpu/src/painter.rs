use std::{collections::BTreeMap, num::NonZeroU64, sync::Arc};

use bytemuck::cast_slice;
use egui::{
    epaint::{ImageDelta, Primitive},
    util::IdTypeMap,
    *,
};
use wgpu::*;

pub struct EguiPainter {
    /// current capacity of vertex buffer
    pub vb_len: usize,
    /// current capacity of index buffer
    pub ib_len: usize,
    /// vertex buffer for all egui (clipped) meshes
    pub vb: Buffer,
    /// index buffer for all egui (clipped) meshes
    pub ib: Buffer,
    /// Uniform buffer to store screen size in logical points
    pub screen_size_buffer: Buffer,
    /// bind group for the Uniform buffer using layout entry [`SCREEN_SIZE_UNIFORM_BUFFER_BINDGROUP_ENTRY`]
    pub screen_size_bind_group: BindGroup,
    /// this layout is reused by all egui textures.
    pub texture_bindgroup_layout: BindGroupLayout,
    /// used by pipeline create function
    pub screen_size_bindgroup_layout: BindGroupLayout,
    /// The current pipeline has been created with this format as the output
    /// If we need to render to a different format, then we need to recreate the render pipeline with the relevant format as output
    pub surface_format: TextureFormat,
    /// egui render pipeline
    pub pipeline: RenderPipeline,
    /// This is the sampler used for most textures that user uploads
    pub linear_sampler: Sampler,
    /// nearest sampler suitable for font textures (or any pixellated textures)
    pub nearest_sampler: Sampler,
    pub font_sampler: Sampler,
    /// Textures uploaded by egui itself.
    pub managed_textures: BTreeMap<u64, EguiTexture>,
    /// these are exposed to user so that they can edit them or insert any custom textures which aren't supported by egui like texture wrapping or array textures etc..
    pub user_textures: BTreeMap<u64, EguiTexture>,
    /// textures to free
    pub delete_textures: Vec<TextureId>,
    pub custom_data: IdTypeMap,
    pub mipmap_pipeline: RenderPipeline,
    pub mipmap_bgl: BindGroupLayout,
    pub mipmap_sampler: Sampler,
}

pub const EGUI_SHADER_SRC: &str = include_str!("../egui.wgsl");

type PrepareCallback = dyn Fn(&Device, &Queue, &mut IdTypeMap) + Sync + Send;
type RenderCallback =
    dyn for<'a, 'b> Fn(PaintCallbackInfo, &'a mut RenderPass<'b>, &'b IdTypeMap) + Sync + Send;

pub struct CallbackFn {
    pub prepare: Arc<PrepareCallback>,
    pub paint: Arc<RenderCallback>,
}

impl Default for CallbackFn {
    fn default() -> Self {
        CallbackFn {
            prepare: Arc::new(|_, _, _| ()),
            paint: Arc::new(|_, _, _| ()),
        }
    }
}

/// We take all the
pub enum EguiDrawCalls {
    Mesh {
        clip_rect: [u32; 4],
        texture_id: TextureId,
        base_vertex: i32,
        index_start: u32,
        index_end: u32,
    },
    Callback {
        paint_callback_info: PaintCallbackInfo,
        clip_rect: [u32; 4],
        paint_callback: PaintCallback,
    },
}
impl EguiPainter {
    pub fn draw_egui_with_renderpass<'rpass>(
        &'rpass self,
        rpass: &mut RenderPass<'rpass>,
        draw_calls: Vec<EguiDrawCalls>,
    ) {
        if self.vb.size() == 0 {
            return;
        }
        // rpass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.screen_size_bind_group, &[]);

        rpass.set_vertex_buffer(0, self.vb.slice(..));
        rpass.set_index_buffer(self.ib.slice(..), IndexFormat::Uint32);
        for draw_call in draw_calls {
            match draw_call {
                EguiDrawCalls::Mesh {
                    clip_rect,
                    texture_id,
                    base_vertex,
                    index_start,
                    index_end,
                } => {
                    let [x, y, width, height] = clip_rect;
                    rpass.set_scissor_rect(x, y, width, height);
                    // In webgl, base vertex is not supported in the draw_indexed function (draw elements in webgl2).
                    // so, we instead bind the buffer with different offsets every call so that indices will point to their respective vertices.
                    // this is possible because webgl2 has bindBufferRange (which allows specifying a offset as the start of the buffer binding)
                    rpass.set_vertex_buffer(0, self.vb.slice(base_vertex as u64 * 20..));
                    match texture_id {
                        TextureId::Managed(key) => {
                            rpass.set_bind_group(
                                1,
                                &self
                                    .managed_textures
                                    .get(&key)
                                    .expect("cannot find managed texture")
                                    .bindgroup,
                                &[],
                            );
                        }
                        TextureId::User(_) => unimplemented!(),
                    }
                    rpass.draw_indexed(index_start..index_end, 0, 0..1);
                }
                EguiDrawCalls::Callback {
                    clip_rect,
                    paint_callback,
                    paint_callback_info,
                } => {
                    let [x, y, width, height] = clip_rect;
                    rpass.set_scissor_rect(x, y, width, height);
                    (paint_callback
                        .callback
                        .downcast_ref::<CallbackFn>()
                        .expect("failed to downcast Callbackfn")
                        .paint)(
                        PaintCallbackInfo {
                            viewport: paint_callback_info.viewport,
                            clip_rect: paint_callback_info.clip_rect,
                            pixels_per_point: paint_callback_info.pixels_per_point,
                            screen_size_px: paint_callback_info.screen_size_px,
                        },
                        rpass,
                        &self.custom_data,
                    );
                }
            }
        }
    }
    pub fn create_render_pipeline(
        dev: &Device,
        pipeline_surface_format: TextureFormat,
        screen_size_bindgroup_layout: &BindGroupLayout,
        texture_bindgroup_layout: &BindGroupLayout,
    ) -> RenderPipeline {
        // pipeline layout. screensize uniform buffer for vertex shader + texture and sampler for fragment shader
        let egui_pipeline_layout = dev.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("egui pipeline layout"),
            bind_group_layouts: &[screen_size_bindgroup_layout, texture_bindgroup_layout],
            push_constant_ranges: &[],
        });
        // shader from the wgsl source.
        let shader_module = dev.create_shader_module(ShaderModuleDescriptor {
            label: Some("egui shader src"),
            source: ShaderSource::Wgsl(EGUI_SHADER_SRC.into()),
        });
        // create pipeline using shaders + pipeline layout
        dev.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("egui pipeline"),
            layout: Some(&egui_pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &VERTEX_BUFFER_LAYOUT,
                compilation_options: PipelineCompilationOptions {
                    constants: &Default::default(),
                    zero_initialize_workgroup_memory: false,
                },
            },
            primitive: EGUI_PIPELINE_PRIMITIVE_STATE,
            depth_stencil: None,
            // support multi sampling in future?
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: Some(if pipeline_surface_format.is_srgb() {
                    "fs_main_linear_output"
                } else {
                    "fs_main_srgb_output"
                }),
                targets: &[Some(ColorTargetState {
                    format: pipeline_surface_format,
                    blend: Some(EGUI_PIPELINE_BLEND_STATE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions {
                    constants: &Default::default(),
                    zero_initialize_workgroup_memory: false,
                },
            }),
            multiview: None,
            cache: None,
        })
    }
    pub fn new(dev: &Device, surface_format: TextureFormat) -> Self {
        // create uniform buffer for screen size
        let screen_size_buffer = dev.create_buffer(&BufferDescriptor {
            label: Some("screen size uniform buffer"),
            size: 16,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // create temporary layout to create screensize uniform buffer bindgroup
        let screen_size_bindgroup_layout =
            dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("egui screen size bindgroup layout"),
                entries: &SCREEN_SIZE_UNIFORM_BUFFER_BINDGROUP_ENTRY,
            });
        // create texture bindgroup layout. all egui textures need to have a bindgroup with this layout to use
        // them in egui draw calls.
        let texture_bindgroup_layout = dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("egui texture bind group layout"),
            entries: &TEXTURE_BINDGROUP_ENTRIES,
        });
        // create screen size bind group with the above layout. store this permanently to bind before drawing egui.
        let screen_size_bind_group = dev.create_bind_group(&BindGroupDescriptor {
            label: Some("egui bindgroup"),
            layout: &screen_size_bindgroup_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &screen_size_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        let pipeline = Self::create_render_pipeline(
            dev,
            surface_format,
            &screen_size_bindgroup_layout,
            &texture_bindgroup_layout,
        );

        // linear and nearest samplers for egui textures to use for creation of their bindgroups
        let linear_sampler = dev.create_sampler(&SamplerDescriptor {
            label: Some("linear sampler"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            ..Default::default()
        });
        let nearest_sampler = dev.create_sampler(&SamplerDescriptor {
            label: Some("nearest sampler"),
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            ..Default::default()
        });

        let font_sampler = dev.create_sampler(&SamplerDescriptor {
            label: Some("egui font sampler"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            ..Default::default()
        });
        // empty vertex and index buffers.
        let vb = dev.create_buffer(&BufferDescriptor {
            label: Some("egui vertex buffer"),
            size: 0,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ib = dev.create_buffer(&BufferDescriptor {
            label: Some("egui index buffer"),
            size: 0,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mipmap_shader = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blit Shader for Mipmaps"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "../blit.wgsl"
            ))),
        });

        let mipmap_pipeline = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &mipmap_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions {
                    constants: &Default::default(),
                    zero_initialize_workgroup_memory: false,
                },
            },
            fragment: Some(wgpu::FragmentState {
                module: &mipmap_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(TextureFormat::Rgba8UnormSrgb.into())],
                compilation_options: PipelineCompilationOptions {
                    constants: &Default::default(),
                    zero_initialize_workgroup_memory: false,
                },
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let mipmap_bgl = dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("mipmap bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let mipmap_sampler = dev.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mipmap sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            screen_size_buffer,
            pipeline,
            linear_sampler,
            nearest_sampler,
            managed_textures: Default::default(),
            user_textures: Default::default(),
            vb,
            ib,
            screen_size_bind_group,
            texture_bindgroup_layout,
            vb_len: 0,
            ib_len: 0,
            delete_textures: Vec::new(),
            custom_data: IdTypeMap::default(),
            screen_size_bindgroup_layout,
            surface_format,
            mipmap_pipeline,
            mipmap_bgl,
            mipmap_sampler,
            font_sampler,
        }
    }
    pub fn on_resume(&mut self, dev: &Device, surface_format: TextureFormat) {
        if self.surface_format != surface_format {
            self.pipeline = Self::create_render_pipeline(
                dev,
                surface_format,
                &self.screen_size_bindgroup_layout,
                &self.texture_bindgroup_layout,
            );
        }
    }
    fn set_textures(
        &mut self,
        dev: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        textures_delta_set: Vec<(TextureId, ImageDelta)>,
    ) {
        let mut textures_needing_mipmap_generation = vec![];
        for (tex_id, delta) in textures_delta_set {
            let width = delta.image.width() as u32;
            let height = delta.image.height() as u32;

            let size = Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let mut is_this_font_texure = false;
            // no need for mipmaps if we are dealing with font texture
            let mip_level_count = match tex_id {
                TextureId::Managed(0) => {
                    is_this_font_texure = true;
                    1
                }
                _ => {
                    let mip_level_count = (width.max(height) as f32).log2().floor() as u32 + 1;
                    textures_needing_mipmap_generation.push((tex_id, mip_level_count));
                    mip_level_count
                }
            };
            let data_color32 = match delta.image {
                ImageData::Color(color_image) => color_image.pixels.clone(),
                ImageData::Font(font_image) => font_image.srgba_pixels(None).collect::<Vec<_>>(),
            };

            let data_bytes: &[u8] = bytemuck::cast_slice(data_color32.as_slice());

            if let Some(delta_pos) = delta.pos {
                let tex = match tex_id {
                    TextureId::Managed(tid) => self.managed_textures.get(&tid),
                    TextureId::User(tid) => self.user_textures.get(&tid),
                };
                // we only update part of the texture, if the tex id refers to a live texture
                if let Some(tex) = tex {
                    queue.write_texture(
                        ImageCopyTexture {
                            texture: &tex.texture,
                            mip_level: 0,
                            origin: Origin3d {
                                x: delta_pos[0].try_into().unwrap(),
                                y: delta_pos[1].try_into().unwrap(),
                                z: 0,
                            },
                            aspect: TextureAspect::All,
                        },
                        data_bytes,
                        ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(size.width * 4),
                            // only required in 3d textures or 2d array textures
                            rows_per_image: None,
                        },
                        size,
                    );
                }
            } else {
                let new_texture = dev.create_texture(&TextureDescriptor {
                    label: None,
                    size,
                    mip_level_count,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba8UnormSrgb,
                    usage: TextureUsages::TEXTURE_BINDING
                        | TextureUsages::COPY_DST
                        | TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[TextureFormat::Rgba8UnormSrgb],
                });

                queue.write_texture(
                    ImageCopyTexture {
                        texture: &new_texture,
                        mip_level: 0,
                        origin: Origin3d::default(),
                        aspect: TextureAspect::All,
                    },
                    data_bytes,
                    ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(size.width * 4),
                        rows_per_image: None,
                    },
                    size,
                );
                let view = new_texture.create_view(&TextureViewDescriptor {
                    label: None,
                    format: Some(TextureFormat::Rgba8UnormSrgb),
                    dimension: Some(TextureViewDimension::D2),
                    aspect: TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: Some(mip_level_count),
                    base_array_layer: 0,
                    array_layer_count: None,
                });
                assert!(delta.options.magnification == delta.options.minification);
                let bindgroup = dev.create_bind_group(&BindGroupDescriptor {
                    label: None,
                    layout: &self.texture_bindgroup_layout,
                    entries: &[
                        BindGroupEntry {
                            binding: 0,
                            resource: BindingResource::TextureView(&view),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: BindingResource::Sampler(if is_this_font_texure {
                                &self.font_sampler
                            } else {
                                match delta.options.magnification {
                                    TextureFilter::Nearest => &self.nearest_sampler,
                                    TextureFilter::Linear => &self.linear_sampler,
                                }
                            }),
                        },
                    ],
                });
                let tex = EguiTexture {
                    texture: new_texture,
                    view,
                    bindgroup,
                };
                match tex_id {
                    TextureId::Managed(tid) => {
                        self.managed_textures.insert(tid, tex);
                    }
                    TextureId::User(tid) => {
                        self.user_textures.insert(tid, tex);
                    }
                }
            }
        }
        for (tex_id, mipmap_level_count) in textures_needing_mipmap_generation {
            let texture = match tex_id {
                TextureId::Managed(tid) => self.managed_textures.get(&tid),
                TextureId::User(tid) => self.user_textures.get(&tid),
            };
            if let Some(texture) = texture {
                let views = (0..mipmap_level_count)
                    .map(|mip| {
                        texture.texture.create_view(&wgpu::TextureViewDescriptor {
                            label: Some("mip"),
                            format: None,
                            dimension: None,
                            aspect: wgpu::TextureAspect::All,
                            base_mip_level: mip,
                            mip_level_count: Some(1),
                            base_array_layer: 0,
                            array_layer_count: None,
                        })
                    })
                    .collect::<Vec<_>>();

                for target_mip in 1..mipmap_level_count as usize {
                    let bind_group = dev.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &self.mipmap_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(
                                    &views[target_mip - 1],
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.mipmap_sampler),
                            },
                        ],
                        label: Some("mipmap bindgroup"),
                    });

                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &views[target_mip],
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                                store: StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });

                    rpass.set_pipeline(&self.mipmap_pipeline);
                    rpass.set_bind_group(0, &bind_group, &[]);
                    rpass.draw(0..3, 0..1);
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn upload_egui_data(
        &mut self,
        dev: &Device,
        queue: &Queue,
        meshes: Vec<ClippedPrimitive>,
        textures_delta: TexturesDelta,
        logical_screen_size: [f32; 2],
        physical_framebuffer_size: [u32; 2],
        encoder: &mut CommandEncoder,
    ) -> Vec<EguiDrawCalls> {
        let scale = physical_framebuffer_size[0] as f32 / logical_screen_size[0];
        // first deal with textures
        {
            // we need to delete textures in textures_delta.free AFTER the draw calls
            // so we store them in self.delete_textures and delete them next frame.
            // otoh, the textures that were scheduled to be deleted previous frame, we will delete now

            let delete_textures = std::mem::replace(&mut self.delete_textures, textures_delta.free);
            // remove textures to be deleted in previous frame
            for tid in delete_textures {
                match tid {
                    TextureId::Managed(key) => {
                        self.managed_textures.remove(&key);
                    }
                    TextureId::User(key) => {
                        self.user_textures.remove(&key);
                    }
                }
            }
            // upload textures
            self.set_textures(dev, queue, encoder, textures_delta.set);
        }
        // update screen size uniform buffer
        queue.write_buffer(
            &self.screen_size_buffer,
            0,
            bytemuck::cast_slice(&logical_screen_size),
        );

        {
            // total vertices and indices lengths
            let (vb_len, ib_len) = meshes.iter().fold((0, 0), |(vb_len, ib_len), mesh| {
                if let Primitive::Mesh(ref m) = mesh.primitive {
                    (vb_len + m.vertices.len(), ib_len + m.indices.len())
                } else {
                    (vb_len, ib_len)
                }
            });
            if vb_len == 0 || ib_len == 0 {
                return meshes
                    .into_iter()
                    .filter_map(|p| match p.primitive {
                        Primitive::Mesh(_) => None,
                        Primitive::Callback(cb) => {
                            (cb.callback
                                .downcast_ref::<CallbackFn>()
                                .expect("failed to downcast egui callback fn")
                                .prepare)(
                                dev, queue, &mut self.custom_data
                            );
                            crate::scissor_from_clip_rect(
                                &p.clip_rect,
                                scale,
                                physical_framebuffer_size,
                            )
                            .map(|clip_rect| EguiDrawCalls::Callback {
                                clip_rect,
                                paint_callback: cb,
                                paint_callback_info: PaintCallbackInfo {
                                    viewport: Rect::from_min_size(
                                        Default::default(),
                                        logical_screen_size.into(),
                                    ),
                                    clip_rect: p.clip_rect,
                                    pixels_per_point: scale,
                                    screen_size_px: physical_framebuffer_size,
                                },
                            })
                        }
                    })
                    .collect();
            }

            // resize if vertex or index buffer capcities are not enough
            if self.vb_len < vb_len {
                self.vb = dev.create_buffer(&BufferDescriptor {
                    label: Some("egui vertex buffer"),
                    size: vb_len as u64 * 20,
                    usage: BufferUsages::COPY_DST | BufferUsages::VERTEX,
                    mapped_at_creation: false,
                });
                self.vb_len = vb_len;
            }
            if self.ib_len < ib_len {
                self.ib = dev.create_buffer(&BufferDescriptor {
                    label: Some("egui index buffer"),
                    size: ib_len as u64 * 4,
                    usage: BufferUsages::COPY_DST | BufferUsages::INDEX,
                    mapped_at_creation: false,
                });
                self.ib_len = ib_len;
            }
            // create mutable slices for vertex and index buffers
            let mut vertex_buffer_mut = queue
                .write_buffer_with(
                    &self.vb,
                    0,
                    NonZeroU64::new(
                        (self.vb_len * 20)
                            .try_into()
                            .expect("unreachable as usize is u64"),
                    )
                    .expect("vertex buffer length should not be zero"),
                )
                .expect("failed to create queuewritebufferview");
            let mut index_buffer_mut = queue
                .write_buffer_with(
                    &self.ib,
                    0,
                    NonZeroU64::new(
                        (self.ib_len * 4)
                            .try_into()
                            .expect("unreachable as usize is u64"),
                    )
                    .expect("index buffer length should not be zero"),
                )
                .expect("failed to create queuewritebufferview");
            // offsets from where to start writing vertex or index buffer data
            let mut vb_offset = 0;
            let mut ib_offset = 0;
            let mut draw_calls = vec![];
            for clipped_primitive in meshes {
                let ClippedPrimitive {
                    clip_rect,
                    primitive,
                } = clipped_primitive;
                let primitive_clip_rect = clip_rect;
                let clip_rect = if let Some(c) = crate::scissor_from_clip_rect(
                    &primitive_clip_rect,
                    scale,
                    physical_framebuffer_size,
                ) {
                    c
                } else {
                    continue;
                };

                match primitive {
                    Primitive::Mesh(mesh) => {
                        let Mesh {
                            indices,
                            vertices,
                            texture_id,
                        } = mesh;

                        // offset upto where we want to write the vertices or indices.
                        let new_vb_offset = vb_offset + vertices.len() * 20; // multiply by vertex size as slice is &[u8]
                        let new_ib_offset = ib_offset + indices.len() * 4; // multiply by index size as slice is &[u8]
                                                                           // write from start offset to end offset
                        vertex_buffer_mut[vb_offset..new_vb_offset]
                            .copy_from_slice(cast_slice(&vertices));
                        index_buffer_mut[ib_offset..new_ib_offset]
                            .copy_from_slice(cast_slice(&indices));
                        // record draw call
                        draw_calls.push(EguiDrawCalls::Mesh {
                            clip_rect,
                            texture_id,
                            // vertex buffer offset is in bytes. so, we divide by size to get the "nth" vertex to use as base
                            base_vertex: (vb_offset / 20)
                                .try_into()
                                .expect("failed to fit vertex buffer offset into i32"),
                            // ib offset is in bytes. divided by index size, we get the starting and ending index to use for this draw call
                            index_start: (ib_offset / 4) as u32,
                            index_end: (new_ib_offset / 4) as u32,
                        });
                        // set end offsets as start offsets for next iteration
                        vb_offset = new_vb_offset;
                        ib_offset = new_ib_offset;
                    }
                    Primitive::Callback(cb) => {
                        (cb.callback
                            .downcast_ref::<CallbackFn>()
                            .expect("failed to downcast egui callback fn")
                            .prepare)(dev, queue, &mut self.custom_data);
                        draw_calls.push(EguiDrawCalls::Callback {
                            clip_rect,
                            paint_callback: cb,
                            paint_callback_info: PaintCallbackInfo {
                                viewport: Rect::from_min_size(
                                    Default::default(),
                                    logical_screen_size.into(),
                                ),
                                clip_rect: primitive_clip_rect,
                                pixels_per_point: scale,
                                screen_size_px: physical_framebuffer_size,
                            },
                        });
                    }
                }
            }
            draw_calls
        }
    }
}

pub const SCREEN_SIZE_UNIFORM_BUFFER_BINDGROUP_ENTRY: [BindGroupLayoutEntry; 1] =
    [BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::VERTEX,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(16),
        },
        count: None,
    }];

pub const TEXTURE_BINDGROUP_ENTRIES: [BindGroupLayoutEntry; 2] = [
    BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: true },
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    },
    BindGroupLayoutEntry {
        binding: 1,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(SamplerBindingType::Filtering),
        count: None,
    },
];
pub const VERTEX_BUFFER_LAYOUT: [VertexBufferLayout; 1] = [VertexBufferLayout {
    // vertex size
    array_stride: 20,
    step_mode: VertexStepMode::Vertex,
    attributes: &[
        // position x, y
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        // texture coordinates x, y
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 8,
            shader_location: 1,
        },
        // color as rgba (unsigned bytes which will be turned into floats inside shader)
        VertexAttribute {
            format: VertexFormat::Unorm8x4,
            offset: 16,
            shader_location: 2,
        },
    ],
}];

pub const EGUI_PIPELINE_PRIMITIVE_STATE: PrimitiveState = PrimitiveState {
    topology: PrimitiveTopology::TriangleList,
    strip_index_format: None,
    front_face: FrontFace::Ccw,
    cull_mode: None,
    unclipped_depth: false,
    polygon_mode: PolygonMode::Fill,
    conservative: false,
};

pub const EGUI_PIPELINE_BLEND_STATE: BlendState = BlendState {
    color: BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
        operation: BlendOperation::Add,
    },
    alpha: BlendComponent {
        src_factor: BlendFactor::OneMinusDstAlpha,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    },
};
pub struct EguiTexture {
    pub texture: Texture,
    pub view: TextureView,
    pub bindgroup: BindGroup,
}
