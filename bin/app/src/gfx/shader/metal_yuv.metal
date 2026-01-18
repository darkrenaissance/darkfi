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