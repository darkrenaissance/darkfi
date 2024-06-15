use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use freetype as ft;
use log::{debug, LevelFilter};
use miniquad::{
    conf, window, Backend, Bindings, BlendFactor, BlendState, BlendValue, BufferId, BufferLayout,
    BufferSource, BufferType, BufferUsage, Equation, EventHandler, KeyCode, KeyMods, MouseButton,
    PassAction, Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource, TextureId,
    UniformDesc, UniformType, VertexAttribute, VertexFormat,
};
use std::{
    array::IntoIter,
    collections::HashMap,
    fmt,
    io::Cursor,
    sync::{mpsc, Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use crate::{
    chatview, editbox,
    error::{Error, Result},
    expr::{SExprMachine, SExprVal},
    gfx::Rectangle,
    keysym::{KeyCodeAsStr, MouseButtonAsU8},
    prop::{Property, PropertySubType, PropertyType},
    pubsub::PublisherPtr,
    res::{ResourceId, ResourceManager},
    scene::{
        MethodResponseFn, Pimpl, SceneGraph, SceneGraphPtr, SceneNode, SceneNodeId, SceneNodeInfo,
        SceneNodeType,
    },
    shader,
};

#[derive(Debug, SerialEncodable, SerialDecodable)]
#[repr(C)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

pub type RenderApiPtr = Arc<RenderApi>;

pub struct RenderApi {
    method_req: mpsc::Sender<GraphicsMethod>,
}

impl RenderApi {
    pub fn new(method_req: mpsc::Sender<GraphicsMethod>) -> Arc<Self> {
        Arc::new(Self { method_req })
    }

    async fn new_texture(&self, width: u16, height: u16, data: Vec<u8>) -> Result<TextureId> {
        let (sendr, recvr) = async_channel::bounded(1);

        let method = GraphicsMethod::NewTexture((width, height, data, sendr));

        self.method_req.send(method).map_err(|_| Error::GfxWindowClosed)?;

        let texture_id = recvr.recv().await.map_err(|_| Error::GfxWindowClosed)?;
        Ok(texture_id)
    }

    fn delete_texture(&self, texture: TextureId) {
        let method = GraphicsMethod::DeleteTexture(texture);

        // Ignore any error
        let _ = self.method_req.send(method);
    }

    pub async fn new_vertex_buffer(&self, verts: Vec<Vertex>) -> Result<BufferId> {
        let (sendr, recvr) = async_channel::bounded(1);

        let method = GraphicsMethod::NewVertexBuffer((verts, sendr));

        self.method_req.send(method).map_err(|_| Error::GfxWindowClosed)?;

        let buffer = recvr.recv().await.map_err(|_| Error::GfxWindowClosed)?;
        Ok(buffer)
    }

    pub async fn new_index_buffer(&self, indices: Vec<u16>) -> Result<BufferId> {
        let (sendr, recvr) = async_channel::bounded(1);

        let method = GraphicsMethod::NewIndexBuffer((indices, sendr));

        self.method_req.send(method).map_err(|_| Error::GfxWindowClosed)?;

        let buffer = recvr.recv().await.map_err(|_| Error::GfxWindowClosed)?;
        Ok(buffer)
    }

    pub fn delete_buffer(&self, buffer: BufferId) {
        let method = GraphicsMethod::DeleteBuffer(buffer);

        // Ignore any error
        let _ = self.method_req.send(method);
    }

    pub async fn replace_draw_call(&self, loc: Vec<usize>, draw_call: DrawCall) {
        let method = GraphicsMethod::ReplaceDrawCall((loc, draw_call));

        // Ignore any error
        let _ = self.method_req.send(method);
    }
}

#[derive(Clone, Debug)]
pub struct DrawMesh {
    pub vertex_buffer: BufferId,
    pub index_buffer: BufferId,
    pub texture: Option<TextureId>,
    pub num_elements: i32,
}

#[derive(Debug)]
pub enum DrawInstruction {
    ApplyViewport(Rectangle<i32>),
    ApplyMatrix(glam::Mat4),
    Draw(DrawMesh),
}

#[derive(Debug)]
pub struct DrawCall {
    pub instrs: Vec<DrawInstruction>,
    pub dcs: Vec<DrawCall>,
}

struct RenderContext<'a> {
    ctx: &'a mut Box<dyn RenderingBackend>,
    root_dc: &'a DrawCall,
    uniforms_data: [u8; 128],
    white_texture: TextureId,
}

impl<'a> RenderContext<'a> {
    fn draw(&mut self) {
        self.draw_call(self.root_dc);
    }

    fn draw_call(&mut self, draw_call: &DrawCall) {
        for instr in &draw_call.instrs {
            match instr {
                DrawInstruction::ApplyViewport(view) => {
                    let (_, screen_height) = window::screen_size();

                    let mut view = view.clone();
                    view.y = screen_height as i32 - (view.y + view.h);

                    //debug!("apply_viewport({:?})", view);
                    self.ctx.apply_viewport(view.x, view.y, view.w, view.h);
                    self.ctx.apply_scissor_rect(view.x, view.y, view.w, view.h);
                }
                DrawInstruction::ApplyMatrix(model) => {
                    //debug!("apply_matrix({:?})", model);
                    let data: [u8; 64] = unsafe { std::mem::transmute_copy(model) };
                    self.uniforms_data[64..].copy_from_slice(&data);
                    self.ctx.apply_uniforms_from_bytes(
                        self.uniforms_data.as_ptr(),
                        self.uniforms_data.len(),
                    );
                }
                DrawInstruction::Draw(mesh) => {
                    //debug!("draw(mesh)");
                    let texture = match mesh.texture {
                        Some(texture) => texture,
                        None => self.white_texture,
                    };
                    let bindings = Bindings {
                        vertex_buffers: vec![mesh.vertex_buffer],
                        index_buffer: mesh.index_buffer,
                        images: vec![texture],
                    };
                    self.ctx.apply_bindings(&bindings);
                    self.ctx.draw(0, mesh.num_elements, 1);
                }
            }
        }

        for dc in &draw_call.dcs {
            self.draw_call(dc);
        }
    }
}

#[derive(Debug)]
pub enum GraphicsMethod {
    NewTexture((u16, u16, Vec<u8>, async_channel::Sender<TextureId>)),
    DeleteTexture(TextureId),
    NewVertexBuffer((Vec<Vertex>, async_channel::Sender<BufferId>)),
    NewIndexBuffer((Vec<u16>, async_channel::Sender<BufferId>)),
    DeleteBuffer(BufferId),
    ReplaceDrawCall((Vec<usize>, DrawCall)),
}

#[derive(Debug, Clone)]
pub enum GraphicsEvent {
    KeyDown((KeyCode, KeyMods, bool)),
    Resize((f32, f32)),
}

struct Stage {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    white_texture: TextureId,
    root_dc: DrawCall,
    last_draw_time: Option<Instant>,

    method_rep: mpsc::Receiver<GraphicsMethod>,
    event_pub: PublisherPtr<GraphicsEvent>,
}

impl Stage {
    pub fn new(
        method_rep: mpsc::Receiver<GraphicsMethod>,
        event_pub: PublisherPtr<GraphicsEvent>,
    ) -> Self {
        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        let white_texture = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);

        let mut shader_meta: ShaderMeta = shader::meta();
        shader_meta.uniforms.uniforms.push(UniformDesc::new("Projection", UniformType::Mat4));
        shader_meta.uniforms.uniforms.push(UniformDesc::new("Model", UniformType::Mat4));

        let shader = ctx
            .new_shader(
                match ctx.info().backend {
                    Backend::OpenGl => ShaderSource::Glsl {
                        vertex: shader::GL_VERTEX,
                        fragment: shader::GL_FRAGMENT,
                    },
                    Backend::Metal => ShaderSource::Msl { program: shader::METAL },
                },
                shader_meta,
            )
            .unwrap();

        let params = PipelineParams {
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            ..Default::default()
        };

        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_color", VertexFormat::Float4),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
            ],
            shader,
            params,
        );

        Stage {
            ctx,
            pipeline,
            white_texture,
            root_dc: DrawCall { instrs: vec![], dcs: vec![] },
            last_draw_time: None,
            method_rep,
            event_pub,
        }
    }

    fn method_new_texture(
        &mut self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        sendr: async_channel::Sender<TextureId>,
    ) {
        let texture = self.ctx.new_texture_from_rgba8(width, height, &data);
        sendr.try_send(texture).unwrap();
    }
    fn method_delete_texture(&mut self, texture: TextureId) {
        self.ctx.delete_texture(texture);
    }
    fn method_new_vertex_buffer(
        &mut self,
        verts: Vec<Vertex>,
        sendr: async_channel::Sender<BufferId>,
    ) {
        let buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&verts),
        );
        sendr.try_send(buffer).unwrap();
    }
    fn method_new_index_buffer(
        &mut self,
        indices: Vec<u16>,
        sendr: async_channel::Sender<BufferId>,
    ) {
        let buffer = self.ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );
        sendr.try_send(buffer).unwrap();
    }
    fn method_delete_buffer(&mut self, buffer: BufferId) {
        self.ctx.delete_buffer(buffer);
    }
    fn method_replace_draw_call(&mut self, mut loc: Vec<usize>, new_dc: DrawCall) {
        loc.reverse();
        let mut dc = &mut self.root_dc;

        while let Some(i) = loc.pop() {
            dc = &mut dc.dcs[i];
        }

        std::mem::replace(dc, new_dc);
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        if self.last_draw_time.is_none() {
            return
        }

        // Only allow 20 ms, process as much as we can during that time
        let elapsed_since_draw = self.last_draw_time.unwrap().elapsed();
        // We're long overdue a redraw. Exit for now
        if elapsed_since_draw > Duration::from_millis(20) {
            return
        }
        // The next redraw must happen 20ms since its last one.
        // Calculate how much time is remaining until then.
        let allowed_time = Duration::from_millis(20) - elapsed_since_draw;
        let deadline = Instant::now() + allowed_time;

        loop {
            let Ok(method) = self.method_rep.recv_deadline(deadline) else { break };
            match method {
                GraphicsMethod::NewTexture((width, height, data, sendr)) => {
                    self.method_new_texture(width, height, data, sendr)
                }
                GraphicsMethod::DeleteTexture(texture) => self.method_delete_texture(texture),
                GraphicsMethod::NewVertexBuffer((verts, sendr)) => {
                    self.method_new_vertex_buffer(verts, sendr)
                }
                GraphicsMethod::NewIndexBuffer((indices, sendr)) => {
                    self.method_new_index_buffer(indices, sendr)
                }
                GraphicsMethod::DeleteBuffer(buffer) => self.method_delete_buffer(buffer),
                GraphicsMethod::ReplaceDrawCall((loc, dc)) => {
                    self.method_replace_draw_call(loc, dc)
                }
            };
        }
    }

    fn draw(&mut self) {
        self.last_draw_time = Some(Instant::now());

        self.ctx.begin_default_pass(PassAction::Nothing);
        self.ctx.apply_pipeline(&self.pipeline);

        // This will make the top left (0, 0) and the bottom right (1, 1)
        // Default is (-1, 1) -> (1, -1)
        let proj = glam::Mat4::from_translation(glam::Vec3::new(-1., 1., 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(2., -2., 1.));

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        //let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        //uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        let mut render_ctx = RenderContext {
            ctx: &mut self.ctx,
            root_dc: &self.root_dc,
            uniforms_data,
            white_texture: self.white_texture,
        };
        render_ctx.draw();

        self.ctx.commit_frame();
    }

    fn key_down_event(&mut self, keycode: KeyCode, mods: KeyMods, repeat: bool) {
        let event = GraphicsEvent::KeyDown((keycode, mods, repeat));
        self.event_pub.notify(event);
    }
    fn resize_event(&mut self, width: f32, height: f32) {
        let event = GraphicsEvent::Resize((width, height));
        self.event_pub.notify(event);
    }
}

pub fn run_gui(method_rep: mpsc::Receiver<GraphicsMethod>, event_pub: PublisherPtr<GraphicsEvent>) {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default().with_max_level(LevelFilter::Debug).with_tag("darkfi"),
        );
    }

    #[cfg(target_os = "linux")]
    {
        let term_logger = simplelog::TermLogger::new(
            simplelog::LevelFilter::Debug,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        );
        simplelog::CombinedLogger::init(vec![term_logger]).expect("logger");
    }

    let mut conf = miniquad::conf::Conf {
        high_dpi: true,
        window_resizable: true,
        platform: miniquad::conf::Platform {
            linux_backend: miniquad::conf::LinuxBackend::WaylandWithX11Fallback,
            wayland_use_fallback_decorations: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let metal = std::env::args().nth(1).as_deref() == Some("metal");
    conf.platform.apple_gfx_api =
        if metal { conf::AppleGfxApi::Metal } else { conf::AppleGfxApi::OpenGl };

    miniquad::start(conf, || Box::new(Stage::new(method_rep, event_pub)));
}
