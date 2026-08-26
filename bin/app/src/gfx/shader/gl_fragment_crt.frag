#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

varying vec2 uv;
varying vec2 resolution;

uniform sampler2D tex;

// CRT effect parameters, driven from Rust
uniform float chromatic_aberration;
uniform float blur_amount;
uniform float blur_radius;
uniform float glow_intensity;
uniform float brightness;

const float GLOW_THRESHOLD = 0.55;

// Sample with chromatic aberration: RGB channels split towards the edges
vec3 sample_scene(vec2 coord) {
    vec2 dir = coord - 0.5;
    float r = texture2D(tex, coord + dir * chromatic_aberration).r;
    float g = texture2D(tex, coord).g;
    float b = texture2D(tex, coord - dir * chromatic_aberration).b;
    return vec3(r, g, b);
}

// Soft blur plus extra glow on highlights, sharing the same taps
vec3 blur_sample(vec2 coord, vec2 texel, out vec3 highlights) {
    vec3 sum = texture2D(tex, coord).rgb * 4.0;
    vec3 bright = max(texture2D(tex, coord).rgb - GLOW_THRESHOLD, 0.0) * 4.0;
    for (int i = 0; i < 8; i++) {
        float a = float(i) * 0.78539816;
        vec2 d = vec2(cos(a), sin(a));
        vec3 inner = texture2D(tex, coord + d * texel * blur_radius).rgb;
        vec3 outer = texture2D(tex, coord + d * texel * blur_radius * 2.4).rgb;
        sum += inner + outer;
        bright += max(inner - GLOW_THRESHOLD, 0.0);
        bright += max(outer - GLOW_THRESHOLD, 0.0);
    }
    highlights = bright * (glow_intensity / 20.0);
    return sum / 20.0;
}

void main() {
    vec2 coord = uv;

    vec2 texel = 1.0 / resolution;

    vec3 color = sample_scene(coord);
    vec3 highlights;
    vec3 blurred = blur_sample(coord, texel, highlights);
    color = mix(color, blurred, blur_amount) + highlights;

    gl_FragColor = vec4(color * brightness, 1.0);
}
