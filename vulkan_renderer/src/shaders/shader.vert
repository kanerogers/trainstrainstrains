#version 450

layout(location = 0) in vec4 in_position;
layout(location = 1) in vec4 in_normal;
layout(location = 2) in vec2 in_uv;

layout(location = 0) out vec4 out_normal;
layout(location = 1) out vec4 out_pos;
layout(location = 2) out vec2 out_uv;

#include "push_constant.glsl"

void main() {
    gl_Position = mvp * in_position;
    out_pos = gl_Position;
    out_normal = in_normal;
    out_uv = in_uv;
}