mod renderer_backend;
mod window_backend;

use egui::{RawInput, TextureId};
use glfw::WindowEvent;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use wgpu::{
    BindGroup, BindGroupLayout, Device, Queue, Sampler, ShaderModule, SurfaceConfiguration,
    SurfaceTexture, Texture, TextureView,
};

pub struct GlfwWindow {
    pub glfw: glfw::Glfw,
    pub events_receiver: Receiver<(f64, WindowEvent)>,
    pub window: glfw::Window,
    pub size_physical_pixels: [u32; 2],
    pub scale: [f32; 2],
    pub cursor_pos_physical_pixels: [f32; 2],
    pub raw_input: RawInput,
    pub frame_events: Vec<WindowEvent>,
}
pub struct WgpuRenderer {
    pub egui_state: EguiState,
    pub textures: HashMap<TextureId, (Texture, TextureView, BindGroup)>,
    pub egui_linear_bindgroup_layout: BindGroupLayout,
    pub egui_linear_sampler: Sampler,
    pub framebuffer_and_view: Option<(SurfaceTexture, TextureView)>,
    pub surface: wgpu::Surface,
    pub config: SurfaceConfiguration,
    pub queue: Queue,
    pub device: Device,
}
pub struct EguiState {
    pub pipeline: wgpu::RenderPipeline,
    pub pipeline_layout: wgpu::PipelineLayout,
    pub bindgroup_layout: BindGroupLayout,
    pub shader_module: ShaderModule,
}
