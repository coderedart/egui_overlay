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

// Converts a color from sRGB gamma to linear light gamma
vec4 to_linear(vec4 sRGB)
{
    bvec3 cutoff = lessThan(sRGB.rgb, vec3(0.04045));
    vec3 higher = pow((sRGB.rgb + vec3(0.055))/vec3(1.055), vec3(2.4));
    vec3 lower = sRGB.rgb/vec3(12.92);

    return vec4(mix(higher, lower, cutoff), sRGB.a);
}

void main() {
    // we output in linear. so that gpu can convert it into srgb for srgb framebuffers
    fout_color = to_linear(vout_sc * from_linear(texture(u_sampler, vout_tc)));
}


