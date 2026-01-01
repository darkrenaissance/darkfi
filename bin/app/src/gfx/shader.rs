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

pub const GL_VERTEX: &str = r#"#version 100
attribute vec2 in_pos;
attribute vec4 in_color;
attribute vec2 in_uv;

varying lowp vec4 color;
varying lowp vec2 uv;

uniform mat4 Projection;
uniform mat4 Model;

void main() {
    gl_Position = Projection * Model * vec4(in_pos, 0, 1);
    color = in_color;
    uv = in_uv;
}"#;

pub const GL_FRAGMENT_RGB: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;

uniform sampler2D tex;

void main() {
    gl_FragColor = color * texture2D(tex, uv);
}"#;

pub const GL_FRAGMENT_YUV: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;

uniform sampler2D tex_y;
uniform sampler2D tex_u;
uniform sampler2D tex_v;

void main() {
    lowp float y = texture2D(tex_y, uv).r;
    lowp float u = texture2D(tex_u, uv).r - 0.5;
    lowp float v = texture2D(tex_v, uv).r - 0.5;

    // BT.601 YUV to RGB conversion
    lowp float r = y + 1.402 * v;
    lowp float g = y - 0.344 * u - 0.714 * v;
    lowp float b = y + 1.772 * u;

    gl_FragColor = color * vec4(r, g, b, 1.0);
}"#;

pub const METAL_RGB: &str = r#"
#include <metal_stdlib>

using namespace metal;

struct Uniforms
{
    float4x4 Projection;
    float4x4 Model;
};

struct Vertex
{
    float2 in_pos   [[attribute(0)]];
    float4 in_color [[attribute(1)]];
    float2 in_uv    [[attribute(2)]];
};

struct RasterizerData
{
    float4 position [[position]];
    float4 color [[user(locn0)]];
    float2 uv [[user(locn1)]];
};

vertex RasterizerData vertexShader(Vertex v [[stage_in]])
{
    RasterizerData out;

    out.position = uniforms.Model * uniforms.Projection * float4(v.in_pos.xy, 0.0, 1.0);
    out.color = v.in_color;
    out.uv = v.texcoord;

    return out
}

fragment float4 fragmentShader(RasterizerData in [[stage_in]], texture2d<float> tex [[texture(0)]], sampler texSmplr [[sampler(0)]])
{
    return in.color * tex.sample(texSmplr, in.uv)
}

"#;

pub const METAL_YUV: &str = r#"
#include <metal_stdlib>

using namespace metal;

struct Uniforms
{
    float4x4 Projection;
    float4x4 Model;
};

struct Vertex
{
    float2 in_pos   [[attribute(0)]];
    float4 in_color [[attribute(1)]];
    float2 in_uv    [[attribute(2)]];
};

struct RasterizerData
{
    float4 position [[position]];
    float4 color [[user(locn0)]];
    float2 uv [[user(locn1)]];
};

vertex RasterizerData vertexShader(Vertex v [[stage_in]])
{
    RasterizerData out;

    out.position = uniforms.Model * uniforms.Projection * float4(v.in_pos.xy, 0.0, 1.0);
    out.color = v.in_color;
    out.uv = v.texcoord;

    return out
}

fragment float4 fragmentShader(RasterizerData in [[stage_in]],
                                texture2d<float> tex_y [[texture(0)]],
                                texture2d<float> tex_u [[texture(1)]],
                                texture2d<float> tex_v [[texture(2)]],
                                sampler tex_y_smplr [[sampler(0)]],
                                sampler tex_u_smplr [[sampler(1)]],
                                sampler tex_v_smplr [[sampler(2)]])
{
    float y = tex_y.sample(tex_y_smplr, in.uv).r;
    float u = tex_u.sample(tex_u_smplr, in.uv).r - 0.5;
    float v = tex_v.sample(tex_v_smplr, in.uv).r - 0.5;

    // BT.601 YUV to RGB conversion
    float r = y + 1.402 * v;
    float g = y - 0.344 * u - 0.714 * v;
    float b = y + 1.772 * u;

    return in.color * float4(r, g, b, 1.0);
}

"#;

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
