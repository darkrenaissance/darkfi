/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use miniquad::*;

pub const GL_VERTEX: &str = include_str!("gl_vertex.vert");
pub const GL_VERTEX_CRT: &str = include_str!("gl_vertex_crt.vert");
pub const GL_FRAGMENT_RGB: &str = include_str!("gl_fragment_rgb.frag");
pub const GL_FRAGMENT_YUV: &str = include_str!("gl_fragment_yuv.frag");
pub const GL_FRAGMENT_CRT: &str = include_str!("gl_fragment_crt.frag");
pub const METAL_RGB: &str = include_str!("metal_rgb.metal");
pub const METAL_YUV: &str = include_str!("metal_yuv.metal");
pub const METAL_CRT: &str = include_str!("metal_crt.metal");

pub fn meta_rgb() -> ShaderMeta {
    ShaderMeta {
        images: vec!["tex".to_string()],
        uniforms: UniformBlockLayout { uniforms: vec![] },
    }
}

pub fn meta_yuv() -> ShaderMeta {
    ShaderMeta {
        images: vec!["tex_y".to_string(), "tex_u".to_string(), "tex_v".to_string()],
        uniforms: UniformBlockLayout { uniforms: vec![] },
    }
}

pub fn create_rgb_pipeline(ctx: &mut Box<dyn RenderingBackend>) -> Pipeline {
    let shader_meta = meta_rgb();

    let shader_source = match ctx.info().backend {
        Backend::OpenGl => ShaderSource::Glsl { vertex: GL_VERTEX, fragment: GL_FRAGMENT_RGB },
        Backend::Metal => ShaderSource::Msl { program: METAL_RGB },
    };

    let params = PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    };

    create_pipeline_with_meta(ctx, shader_source, shader_meta, params)
}

pub fn create_yuv_pipeline(ctx: &mut Box<dyn RenderingBackend>) -> Pipeline {
    let shader_meta = meta_yuv();

    let shader_source = match ctx.info().backend {
        Backend::OpenGl => ShaderSource::Glsl { vertex: GL_VERTEX, fragment: GL_FRAGMENT_YUV },
        Backend::Metal => ShaderSource::Msl { program: METAL_YUV },
    };

    let params = PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    };

    create_pipeline_with_meta(ctx, shader_source, shader_meta, params)
}

pub fn create_crt_pipeline(ctx: &mut Box<dyn RenderingBackend>) -> Pipeline {
    let shader_meta = ShaderMeta {
        images: vec!["tex".to_string()],
        uniforms: UniformBlockLayout {
            uniforms: vec![
                UniformDesc::new("Projection", UniformType::Mat4),
                UniformDesc::new("Model", UniformType::Mat4),
                UniformDesc::new("chromatic_aberration", UniformType::Float1),
                UniformDesc::new("blur_amount", UniformType::Float1),
                UniformDesc::new("blur_radius", UniformType::Float1),
                UniformDesc::new("glow_intensity", UniformType::Float1),
                UniformDesc::new("brightness", UniformType::Float1),
            ],
        },
    };

    let shader_source = match ctx.info().backend {
        Backend::OpenGl => ShaderSource::Glsl { vertex: GL_VERTEX_CRT, fragment: GL_FRAGMENT_CRT },
        Backend::Metal => ShaderSource::Msl { program: METAL_CRT },
    };

    let shader = ctx.new_shader(shader_source, shader_meta).unwrap();

    ctx.new_pipeline(
        &[BufferLayout::default()],
        &[
            VertexAttribute::new("in_pos", VertexFormat::Float2),
            VertexAttribute::new("in_color", VertexFormat::Float4),
            VertexAttribute::new("in_uv", VertexFormat::Float2),
        ],
        shader,
        PipelineParams::default(),
    )
}

fn create_pipeline_with_meta(
    ctx: &mut Box<dyn RenderingBackend>,
    shader_source: ShaderSource,
    mut shader_meta: ShaderMeta,
    params: PipelineParams,
) -> Pipeline {
    shader_meta.uniforms.uniforms.push(UniformDesc::new("Projection", UniformType::Mat4));
    shader_meta.uniforms.uniforms.push(UniformDesc::new("Model", UniformType::Mat4));
    shader_meta.uniforms.uniforms.push(UniformDesc::new("Alpha", UniformType::Float1));

    let shader = ctx.new_shader(shader_source, shader_meta).unwrap();

    ctx.new_pipeline(
        &[BufferLayout::default()],
        &[
            VertexAttribute::new("in_pos", VertexFormat::Float2),
            VertexAttribute::new("in_color", VertexFormat::Float4),
            VertexAttribute::new("in_uv", VertexFormat::Float2),
        ],
        shader,
        params,
    )
}
