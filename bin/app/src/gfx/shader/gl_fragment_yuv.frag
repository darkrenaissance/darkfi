#version 100
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
}