#version 100
varying lowp vec4 color;
varying lowp vec2 uv;

uniform sampler2D tex;
uniform lowp float Alpha;

void main() {
    gl_FragColor = color * texture2D(tex, uv);
    gl_FragColor.a *= Alpha;
}