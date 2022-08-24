use std::num::{NonZeroU32, NonZeroU64};

use bytemuck::cast_slice;
use egui::{ClippedPrimitive, PaintCallback, TextureId, TexturesDelta};
use intmap::IntMap;

use graph::*;
use rend3::{
    graph::{RenderGraph, RenderTargetHandle},
    *,
};
use wgpu::*;
// pub const EGUI_SHADER_MODULE: ShaderModuleDescriptor = ;
pub struct EguiRenderRoutine {
    pub egui_render_data: EguiRenderData,
}
pub struct EguiRenderOutput {
    pub meshes: Vec<ClippedPrimitive>,
    pub textures_delta: TexturesDelta,
    pub scale: f32,
    pub window_size: Option<[f32; 2]>,
    pub fb_size: [u32; 2],
}
impl EguiRenderRoutine {
    pub fn new(renderer: &Renderer, surface_format: TextureFormat) -> Self {
        let egui_render_data = EguiRenderData::new(&renderer.device, surface_format);
        Self { egui_render_data }
    }
    pub fn add_to_graph<'node>(
        &'node mut self,
        graph: &mut RenderGraph<'node>,
        input: EguiRenderOutput,
        output: RenderTargetHandle,
    ) {
        let mut builder = graph.add_node("egui");

        let output_handle = builder.add_render_target_output(output);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: None,
        });

        let pt_handle = builder.passthrough_ref_mut(self);

        builder.build(
            move |pt, renderer, encoder_or_pass, _temps, _ready, _graph_data| {
                let this = pt.get_mut(pt_handle);
                let rpass = encoder_or_pass.get_rpass(rpass_handle);

                this.egui_render_data.update_data(
                    renderer,
                    input.meshes,
                    input.window_size,
                    input.fb_size,
                    input.scale,
                    input.textures_delta,
                );

                this.egui_render_data.execute_with_renderpass(rpass);
            },
        );
    }
}

pub struct EguiRenderData {
    screen_size_ub: Buffer,
    screen_size_bindgroup: BindGroup,
    pipeline: RenderPipeline,
    vb: Buffer,
    vb_len: usize,
    ib: Buffer,
    ib_len: usize,
    linear_sampler: Sampler,
    nearest_sampler: Sampler,
    texture_layout: BindGroupLayout,
    draw_calls: Vec<DrawCallInfo>,
    managed_textures: IntMap<EguiManagedTexture>,
    textures_to_clear: Vec<TextureId>,
}
pub struct EguiManagedTexture {
    pub texture: Texture,
    pub view: TextureView,
    pub bindgroup: BindGroup,
}

pub enum DrawCallInfo {
    Mesh {
        clip_rect: [u32; 4],
        vb_bounds: [usize; 2],
        ib_bounds: [usize; 2],
        texture: u64,
    },
    Callback(PaintCallback),
}

impl EguiRenderData {
    pub fn execute_with_renderpass<'pass>(&'pass mut self, render_pass: &mut RenderPass<'pass>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.screen_size_bindgroup, &[]);
        render_pass.set_vertex_buffer(0, self.vb.slice(..));
        render_pass.set_index_buffer(self.ib.slice(..), IndexFormat::Uint32);
        for draw_call in self.draw_calls.iter() {
            match draw_call {
                DrawCallInfo::Mesh {
                    clip_rect,
                    vb_bounds,
                    ib_bounds,
                    texture,
                } => {
                    let [x, y, width, height] = *clip_rect;
                    if width != 0 && height != 0 {
                        render_pass.set_scissor_rect(x, y, width, height);
                    } else {
                        continue;
                    }
                    render_pass.set_bind_group(
                        1,
                        &self
                            .managed_textures
                            .get(*texture)
                            .expect("failed to find texture")
                            .bindgroup,
                        &[],
                    );
                    let ib_start = ib_bounds[0] as u32;
                    let ib_end = ib_bounds[1] as u32;
                    let base_vertex = vb_bounds[0] as i32;
                    render_pass.draw_indexed(ib_start..ib_end, base_vertex, 0..1);
                }
                DrawCallInfo::Callback(_) => todo!(),
            }
        }
    }
    pub fn update_data(
        &mut self,
        renderer: &Renderer,
        clipped_meshes: Vec<ClippedPrimitive>,
        screen_size: Option<[f32; 2]>,
        screen_size_physical: [u32; 2],
        scale: f32,
        textures_delta: TexturesDelta,
    ) {
        // clear textures from previous frame
        for tex_id in self.textures_to_clear.drain(..) {
            match tex_id {
                TextureId::Managed(m) => {
                    self.managed_textures.remove(m);
                }
                TextureId::User(_) => todo!(),
            }
        }
        let dev = renderer.device.clone();
        let queue = renderer.queue.clone();
        // update screensize if there's been a resize (current size of the surface texture)
        if let Some(screen_size) = screen_size {
            queue.write_buffer(&self.screen_size_ub, 0, bytemuck::cast_slice(&screen_size));
        }
        self.deal_with_textures_delta(&dev, &queue, textures_delta);
        self.draw_calls.clear();
        // count total vb and ib length required
        let mut total_vb_len = 0;
        let mut total_ib_len = 0;
        for cp in &clipped_meshes {
            match cp.primitive {
                egui::epaint::Primitive::Mesh(ref m) => {
                    total_ib_len += m.indices.len();
                    total_vb_len += m.vertices.len();
                }
                egui::epaint::Primitive::Callback(_) => todo!(),
            }
        }
        // if total size doesn't fit, create new buffers with enough size
        if self.vb_len < total_vb_len {
            self.vb = dev.create_buffer(&BufferDescriptor {
                label: Some("egui vertex buffer"),
                size: total_vb_len as u64 * 20,
                usage: BufferUsages::COPY_DST | BufferUsages::VERTEX,
                mapped_at_creation: false,
            });
            self.vb_len = total_vb_len;
        }
        if self.ib_len < total_ib_len {
            self.ib = dev.create_buffer(&BufferDescriptor {
                label: Some("egui index buffer"),
                size: total_ib_len as u64 * 4,
                usage: BufferUsages::COPY_DST | BufferUsages::INDEX,
                mapped_at_creation: false,
            });
            self.ib_len = total_ib_len;
        }

        // these are starting bounds of buffer slices which will be used for each draw call
        let mut vb_offset = 0;
        let mut ib_offset = 0;

        // update the buffers with data and create draw calls
        self.draw_calls = clipped_meshes
            .into_iter()
            .map(|mesh| match mesh.primitive {
                egui::epaint::Primitive::Mesh(ref m) => {
                    // current sizes
                    let vlen = m.vertices.len();
                    let ilen = m.indices.len();
                    // range of buffer slice which will be used for this draw call
                    let vb_bounds = [vb_offset, vb_offset + vlen];
                    let ib_bounds = [ib_offset, ib_offset + ilen];
                    // write to buffers the relevant data
                    queue.write_buffer(
                        &self.vb,
                        (vb_offset * 20).try_into().unwrap(),
                        cast_slice(&m.vertices),
                    );
                    queue.write_buffer(
                        &self.ib,
                        (ib_offset * 4).try_into().unwrap(),
                        cast_slice(&m.indices),
                    );
                    // bump the offsets so that next mesh can use these as starting bounds
                    ib_offset += ilen;
                    vb_offset += vlen;

                    // idk what these rects are doing, but whatever..
                    let clip_rect = mesh.clip_rect;
                    let clip_min_x = scale * clip_rect.min.x;
                    let clip_min_y = scale * clip_rect.min.y;
                    let clip_max_x = scale * clip_rect.max.x;
                    let clip_max_y = scale * clip_rect.max.y;

                    // Make sure clip rect can fit within an `u32`.
                    let clip_min_x = clip_min_x.clamp(0.0, screen_size_physical[0] as f32);
                    let clip_min_y = clip_min_y.clamp(0.0, screen_size_physical[1] as f32);
                    let clip_max_x = clip_max_x.clamp(clip_min_x, screen_size_physical[0] as f32);
                    let clip_max_y = clip_max_y.clamp(clip_min_y, screen_size_physical[1] as f32);

                    let clip_min_x = clip_min_x.round() as u32;
                    let clip_min_y = clip_min_y.round() as u32;
                    let clip_max_x = clip_max_x.round() as u32;
                    let clip_max_y = clip_max_y.round() as u32;

                    let width = (clip_max_x - clip_min_x).max(1);
                    let height = (clip_max_y - clip_min_y).max(1);

                    // Clip scissor rectangle to target size.
                    let x = clip_min_x.min(screen_size_physical[0]);
                    let y = clip_min_y.min(screen_size_physical[1]);
                    let width = width.min(screen_size_physical[0] - x);
                    let height = height.min(screen_size_physical[1] - y);

                    let texture = match m.texture_id {
                        TextureId::Managed(m) => m,
                        TextureId::User(_) => todo!(),
                    };

                    DrawCallInfo::Mesh {
                        clip_rect: [x, y, width, height],
                        texture,
                        vb_bounds,
                        ib_bounds,
                    }
                }
                egui::epaint::Primitive::Callback(c) => DrawCallInfo::Callback(c),
            })
            .collect();
    }
    pub fn deal_with_textures_delta(
        &mut self,
        dev: &Device,
        queue: &Queue,
        mut textures_delta: TexturesDelta,
    ) {
        self.textures_to_clear.append(&mut textures_delta.free);
        // create and set textures
        for (tex_id, new_texture_data) in textures_delta.set {
            let key = match tex_id {
                TextureId::Managed(m) => m,
                TextureId::User(_) => unreachable!("egui only sends Managed textures."),
            };
            let pos = new_texture_data.pos;
            let data = new_texture_data.image;
            let (pixels, texture_label, size, mipmap_levels) = match &data {
                egui::ImageData::Color(_) => todo!(),
                egui::ImageData::Font(font_image) => {
                    let pixels: Vec<u8> = font_image
                        .srgba_pixels(1.0)
                        .flat_map(|c| c.to_array())
                        .collect();
                    (pixels, "egui font texture", font_image.size, 1)
                }
            };
            // create texture
            if pos.is_none() {
                let texture = dev.create_texture(&TextureDescriptor {
                    label: Some(texture_label),
                    size: Extent3d {
                        width: size[0]
                            .try_into()
                            .expect("failed to fit texture width into u32"),
                        height: size[1]
                            .try_into()
                            .expect("failed ot fit texture height into u32"),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: mipmap_levels,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba8UnormSrgb,
                    usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                });
                let view = texture.create_view(&TextureViewDescriptor {
                    label: Some(&format!("{} view", texture_label)),
                    format: Some(TextureFormat::Rgba8UnormSrgb),
                    dimension: Some(TextureViewDimension::D2),
                    aspect: TextureAspect::default(),
                    base_mip_level: 0,
                    mip_level_count: Some(
                        NonZeroU32::try_from(mipmap_levels)
                            .expect("mip mpa levle count won't fit in u32"),
                    ),
                    base_array_layer: 0,
                    array_layer_count: Some(
                        NonZeroU32::try_from(1).expect("array layer count not non-zero-u32"),
                    ),
                });
                let sampler = match new_texture_data.filter {
                    egui::TextureFilter::Nearest => &self.nearest_sampler,
                    egui::TextureFilter::Linear => &self.linear_sampler,
                };
                let bindgroup = dev.create_bind_group(&BindGroupDescriptor {
                    label: Some(&format!("{texture_label} bindgroup")),
                    layout: &self.texture_layout,
                    entries: &[
                        BindGroupEntry {
                            binding: 0,
                            resource: BindingResource::Sampler(sampler),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: BindingResource::TextureView(&view),
                        },
                    ],
                });
                self.managed_textures.insert(
                    key,
                    EguiManagedTexture {
                        texture,
                        view,
                        bindgroup,
                    },
                );
            }
            let t = &self
                .managed_textures
                .get(key)
                .as_ref()
                .expect("failed to get managed texture")
                .texture;
            queue.write_texture(
                ImageCopyTexture {
                    texture: t,
                    mip_level: 0,
                    origin: Origin3d::default(),
                    aspect: TextureAspect::All,
                },
                &pixels,
                ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        NonZeroU32::new(size[0] as u32 * 4).expect("texture bytes per row is zero"),
                    ),
                    rows_per_image: Some(
                        NonZeroU32::new(size[1] as u32).expect("texture rows count is zero"),
                    ),
                },
                Extent3d {
                    width: size[0] as u32,
                    height: size[1] as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    pub fn new(dev: &Device, surface_format: TextureFormat) -> Self {
        let shader_module = dev.create_shader_module(include_wgsl!("egui.wgsl"));
        let screen_size_ub = dev.create_buffer(&BufferDescriptor {
            label: Some("screen size uniform buffer"),
            size: 8,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
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
        let screen_size_bind_group_layout =
            dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("egui screen size bindgroup layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            NonZeroU64::new(8)
                                .expect("screen size uniform buffer MUST BE 8 bytes in size"),
                        ),
                    },
                    count: None,
                }],
            });
        let texture_bindgroup_layout = dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("egui texture bind group layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let screen_size_bindgroup = dev.create_bind_group(&BindGroupDescriptor {
            label: Some("egui bindgroup"),
            layout: &screen_size_bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &screen_size_ub,
                    offset: 0,
                    size: None,
                }),
            }],
        });
        let egui_pipeline_layout = dev.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("egui pipeline layout"),
            bind_group_layouts: &[&screen_size_bind_group_layout, &texture_bindgroup_layout],
            push_constant_ranges: &[],
        });
        let egui_pipeline = dev.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("egui pipeline"),
            layout: Some(&egui_pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                entry_point: "vs_main",
                buffers: &[VertexBufferLayout {
                    array_stride: 20,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: VertexFormat::Unorm8x4,
                            offset: 16,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });
        let linear_sampler = dev.create_sampler(&SamplerDescriptor {
            label: Some("linear sampler"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            ..Default::default()
        });
        let nearest_sampler = dev.create_sampler(&SamplerDescriptor {
            label: Some("nearest sampler"),
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            vb,
            vb_len: 0,
            ib,
            ib_len: 0,
            draw_calls: Vec::new(),
            managed_textures: IntMap::new(),
            linear_sampler,
            nearest_sampler,
            screen_size_ub,
            texture_layout: texture_bindgroup_layout,
            screen_size_bindgroup,
            pipeline: egui_pipeline,
            textures_to_clear: Vec::new(),
        }
    }
}
