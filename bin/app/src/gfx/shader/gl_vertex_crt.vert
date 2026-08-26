#version 100
attribute vec2 in_pos;
attribute vec4 in_color;
attribute vec2 in_uv;

varying vec2 uv;
varying vec2 resolution;

uniform mat4 Projection;
uniform mat4 Model;

void main() {
    gl_Position = Projection * Model * vec4(in_pos, 0, 1);
    uv = in_uv;
    resolution = in_color.xy;
}
