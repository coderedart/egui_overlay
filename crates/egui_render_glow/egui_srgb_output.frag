#version 300 es

precision highp float;
// fragment shader uniforms. texture and sampler
uniform sampler2D u_sampler;

// fragment inputs
in vec2 vout_tc;
in vec4 vout_sc; // srgb color

out vec4 fout_color;


// Converts a color from linear light gamma to sRGB gamma
vec4 from_linear(vec4 linearRGB)
{
    bvec3 cutoff = lessThan(linearRGB.rgb, vec3(0.0031308));
    vec3 higher = vec3(1.055)*pow(linearRGB.rgb, vec3(1.0/2.4)) - vec3(0.055);
    vec3 lower = linearRGB.rgb * vec3(12.92);

    return vec4(mix(higher, lower, cutoff), linearRGB.a);
}

void main() {
    // we output in srgb space to write to linear framebuffers without gpu doing any conversion
    fout_color = vout_sc * from_linear(texture(u_sampler, vout_tc));
}