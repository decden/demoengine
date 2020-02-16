#version 440

layout(location=0) out vec4 out_color;
layout(location=1) out vec4 out_normal;
layout(location=2) out float out_depth;

in vec2 v_uv;

uniform float u_OuterRadius;
uniform float u_InnerRadius;
uniform float u_GridSpheresRadius;
uniform float u_AspectRatio;

const float MAX_MARCHING_STEPS = 180;
const float EPSILON = 1e-3;

float sphere(vec3 p, vec3 c, float r) {
    return length(p - c) - r;
}

float cube(vec3 p, vec3 c, float r) {
    vec3 d = abs(p - c) - r;

    float insideDistance = min(max(d.x, max(d.y, d.z)), 0.0);
    float outsideDistance = length(max(d, 0.0));

    return insideDistance + outsideDistance;
}

float s(vec3 p) {
    float v = max(sphere(p, vec3(0,0,0), u_OuterRadius), -cube(p, vec3(0,0,-0.5), u_InnerRadius));

    for (int x = 0; x < 3; ++x)
        for (int y = 0; y < 3; ++y)
            for (int z = 0; z < 3; ++z)
                v = min(v, sphere(p, vec3(x-1, y-1, z-1) * 0.6, u_GridSpheresRadius));

    return v;
}

vec3 ds(vec3 p) {
    const float dx = 0.00001;
    return vec3(
        (s(p + vec3(dx,0,0)) - s(p - vec3(dx,0,0))) / dx,
        (s(p + vec3(0,dx,0)) - s(p - vec3(0,dx,0))) / dx,
        (s(p + vec3(0,0,dx)) - s(p - vec3(0,0,dx))) / dx
    );
}

float trace(float start, float end, vec3 eye, vec3 viewRayDirection) {
    float depth = start;
    for (int i = 0; i < MAX_MARCHING_STEPS; i++) {
        float dist = s(eye + depth * viewRayDirection);
        if (dist < EPSILON) {
            return depth;
        }

        depth += dist*1;

        if (depth >= end) {
            return end;
        }
    }
    return end;
}

vec3 shade(vec3 pos, vec3 normal) {
    float lighting = max(0, -dot(normal, normalize(vec3(4, 0, 10)))) * 0.96 + 0.04;

    vec3 p = abs(sin(pos * 100));
    float fac = pow(p.x * p.y * p.z, 15.0);

    vec3 albedo = pos * 1.6 + vec3(1);
    return albedo * lighting.xxx * (fac * 0.4 + 0.6);
}

void main() {
    vec3 eye = vec3(0,0,-2);
    vec3 viewRayDirection = normalize(vec3((v_uv.x - 0.5) * u_AspectRatio, v_uv.y - 0.5, 1));
    float start = 0;
    float end = 3;

    float depth = trace(start, end, eye, viewRayDirection);
    float norm_depth = depth / (end - start);
    vec3 norm = normalize(ds(eye + depth * viewRayDirection));

    if (depth >= end)
        discard;

    out_color.rgb = shade(viewRayDirection * depth, norm);
    out_normal.rgb = norm * 0.5 + 0.5;
    out_depth = norm_depth;
}