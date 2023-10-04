#version 450

layout(location = 0) in vec4 in_position;
layout(location = 1) in vec4 in_colour;

layout(location = 0) out vec4 out_colour;

#include "push_constant.glsl"

void main() {
    gl_Position = mvp * in_position;
    out_colour = in_colour;
}