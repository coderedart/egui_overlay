#version 300 es


// vertex input
layout(location = 0) in vec2 vin_pos; // vertex in position
layout(location = 1) in vec2 vin_tc; // vertex in texture coordinates
layout(location = 2) in vec4 vin_sc; // vertex in normalized srgba color 

// vertex output
out vec2 vout_tc; // vertex out texture coordinates
out vec4 vout_sc; // srgb color

// vertex uniform
uniform vec2 u_screen_size; // in physical pixels


void main() {
    gl_Position = vec4(
                      2.0 * vin_pos.x / u_screen_size.x - 1.0,
                      1.0 - 2.0 * vin_pos.y / u_screen_size.y,
                      0.0,
                      1.0);
    vout_tc = vin_tc;
    // egui does everything in srgb space
    vout_sc = vin_sc;
    
}