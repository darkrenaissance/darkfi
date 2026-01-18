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
pub const GL_FRAGMENT_RGB: &str = include_str!("gl_fragment_rgb.frag");
pub const GL_FRAGMENT_YUV: &str = include_str!("gl_fragment_yuv.frag");
pub const METAL_RGB: &str = include_str!("metal_rgb.metal");
pub const METAL_YUV: &str = include_str!("metal_yuv.metal");

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

    create_pipeline_with_meta(ctx, shader_source, shader_meta)
}

pub fn create_yuv_pipeline(ctx: &mut Box<dyn RenderingBackend>) -> Pipeline {
    let shader_meta = meta_yuv();

    let shader_source = match ctx.info().backend {
        Backend::OpenGl => ShaderSource::Glsl { vertex: GL_VERTEX, fragment: GL_FRAGMENT_YUV },
        Backend::Metal => ShaderSource::Msl { program: METAL_YUV },
    };

    create_pipeline_with_meta(ctx, shader_source, shader_meta)
}

fn create_pipeline_with_meta(
    ctx: &mut Box<dyn RenderingBackend>,
    shader_source: ShaderSource,
    mut shader_meta: ShaderMeta,
) -> Pipeline {
    shader_meta.uniforms.uniforms.push(UniformDesc::new("Projection", UniformType::Mat4));
    shader_meta.uniforms.uniforms.push(UniformDesc::new("Model", UniformType::Mat4));

    let shader = ctx.new_shader(shader_source, shader_meta).unwrap();

    let params = PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    };

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
