#version 440

in vec2 v_uv;

layout(binding=0) uniform sampler2D TexColor;
layout(binding=1) uniform sampler2D TexColor2;
uniform float u_Time;
uniform float u_AspectRatio;

layout(location=0) out vec4 out_color;


void main() {
  vec2 uv = v_uv.x * vec2(1.0 / u_AspectRatio, 1);
  vec2 displacement = vec2(sin((uv.x - 0.5) * 30 + u_Time * 3), cos((uv.y - 0.5) * 30 + u_Time * 2.1341)) * 0.005;
  displacement *= vec2(u_AspectRatio, 1);
  out_color = texture(TexColor, v_uv + displacement);
}