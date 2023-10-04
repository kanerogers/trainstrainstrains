layout(push_constant) uniform push_constants {
    uint emissive_texture_id;
    uint metallic_roughness_ao_texture_id;
    uint normal_texture_id;
    uint base_colour_texture_id;
    vec4 base_colour_factor;
    vec4 view_pos;
    mat4 mvp;
    float time_of_day;
};