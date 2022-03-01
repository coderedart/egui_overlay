use egui::epaint::Vertex;
use egui::{ClippedMesh, Color32, ImageData, TextureId};
use std::collections::HashMap;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    include_wgsl, AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, BlendComponent,
    BlendFactor, BlendOperation, BlendState, BufferAddress, BufferBinding, BufferBindingType,
    BufferDescriptor, BufferUsages, Color, ColorTargetState, ColorWrites, CommandEncoder,
    CommandEncoderDescriptor, Extent3d, FilterMode, FragmentState, FrontFace, ImageCopyTexture,
    ImageDataLayout, IndexFormat, LoadOp, Operations, Origin3d, PipelineLayoutDescriptor,
    PrimitiveState, PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, SamplerBindingType, SamplerDescriptor, ShaderStages,
    SurfaceConfiguration, SurfaceError, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension, VertexBufferLayout, VertexState, VertexStepMode,
};

use crate::{EguiState, GlfwWindow, WgpuRenderer};

impl WgpuRenderer {
    pub async fn new(window: &GlfwWindow) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::Backends::VULKAN);
        let surface = unsafe { instance.create_surface(&window.window) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| "failed to create adapter".to_string())?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: "overlay device".into(),
                    features: Default::default(),
                    limits: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("failed to create wgpu device due to : {:#?}", e))?;

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface
                .get_preferred_format(&adapter)
                .ok_or("surface has no preferred format".to_string())?,
            width: window.size_physical_pixels[0],
            height: window.size_physical_pixels[1],
            present_mode: wgpu::PresentMode::Fifo,
        };
        surface.configure(&device, &config);
        let egui_linear_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("egui linear sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: Default::default(),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });
        let egui_linear_bindgroup_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("egui linear bindgroup layout"),
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
                            sample_type: TextureSampleType::Float { filterable: true },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let egui_state = EguiState::new(&device, &egui_linear_bindgroup_layout, &config)?;
        Ok(Self {
            egui_state,
            textures: HashMap::new(),
            egui_linear_bindgroup_layout,
            egui_linear_sampler,
            framebuffer_and_view: None,
            surface,
            config,
            queue,
            device,
        })
    }
    pub fn pre_tick(&mut self, window: &GlfwWindow) -> Result<(), String> {
        if window.size_physical_pixels[0] != self.config.width
            || window.size_physical_pixels[1] != self.config.height
        {
            self.config.width = window.size_physical_pixels[0];
            self.config.height = window.size_physical_pixels[1];
            self.surface.configure(&self.device, &self.config);
        }
        // if we fail to get a framebuffer, we return. so, make sure to do any texture updates before this point
        match self.surface.get_current_texture() {
            Ok(fb) => {
                let fbv = fb.texture.create_view(&TextureViewDescriptor {
                    label: Some("frambuffer view"),
                    format: Option::from(self.config.format),
                    dimension: Some(TextureViewDimension::D2),
                    aspect: TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: 0,
                    array_layer_count: None,
                });

                self.framebuffer_and_view = Some((fb, fbv));
            }
            Err(e) => match e {
                SurfaceError::Outdated => {
                    self.surface.configure(&self.device, &self.config);
                    match self.surface.get_current_texture() {
                        Ok(fb) => {
                            let fbv = fb.texture.create_view(&TextureViewDescriptor {
                                label: Some("frambuffer view"),
                                format: Option::from(self.config.format),
                                dimension: Some(TextureViewDimension::D2),
                                aspect: TextureAspect::All,
                                base_mip_level: 0,
                                mip_level_count: None,
                                base_array_layer: 0,
                                array_layer_count: None,
                            });

                            self.framebuffer_and_view = Some((fb, fbv));
                        }
                        rest => return Err(format!("error even after configure: {:#?}", rest)),
                    }
                }
                rest => {
                    return Err(format!("surface error: {:#?}", rest));
                }
            },
        };
        Ok(())
    }
    pub fn tick(
        &mut self,
        textures_delta: egui::TexturesDelta,
        shapes: Vec<egui::ClippedMesh>,
        window: &GlfwWindow,
    ) -> Result<(), String> {
        let _tex_update = !textures_delta.set.is_empty() || !textures_delta.free.is_empty();

        for (id, delta) in textures_delta.set {
            let whole = delta.is_whole();
            let width = delta.image.width() as u32;
            let height = delta.image.height() as u32;
            let pixels: Vec<u8> = match delta.image {
                ImageData::Color(c) => c
                    .pixels
                    .into_iter()
                    .flat_map(|c32| c32.to_array())
                    .collect(),
                ImageData::Alpha(a) => a
                    .pixels
                    .into_iter()
                    .flat_map(|a8| Color32::from_white_alpha(a8).to_array())
                    .collect(),
            };
            let size = pixels.len() as u32;
            let position = [
                delta.pos.unwrap_or([0, 0])[0] as u32,
                delta.pos.unwrap_or([0, 0])[1] as u32,
            ];
            assert_eq!(size, width * height * 4);
            if whole {
                let format = TextureFormat::Rgba8UnormSrgb;
                let dimension = TextureDimension::D2;
                let mip_level_count = if id != TextureId::Managed(0) {
                    f32::floor(f32::log2(width.max(height) as f32)) as u32 + 1
                } else {
                    1
                };
                let new_texture = self.device.create_texture(&TextureDescriptor {
                    label: Some(&format!("{:#?}", id)),
                    size: Extent3d {
                        width: width as u32,
                        height: height as u32,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count,
                    sample_count: 1,
                    dimension,
                    format,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                });
                let view = new_texture.create_view(&TextureViewDescriptor {
                    label: Some(&format!("view {:#?}", id)),
                    format: Some(format),
                    dimension: Some(TextureViewDimension::D2),
                    aspect: TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: Some(
                        mip_level_count
                            .try_into()
                            .expect("failed to fit mip level count into nonzero"),
                    ),
                    base_array_layer: 0,
                    array_layer_count: Some(1.try_into().expect("failed to fit 1 into nonzero")),
                });
                let bindgroup = self.device.create_bind_group(&BindGroupDescriptor {
                    label: Some(&format!("bindgroup {:#?}", id)),
                    layout: &self.egui_linear_bindgroup_layout,
                    entries: &[
                        BindGroupEntry {
                            binding: 0,
                            resource: BindingResource::Sampler(&self.egui_linear_sampler),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: BindingResource::TextureView(&view),
                        },
                    ],
                });
                self.textures.insert(id, (new_texture, view, bindgroup));
            }
            if let Some((tex, _view, _bindgroup)) = self.textures.get(&id) {
                self.queue.write_texture(
                    ImageCopyTexture {
                        texture: tex,
                        mip_level: 0,
                        origin: Origin3d {
                            x: position[0],
                            y: position[1],
                            z: 0,
                        },
                        aspect: TextureAspect::All,
                    },
                    pixels.as_slice(),
                    ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(
                            (width as u32 * 4)
                                .try_into()
                                .expect("failed to fit image width into 4 byte layout"),
                        ),
                        rows_per_image: None,
                    },
                    Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                )
            }
        }
        if let Some((fb, fbv)) = self.framebuffer_and_view.take() {
            let mut encoder = self
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("egui command encoder"),
                });
            {
                self.egui_state.tick(
                    &fbv,
                    &mut encoder,
                    window,
                    &self.device,
                    shapes,
                    &self.textures,
                )?;
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            fb.present();
        }
        for id in textures_delta.free {
            self.textures.remove(&id);
        }

        Ok(())
    }
}

impl EguiState {
    pub fn new(
        device: &wgpu::Device,
        texture_bindgroup_layout: &BindGroupLayout,
        config: &SurfaceConfiguration,
    ) -> Result<Self, String> {
        let bindgroup_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("egui bind group layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("egui pipeline layout"),
            bind_group_layouts: &[&bindgroup_layout, texture_bindgroup_layout],
            push_constant_ranges: &[],
        });
        let shader_module = device.create_shader_module(&include_wgsl!("egui.wgsl"));
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x2,
            1 => Float32x2,
            2 => Unorm8x4,
        ];

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("egui render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                entry_point: "vs_main",
                buffers: &[VertexBufferLayout {
                    array_stride: 20,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &attributes,
                }],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: Default::default(),
                conservative: false,
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: "fs_main",
                targets: &[ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::OneMinusDstAlpha,
                            dst_factor: BlendFactor::One,
                            operation: BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                }],
            }),
            multiview: None,
        });
        Ok(Self {
            pipeline,
            pipeline_layout,
            bindgroup_layout,
            shader_module,
        })
    }

    pub fn tick(
        &mut self,
        fbv: &TextureView,
        encoder: &mut CommandEncoder,
        window: &GlfwWindow,
        device: &wgpu::Device,
        shapes: Vec<ClippedMesh>,
        textures: &HashMap<TextureId, (Texture, TextureView, BindGroup)>,
    ) -> Result<(), String> {
        let size_in_points: [f32; 2] = [
            window.size_physical_pixels[0] as f32 / window.scale[0],
            window.size_physical_pixels[1] as f32 / window.scale[1],
        ];

        let ub = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("egui uniform buffer"),
            contents: bytemuck::cast_slice(size_in_points.as_slice()),
            usage: BufferUsages::UNIFORM,
        });
        let ub_bindgroup = device.create_bind_group(&BindGroupDescriptor {
            label: Some("egui uniform bindgroup"),
            layout: &self.bindgroup_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &ub,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        let vb_size: usize = shapes.iter().map(|cm| cm.1.vertices.len()).sum::<usize>()
            * std::mem::size_of::<egui::epaint::Vertex>();
        let ib_size: usize = shapes.iter().map(|cm| cm.1.indices.len()).sum::<usize>() * 4;
        let vb = device.create_buffer(&BufferDescriptor {
            label: Some("egui vertex buffer"),
            size: vb_size as BufferAddress,
            usage: BufferUsages::VERTEX,
            mapped_at_creation: true,
        });
        let ib = device.create_buffer(&BufferDescriptor {
            label: Some("egui index buffer"),
            size: (ib_size) as BufferAddress,
            usage: BufferUsages::INDEX,
            mapped_at_creation: true,
        });
        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[RenderPassColorAttachment {
                    view: fbv,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &ub_bindgroup, &[]);
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), IndexFormat::Uint32);
            {
                let mut vb_view = vb.slice(..).get_mapped_range_mut();
                let mut ib_view = ib.slice(..).get_mapped_range_mut();
                let mut vb_offset: usize = 0;
                let mut ib_offset: usize = 0;

                for mesh in shapes {
                    let vb_len = mesh.1.vertices.len() * 20;
                    let ib_len = mesh.1.indices.len() * std::mem::size_of::<u32>();
                    vb_view[vb_offset..(vb_offset + vb_len)].copy_from_slice(
                        bytemuck::cast_slice::<Vertex, u8>(mesh.1.vertices.as_slice()),
                    );
                    ib_view[ib_offset..(ib_offset + ib_len)]
                        .copy_from_slice(bytemuck::cast_slice(mesh.1.indices.as_slice()));
                    render_pass.set_bind_group(
                        1,
                        &textures
                            .get(&mesh.1.texture_id)
                            .ok_or_else(|| "texture not found".to_string())?
                            .2,
                        &[],
                    );
                    let clip_rect = mesh.0;
                    // Transform clip rect to physical pixels:
                    let pixels_per_point = window.scale[0];
                    let clip_min_x = pixels_per_point * clip_rect.min.x;
                    let clip_min_y = pixels_per_point * clip_rect.min.y;
                    let clip_max_x = pixels_per_point * clip_rect.max.x;
                    let clip_max_y = pixels_per_point * clip_rect.max.y;

                    // // Make sure clip rect can fit within a `u32`:
                    // let clip_min_x = clip_min_x.clamp(0.0, wtx.config.width as f32);
                    // let clip_min_y = clip_min_y.clamp(0.0, wtx.config.height as f32);
                    // let clip_max_x = clip_max_x.clamp(clip_min_x, wtx.config.width as f32);
                    // let clip_max_y = clip_max_y.clamp(clip_min_y, wtx.config.height as f32);

                    // let clip_min_x = clip_min_x.round() as i32;
                    // let clip_min_y = clip_min_y.round() as i32;
                    // let clip_max_x = clip_max_x.round() as i32;
                    // let clip_max_y = clip_max_y.round() as i32;
                    // wgpu cannot handle zero sized scissor rectangles, so this workaround is necessary
                    // https://github.com/gfx-rs/wgpu/issues/1750
                    if (clip_max_y - clip_min_y) >= 1.0 && (clip_max_x - clip_min_x) >= 1.0 {
                        render_pass.set_scissor_rect(
                            clip_min_x as u32,
                            (clip_min_y) as u32,
                            (clip_max_x - clip_min_x) as u32,
                            (clip_max_y - clip_min_y) as u32,
                        );

                        render_pass.draw_indexed(
                            ((ib_offset / 4) as u32)
                                ..(((ib_offset / 4) + mesh.1.indices.len()) as u32),
                            (vb_offset / 20)
                                .try_into()
                                .expect("failed to use vb_offset for base_vertex properly"),
                            0..1,
                        );
                    }

                    vb_offset += vb_len;
                    ib_offset += ib_len;
                }
            }
            vb.unmap();
            ib.unmap();
        }
        Ok(())
    }
}
