import { expect, test } from "bun:test";
import { createWebGLContext, translateShader } from "./webgl.js";

// A stand-in <canvas>: what a context reads of its element.
const canvas = (width, height) => ({ width, height, painted: 0, dom: { paint: (n) => n.painted++ } });

function program(gl, vs, fs) {
  const p = gl.createProgram();
  for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]]) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    gl.attachShader(p, s);
  }
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  gl.useProgram(p);
  return p;
}

// A triangle over the left half of clip space: x from -1 to 0.
function leftHalf(gl, p) {
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 0, -1, -1, 3, 0, 3, 0, -1, -1, 3]), gl.STATIC_DRAW);
  const at = gl.getAttribLocation(p, "position");
  gl.enableVertexAttribArray(at);
  gl.vertexAttribPointer(at, 2, gl.FLOAT, false, 0, 0);
}
const pixel = ({ width, data }, x, y) => [...data.subarray((y * width + x) * 4, (y * width + x) * 4 + 4)];

test("WebGL2 draws ESSL 3.00 into the canvas's buffer, read back top row first", () => {
  const el = canvas(4, 2);
  const gl = createWebGLContext(el, "webgl2", { antialias: false });
  expect(gl.getParameter(gl.VERSION)).toStartWith("WebGL 2.0");
  expect([...gl.getParameter(gl.VIEWPORT)]).toEqual([0, 0, 4, 2]);
  const p = program(
    gl,
    "#version 300 es\nin vec2 position; void main() { gl_Position = vec4(position, 0.0, 1.0); }",
    "#version 300 es\nprecision highp float; uniform vec4 color; out vec4 outColor; void main() { outColor = color; }",
  );
  leftHalf(gl, p);
  gl.uniform4f(gl.getUniformLocation(p, "color"), 1, 0, 0, 1);
  gl.clearColor(0, 0, 1, 1);
  gl.clear(gl.COLOR_BUFFER_BIT);
  gl.drawArrays(gl.TRIANGLES, 0, 6);
  expect(gl.getError()).toBe(gl.NO_ERROR);
  expect(el.painted).toBe(2);
  const shot = gl._snapshot();
  expect([pixel(shot, 0, 0), pixel(shot, 3, 1)]).toEqual([[255, 0, 0, 255], [0, 0, 255, 255]]);
  // Drawing into a framebuffer of its own isn't the canvas's.
  gl.bindFramebuffer(gl.FRAMEBUFFER, gl.createFramebuffer());
  gl.clear(gl.COLOR_BUFFER_BIT);
  expect(el.painted).toBe(2);
  expect(gl.getParameter(gl.FRAMEBUFFER_BINDING)).not.toBeNull();
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  expect(gl.getParameter(gl.FRAMEBUFFER_BINDING)).toBeNull();
});

test("WebGL1: ESSL 1.00, a vertex array object from its extension, and a flipped luminance texture", () => {
  const el = canvas(2, 2);
  const gl = createWebGLContext(el, "webgl", { antialias: true });
  expect(gl.getContextAttributes().antialias).toBe(true);
  const vao = gl.getExtension("OES_vertex_array_object");
  vao.bindVertexArrayOES(vao.createVertexArrayOES());
  const p = program(
    gl,
    "attribute vec2 position; varying vec2 uv; void main() { uv = position * 0.5 + 0.5; gl_Position = vec4(position, 0.0, 1.0); }",
    "precision mediump float; varying vec2 uv; uniform sampler2D tex; void main() { gl_FragColor = texture2D(tex, uv); }",
  );
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(gl.getAttribLocation(p, "position"), 2, gl.FLOAT, false, 0, 0);
  // Rows top first, as an image: flipped, the top row lands at the top of the texture (v = 1).
  gl.bindTexture(gl.TEXTURE_2D, gl.createTexture());
  gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);
  gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.LUMINANCE, 2, 2, 0, gl.LUMINANCE, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 0, 0]));
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  expect(gl.getError()).toBe(gl.NO_ERROR);
  const shot = gl._snapshot();
  expect([pixel(shot, 0, 0), pixel(shot, 0, 1)]).toEqual([[255, 255, 255, 255], [0, 0, 0, 255]]);
  // readPixels reads the multisampled buffer resolved, bottom row first as in GL.
  const out = new Uint8Array(4);
  gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, out);
  expect([...out]).toEqual([0, 0, 0, 255]);
  // Specified again as RGBA, it samples every channel: the luminance swizzle is gone.
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([10, 20, 30, 255]));
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  expect(pixel(gl._snapshot(), 0, 0)).toEqual([10, 20, 30, 255]);
  // Resizing the canvas resizes the drawing buffer.
  el.width = 3;
  gl._resizeDrawingBuffer();
  expect([gl.drawingBufferWidth, gl._snapshot().width]).toEqual([3, 3]);
});

test("ESSL 1.00 becomes GLSL 4.10 core", () => {
  const fs = translateShader("#extension GL_OES_standard_derivatives : enable\nprecision mediump float;\nvoid main() { gl_FragData[1] = texture2D(t, uv); gl_FragDepthEXT = 0.5; }", false);
  expect(fs).toStartWith("#version 410 core\n#define gpuijs_GL_ES 1\n#define gpuijs_GL_FRAGMENT_PRECISION_HIGH 1\n#define varying in\n");
  expect(fs).toContain("layout(location = 1) out vec4 gpuijs_FragData1;");
  expect(fs).toContain("gpuijs_FragData1 = texture2D(t, uv); gl_FragDepth = 0.5;");
  expect(fs).not.toContain("#extension");
  expect(translateShader("#version 300 es\nprecision highp sampler2DArray;\nout vec4 c;", false)).toBe("#version 410 core\n#define gpuijs_GL_ES 1\n#define gpuijs_GL_FRAGMENT_PRECISION_HIGH 1\n\n\nout vec4 c;");
});
