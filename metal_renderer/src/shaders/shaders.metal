#include <metal_stdlib>

using namespace metal;

typedef struct {
	float4 position;
	float4 normal;
    float2 uv;
} vertex_t;

struct ColorInOut {
    float4 position [[position]];
    float4 color;
    float2 uv;
};

typedef struct {
    float4x4 mvp;
    float4 colour;
} Uniforms;

// vertex shader function
vertex ColorInOut triangle_vertex(const device vertex_t* vertex_array [[ buffer(0) ]],
                                  const device Uniforms* uniforms [[ buffer(1) ]],
                                   uint vid [[ vertex_id ]],
                                   uint instance_id [[instance_id]])
{
    ColorInOut out;

    auto device const &v = vertex_array[vid];
    auto device const &u = uniforms[instance_id];
    out.position = u.mvp * v.position;
    out.color = u.colour;
    out.uv = v.uv;

    return out;
}

fragment float4 triangle_fragment(ColorInOut in [[stage_in]])
{
    return in.color;
};

struct YakuiInOut {
    float4 position [[position]];
    float4 color;
    float2 uv;
};

typedef struct {
    float2 position;
    float2 uv;
    float4 colour;
} YakuiVertex;

typedef struct {

} YakuiArgumentBuffer;


// vertex shader function
vertex YakuiInOut yakui_vertex(const device YakuiVertex* vertex_array [[ buffer(0) ]],
                               uint vid [[ vertex_id ]])
{
    YakuiInOut out;
    auto device const &v = vertex_array[vid];
    // Convert the co-ordinates from yakui coordinates to Metal:
    //
    // yakui: (0, 0) == top left, (1, 1) == bottom right
    // Metal: (-1, 1) === top left, (1, -1,) == bottom right
    float x = (v.position.x - 0.5) * 2.;
    float y = -(v.position.y - 0.5) * 2.;

    out.position = float4(x, y, 0., 1.);
    out.color = v.colour;
    out.uv = v.uv;

    return out;
}

fragment float4 yakui_text_fragment(ColorInOut in [[stage_in]],
                                    texture2d<half> colorTexture [[ texture(0) ]])
{
    constexpr sampler textureSampler (mag_filter::linear,
                                      min_filter::linear);

    const float coverage = colorTexture.sample(textureSampler, in.uv).r;
    return in.color * coverage;
};

fragment float4 yakui_texture_fragment(ColorInOut in [[stage_in]],
                               texture2d<half> colorTexture [[ texture(0) ]])
{
    constexpr sampler textureSampler (mag_filter::linear,
                                      min_filter::linear);

    const float4 sample = float4(colorTexture.sample(textureSampler, in.uv));
    return in.color;
};
