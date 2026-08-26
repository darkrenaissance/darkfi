#include <metal_stdlib>

using namespace metal;

struct Uniforms
{
    float4x4 Projection;
    float4x4 Model;
    float chromatic_aberration;
    float blur_amount;
    float blur_radius;
    float glow_intensity;
    float brightness;
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
    float2 uv [[user(locn0)]];
    float2 resolution [[user(locn1)]];
};

vertex RasterizerData vertexShader(Vertex v [[stage_in]], constant Uniforms& uniforms [[buffer(0)]])
{
    RasterizerData out;

    out.position = uniforms.Projection * uniforms.Model * float4(v.in_pos, 0.0, 1.0);
    out.uv = v.in_uv;
    out.resolution = v.in_color.xy;

    return out;
}

constant float GLOW_THRESHOLD = 0.55;

float3 sample_scene(texture2d<float> tex, sampler texSmplr, float2 coord, float chromatic_aberration)
{
    float2 dir = coord - 0.5;
    float r = tex.sample(texSmplr, coord + dir * chromatic_aberration).r;
    float g = tex.sample(texSmplr, coord).g;
    float b = tex.sample(texSmplr, coord - dir * chromatic_aberration).b;
    return float3(r, g, b);
}

// Soft blur plus extra glow on highlights, sharing the same taps
float3 blur_sample(
    texture2d<float> tex,
    sampler texSmplr,
    float2 coord,
    float2 texel,
    float blur_radius,
    float glow_intensity,
    thread float3& highlights
)
{
    float3 center = tex.sample(texSmplr, coord).rgb;
    float3 sum = center * 4.0;
    float3 bright = max(center - GLOW_THRESHOLD, 0.0) * 4.0;
    for (int i = 0; i < 8; i++) {
        float a = float(i) * 0.78539816;
        float2 d = float2(cos(a), sin(a));
        float3 inner = tex.sample(texSmplr, coord + d * texel * blur_radius).rgb;
        float3 outer = tex.sample(texSmplr, coord + d * texel * blur_radius * 2.4).rgb;
        sum += inner + outer;
        bright += max(inner - GLOW_THRESHOLD, 0.0);
        bright += max(outer - GLOW_THRESHOLD, 0.0);
    }
    highlights = bright * (glow_intensity / 20.0);
    return sum / 20.0;
}

fragment float4 fragmentShader(
    RasterizerData in [[stage_in]],
    texture2d<float> tex [[texture(0)]],
    sampler texSmplr [[sampler(0)]],
    constant Uniforms& uniforms [[buffer(0)]]
)
{
    float2 coord = in.uv;

    float2 texel = 1.0 / in.resolution;

    float3 color = sample_scene(tex, texSmplr, coord, uniforms.chromatic_aberration);
    float3 highlights;
    float3 blurred = blur_sample(
        tex, texSmplr, coord, texel,
        uniforms.blur_radius, uniforms.glow_intensity, highlights
    );
    color = mix(color, blurred, uniforms.blur_amount) + highlights;

    return float4(color * uniforms.brightness, 1.0);
}
