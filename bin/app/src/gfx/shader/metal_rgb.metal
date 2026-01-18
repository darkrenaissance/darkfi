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