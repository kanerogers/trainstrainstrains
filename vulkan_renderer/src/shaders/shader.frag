#version 450
#define NO_TEXTURE 4294967295
#define WORKFLOW_MAIN 0
#define WORKFLOW_TEXT 1
#define sunlightColor vec3(1.0, 1.0, 0.9)
#define moonlightColor vec3(0.2, 0.2, 0.5)

layout (location = 0) in vec4 in_normal;
layout (location = 1) in vec4 in_pos;
layout (location = 2) in vec2 in_uv;
layout (location = 0) out vec4 out_colour;

layout(set = 0, binding = 0) uniform sampler2D textures[1000];


#include "push_constant.glsl"



// Narkowicz 2015, "ACES Filmic Tone Mapping Curve"
vec3 aces(vec3 x) {
  const float a = 2.51;
  const float b = 0.03;
  const float c = 2.43;
  const float d = 0.59;
  const float e = 0.14;
  return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

vec3 uncharted2Tonemap(vec3 x) {
  float A = 0.15;
  float B = 0.50;
  float C = 0.10;
  float D = 0.20;
  float E = 0.02;
  float F = 0.30;
  float W = 11.2;
  return ((x * (A * x + C * B) + D * E) / (x * (A * x + B) + D * F)) - E / F;
}

vec3 filmic(vec3 x) {
  vec3 X = max(vec3(0.0), x - 0.004);
  vec3 result = (X * (6.2 * X + 0.5)) / (X * (6.2 * X + 1.7) + 0.06);
  return pow(result, vec3(2.2));
}

vec3 blinn_phong(vec3 baseColour) {
    vec3 normal = normalize(in_normal.xyz);
    vec3 lightDir = normalize(vec3(0.5, time_of_day, -0.5)); // Simple daylight direction
    vec3 viewDir = normalize(in_pos.xyz - view_pos.xyz);

    // Diffuse calculation
    float diff = max(dot(normal, lightDir), 0.0);

    // Specular calculation
    vec3 halfwayDir = normalize(lightDir + viewDir);
    float spec = pow(max(dot(normal, halfwayDir), 0.0), 32.0);

    vec3 currentLightColor = mix(moonlightColor, sunlightColor, time_of_day);

    // Combine
    vec3 ambient = 0.6 * baseColour.xyz * currentLightColor;
    vec3 diffuse = diff * baseColour.xyz * currentLightColor;
    vec3 specular = spec * vec3(1.0, 1.0, 1.0); // white highlight

    vec3 finalColor = ambient + diffuse + (specular * 0.5);

    return finalColor;
}



void main() {
    vec4 base_colour;
    if (base_colour_texture_id == NO_TEXTURE) {
        base_colour = base_colour_factor;
    } else {
        base_colour = base_colour_factor * texture(textures[base_colour_texture_id], in_uv);
    } 
    
    vec3 shaded = blinn_phong(base_colour.rgb);
    out_colour = vec4(aces(shaded), base_colour.a);
}