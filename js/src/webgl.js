// WebGL 1 and 2 for <canvas>, on macOS's OpenGL 4.1 core (Metal underneath) through bun:ffi.
// Each context draws into its own offscreen framebuffer, which stands for WebGL's default one;
// after a task that drew, the renderer reads it back and sends the host its pixels, as a 2D
// canvas's (`sendPixels` in renderer.js).
// shortcut: a full readback per drawn frame over the pipe; share an IOSurface with the host if big canvases are slow.
import { CString, FFIType, dlopen, ptr } from "bun:ffi";
import { readFileSync } from "node:fs";
import { skia } from "./media.js";
import { EXTENSIONS, WEBGL1, WEBGL2 } from "./webgl-constants.js";

const GL = { ...WEBGL1, ...WEBGL2 };
// Desktop GL's names for what WebGL doesn't spell (or spells for ES).
const RED = 0x1903;
const GREEN = 0x1904;
const BLUE = 0x1905;
const NUM_SAMPLE_COUNTS = 0x9380;
const RG = 0x8227;
const R8 = 0x8229;
const RG8 = 0x822b;
const HALF_FLOAT_OES = 0x8d61;
const PROGRAM_POINT_SIZE = 0x8642;
const TEXTURE_CUBE_MAP_SEAMLESS = 0x884f;
const TEXTURE_SWIZZLE = [0x8e42, 0x8e43, 0x8e44, 0x8e45];
const UNPACK_FLIP_Y = 0x9240;
const UNPACK_PREMULTIPLY_ALPHA = 0x9241;
const UNPACK_COLORSPACE_CONVERSION = 0x9243;
const BROWSER_DEFAULT = 0x9244;
const MAX_CLIENT_WAIT_TIMEOUT = 0x9247;
const UNMASKED_VENDOR = 0x9245;
const UNMASKED_RENDERER = 0x9246;
const MAX_ANISOTROPY = 0x84ff;
const ANY_SAMPLES_PASSED_CONSERVATIVE = 0x8d6a;
const BACK = 0x0405;
const INFO_LOG_LENGTH = 0x8b84;

// Arguments and result: u u32, i i32, f f32, d f64, b u8 (GLboolean), p pointer, l i64, q u64.
const SIGNATURES = `
CGLChoosePixelFormat ppp:i|CGLCreateContext ppp:i|CGLSetCurrentContext p:i|CGLDestroyContext p:i|CGLDestroyPixelFormat p:i
glActiveTexture u|glAttachShader uu|glBeginQuery uu|glBeginTransformFeedback u|glBindAttribLocation uup|glBindBuffer uu
glBindBufferBase uuu|glBindBufferRange uuull|glBindFramebuffer uu|glBindRenderbuffer uu|glBindSampler uu|glBindTexture uu
glBindTransformFeedback uu|glBindVertexArray u|glBlendColor ffff|glBlendEquation u|glBlendEquationSeparate uu|glBlendFunc uu
glBlendFuncSeparate uuuu|glBlitFramebuffer iiiiiiiiuu|glBufferData ulpu|glBufferSubData ullp|glCheckFramebufferStatus u:u
glClear u|glClearBufferfi uifi|glClearBufferfv uip|glClearBufferiv uip|glClearBufferuiv uip|glClearColor ffff|glClearDepth d
glClearStencil i|glClientWaitSync puq:u|glColorMask bbbb|glCompileShader u|glCompressedTexImage2D uiuiiiip
glCompressedTexImage3D uiuiiiiip|glCompressedTexSubImage2D uiiiiiuip|glCompressedTexSubImage3D uiiiiiiiuip
glCopyBufferSubData uulll|glCopyTexImage2D uiuiiiii|glCopyTexSubImage2D uiiiiiii|glCopyTexSubImage3D uiiiiiiii
glCreateProgram :u|glCreateShader u:u|glCullFace u|glDeleteBuffers ip|glDeleteFramebuffers ip|glDeleteProgram u
glDeleteQueries ip|glDeleteRenderbuffers ip|glDeleteSamplers ip|glDeleteShader u|glDeleteSync p|glDeleteTextures ip
glDeleteTransformFeedbacks ip|glDeleteVertexArrays ip|glDepthFunc u|glDepthMask b|glDepthRange dd|glDetachShader uu|glDisable u
glDisableVertexAttribArray u|glDrawArrays uii|glDrawArraysInstanced uiii|glDrawBuffers ip|glDrawElements uiup
glDrawElementsInstanced uiupi|glDrawRangeElements uuuiup|glEnable u|glEnableVertexAttribArray u|glEndQuery u
glEndTransformFeedback |glFenceSync uu:p|glFinish |glFlush |glFramebufferRenderbuffer uuuu|glFramebufferTexture2D uuuui
glFramebufferTextureLayer uuuii|glFrontFace u|glGenBuffers ip|glGenFramebuffers ip|glGenQueries ip|glGenRenderbuffers ip
glGenSamplers ip|glGenTextures ip|glGenTransformFeedbacks ip|glGenVertexArrays ip|glGenerateMipmap u
glGetActiveAttrib uuipppp|glGetActiveUniform uuipppp|glGetActiveUniformBlockName uuipp|glGetActiveUniformBlockiv uuup
glGetActiveUniformsiv uipup|glGetAttachedShaders uipp|glGetAttribLocation up:i|glGetBooleanv up|glGetBufferParameteriv uup
glGetBufferSubData ullp|glGetError :u|glGetFloatv up|glGetFragDataLocation up:i|glGetFramebufferAttachmentParameteriv uuup
glGetInteger64v up|glGetIntegeri_v uup|glGetIntegerv up|glGetInternalformativ uuuip|glGetProgramInfoLog uipp|glGetProgramiv uup
glGetQueryObjectuiv uup|glGetQueryiv uup|glGetRenderbufferParameteriv uup|glGetSamplerParameterfv uup
glGetSamplerParameteriv uup|glGetShaderInfoLog uipp|glGetShaderPrecisionFormat uupp|glGetShaderiv uup|glGetString u:p
glGetSynciv puipp|glGetTexParameterfv uup|glGetTexParameteriv uup|glGetTransformFeedbackVarying uuipppp
glGetUniformBlockIndex up:u|glGetUniformIndices uipp|glGetUniformLocation up:i|glGetUniformfv uip|glGetUniformiv uip
glGetUniformuiv uip|glGetVertexAttribfv uup|glGetVertexAttribiv uup|glGetVertexAttribIiv uup|glGetVertexAttribIuiv uup
glGetVertexAttribPointerv uup|glHint uu|glIsBuffer u:b|glIsEnabled u:b|glIsFramebuffer u:b|glIsProgram u:b|glIsQuery u:b
glIsRenderbuffer u:b|glIsSampler u:b|glIsShader u:b|glIsSync p:b|glIsTexture u:b|glIsTransformFeedback u:b|glIsVertexArray u:b
glLineWidth f|glLinkProgram u|glPauseTransformFeedback |glPixelStorei ui|glPolygonOffset ff|glReadBuffer u
glReadPixels iiiiuup|glRenderbufferStorage uuii|glRenderbufferStorageMultisample uiuii|glResumeTransformFeedback
glSampleCoverage fb|glSamplerParameterf uuf|glSamplerParameteri uui|glScissor iiii|glShaderSource uipp|glStencilFunc uiu
glStencilFuncSeparate uuiu|glStencilMask u|glStencilMaskSeparate uu|glStencilOp uuu|glStencilOpSeparate uuuu
glTexImage2D uiiiiiuup|glTexImage3D uiiiiiiuup|glTexParameterf uuf|glTexParameteri uui|glTexStorage2D uiuii
glTexStorage3D uiuiii|glTexSubImage2D uiiiiiuup|glTexSubImage3D uiiiiiiiuup|glTransformFeedbackVaryings uipu
glUniform1f if|glUniform1fv iip|glUniform1i ii|glUniform1iv iip|glUniform1ui iu|glUniform1uiv iip|glUniform2f iff
glUniform2fv iip|glUniform2i iii|glUniform2iv iip|glUniform2ui iuu|glUniform2uiv iip|glUniform3f ifff|glUniform3fv iip
glUniform3i iiii|glUniform3iv iip|glUniform3ui iuuu|glUniform3uiv iip|glUniform4f iffff|glUniform4fv iip|glUniform4i iiiii
glUniform4iv iip|glUniform4ui iuuuu|glUniform4uiv iip|glUniformBlockBinding uuu|glUniformMatrix2fv iibp
glUniformMatrix2x3fv iibp|glUniformMatrix2x4fv iibp|glUniformMatrix3fv iibp|glUniformMatrix3x2fv iibp
glUniformMatrix3x4fv iibp|glUniformMatrix4fv iibp|glUniformMatrix4x2fv iibp|glUniformMatrix4x3fv iibp|glUseProgram u
glValidateProgram u|glVertexAttrib1f uf|glVertexAttrib1fv up|glVertexAttrib2f uff|glVertexAttrib2fv up|glVertexAttrib3f ufff
glVertexAttrib3fv up|glVertexAttrib4f uffff|glVertexAttrib4fv up|glVertexAttribDivisor uu|glVertexAttribI4i uiiii
glVertexAttribI4iv up|glVertexAttribI4ui uuuuu|glVertexAttribI4uiv up|glVertexAttribIPointer uiuip|glVertexAttribPointer uiubip
glViewport iiii|glWaitSync puq`;
const TYPES = { u: FFIType.u32, i: FFIType.i32, f: FFIType.f32, d: FFIType.f64, b: FFIType.u8, p: FFIType.ptr, l: FFIType.i64, q: FFIType.u64 };

let lib = null;
function gl() {
  if (lib) return lib;
  const symbols = {};
  for (const entry of SIGNATURES.trim().split(/[|\n]/)) {
    const [name, sig = ""] = entry.trim().split(" ");
    const [args, ret] = sig.split(":");
    symbols[name] = { args: [...(args ?? "")].map((c) => TYPES[c]), returns: ret ? TYPES[ret] : FFIType.void };
  }
  lib = dlopen("/System/Library/Frameworks/OpenGL.framework/OpenGL", symbols).symbols;
  return lib;
}

let current = 0;
function use(ctx) {
  if (current === ctx._cgl) return;
  gl().CGLSetCurrentContext(ctx._cgl);
  current = ctx._cgl;
}

// A 3.2+ core profile context: macOS gives 4.1.
function createCGL() {
  const g = gl();
  const attrs = new Int32Array([99 /* OpenGLProfile */, 0x3200, 73 /* Accelerated */, 96 /* AllowOfflineRenderers */, 0]);
  const format = new BigUint64Array(1);
  const out = new BigUint64Array(1);
  if (g.CGLChoosePixelFormat(attrs, format, new Int32Array(1)) || !format[0]) throw new Error("gpuijs: no OpenGL pixel format for WebGL");
  const err = g.CGLCreateContext(Number(format[0]), null, out);
  g.CGLDestroyPixelFormat(Number(format[0]));
  if (err || !out[0]) throw new Error(`gpuijs: OpenGL context for WebGL failed (CGL error ${err})`);
  return Number(out[0]);
}

const one = new Int32Array(1);
const genOne = (fn) => (fn(1, one), one[0]);
const deleteOne = (fn, id) => fn(1, new Uint32Array([id]));
const getInt = (fn, ...args) => (fn(...args, one), one[0]);
const cstr = (s) => Buffer.from(`${s}\0`);
const bytes = (v) => (ArrayBuffer.isView(v) ? new Uint8Array(v.buffer, v.byteOffset, v.byteLength) : new Uint8Array(v));
// A WebGL2 `srcOffset`/`length` pair over a typed array, in elements.
const slice = (v, offset = 0, length = 0) => (offset || length ? v.subarray(offset, length ? offset + length : undefined) : v);

class WebGLObject {
  constructor(ctx, id) {
    this._ctx = ctx;
    this._id = id;
  }
}
export class WebGLBuffer extends WebGLObject {}
export class WebGLFramebuffer extends WebGLObject {}
export class WebGLProgram extends WebGLObject {}
export class WebGLQuery extends WebGLObject {}
export class WebGLRenderbuffer extends WebGLObject {}
export class WebGLSampler extends WebGLObject {}
export class WebGLShader extends WebGLObject {}
export class WebGLSync extends WebGLObject {}
export class WebGLTexture extends WebGLObject {}
export class WebGLTransformFeedback extends WebGLObject {}
export class WebGLVertexArrayObject extends WebGLObject {}
export class WebGLUniformLocation {
  constructor(program, id, name) {
    Object.assign(this, { _program: program, _id: id, _name: name });
  }
}
export class WebGLActiveInfo {
  constructor(name, size, type) {
    Object.assign(this, { name, size, type });
  }
}
export class WebGLShaderPrecisionFormat {
  constructor(rangeMin, rangeMax, precision) {
    Object.assign(this, { rangeMin, rangeMax, precision });
  }
}
// What `getParameter` answers with an object for: its binding's GL query and wrapper class.
const BINDINGS = {
  [GL.ARRAY_BUFFER_BINDING]: WebGLBuffer,
  [GL.ELEMENT_ARRAY_BUFFER_BINDING]: WebGLBuffer,
  [GL.COPY_READ_BUFFER_BINDING]: WebGLBuffer,
  [GL.COPY_WRITE_BUFFER_BINDING]: WebGLBuffer,
  [GL.PIXEL_PACK_BUFFER_BINDING]: WebGLBuffer,
  [GL.PIXEL_UNPACK_BUFFER_BINDING]: WebGLBuffer,
  [GL.TRANSFORM_FEEDBACK_BUFFER_BINDING]: WebGLBuffer,
  [GL.UNIFORM_BUFFER_BINDING]: WebGLBuffer,
  [GL.CURRENT_PROGRAM]: WebGLProgram,
  [GL.TEXTURE_BINDING_2D]: WebGLTexture,
  [GL.TEXTURE_BINDING_CUBE_MAP]: WebGLTexture,
  [GL.TEXTURE_BINDING_3D]: WebGLTexture,
  [GL.TEXTURE_BINDING_2D_ARRAY]: WebGLTexture,
  [GL.FRAMEBUFFER_BINDING]: WebGLFramebuffer,
  [GL.READ_FRAMEBUFFER_BINDING]: WebGLFramebuffer,
  [GL.RENDERBUFFER_BINDING]: WebGLRenderbuffer,
  [GL.VERTEX_ARRAY_BINDING]: WebGLVertexArrayObject,
  [GL.TRANSFORM_FEEDBACK_BINDING]: WebGLTransformFeedback,
  [GL.SAMPLER_BINDING]: WebGLSampler,
};
const BOOLEANS = new Set([GL.BLEND, GL.CULL_FACE, GL.DEPTH_TEST, GL.DEPTH_WRITEMASK, GL.DITHER, GL.POLYGON_OFFSET_FILL, GL.SAMPLE_ALPHA_TO_COVERAGE, GL.SAMPLE_COVERAGE, GL.SAMPLE_COVERAGE_INVERT, GL.SCISSOR_TEST, GL.STENCIL_TEST, GL.RASTERIZER_DISCARD, GL.TRANSFORM_FEEDBACK_ACTIVE, GL.TRANSFORM_FEEDBACK_PAUSED]);
const FLOATS = new Set([GL.LINE_WIDTH, GL.DEPTH_CLEAR_VALUE, GL.POLYGON_OFFSET_FACTOR, GL.POLYGON_OFFSET_UNITS, GL.SAMPLE_COVERAGE_VALUE, GL.MAX_TEXTURE_LOD_BIAS, MAX_ANISOTROPY]);
const INT64S = new Set([GL.MAX_SERVER_WAIT_TIMEOUT, GL.MAX_UNIFORM_BLOCK_SIZE, GL.MAX_COMBINED_VERTEX_UNIFORM_COMPONENTS, GL.MAX_COMBINED_FRAGMENT_UNIFORM_COMPONENTS]);
// pname -> [array type, length]
const VECTORS = {
  [GL.VIEWPORT]: [Int32Array, 4],
  [GL.SCISSOR_BOX]: [Int32Array, 4],
  [GL.MAX_VIEWPORT_DIMS]: [Int32Array, 2],
  [GL.COLOR_CLEAR_VALUE]: [Float32Array, 4],
  [GL.BLEND_COLOR]: [Float32Array, 4],
  [GL.DEPTH_RANGE]: [Float32Array, 2],
  [GL.ALIASED_LINE_WIDTH_RANGE]: [Float32Array, 2],
};
const PROGRAM_BOOLEANS = new Set([GL.DELETE_STATUS, GL.LINK_STATUS, GL.VALIDATE_STATUS]);
const SHADER_BOOLEANS = new Set([GL.DELETE_STATUS, GL.COMPILE_STATUS]);

// Desktop core GL lacks WebGL1's luminance and alpha textures, and wants float and half-float
// textures sized: an upload's internal format, format and type as core GL takes them, and the
// swizzle that makes a red/green texture read as luminance/alpha.
function coreFormat(internal, format, type) {
  if (type === HALF_FLOAT_OES) type = GL.HALF_FLOAT;
  const float = type === GL.FLOAT;
  const half = type === GL.HALF_FLOAT;
  const sized = (bits8, bits16, bits32) => (float ? bits32 : half ? bits16 : bits8);
  switch (internal) {
    case GL.LUMINANCE:
      return [sized(R8, GL.R16F, GL.R32F), RED, type, [RED, RED, RED, GL.ONE]];
    case GL.ALPHA:
      return [sized(R8, GL.R16F, GL.R32F), RED, type, [GL.ZERO, GL.ZERO, GL.ZERO, RED]];
    case GL.LUMINANCE_ALPHA:
      return [sized(RG8, GL.RG16F, GL.RG32F), RG, type, [RED, RED, RED, GREEN]];
    case GL.RGBA:
      return [sized(GL.RGBA8, GL.RGBA16F, GL.RGBA32F), format, type, null];
    case GL.RGB:
      return [sized(GL.RGB8, GL.RGB16F, GL.RGB32F), format, type, null];
    case 0x8c40: // SRGB_EXT
      return [GL.SRGB8, GL.RGB, type, null];
    case 0x8c42: // SRGB_ALPHA_EXT
      return [GL.SRGB8_ALPHA8, GL.RGBA, type, null];
    case GL.DEPTH_COMPONENT:
      return [type === GL.UNSIGNED_SHORT ? GL.DEPTH_COMPONENT16 : GL.DEPTH_COMPONENT24, format, type, null];
    case GL.DEPTH_STENCIL:
      return [GL.DEPTH24_STENCIL8, format, type, null];
  }
  return [internal, format === GL.LUMINANCE ? RED : format, type, null];
}
const CHANNELS = { [GL.RGBA]: 4, [GL.RGB]: 3, [RED]: 1, [GL.RED]: 1, [RG]: 2, [GL.LUMINANCE]: 1, [GL.ALPHA]: 1, [GL.LUMINANCE_ALPHA]: 2, [GL.RGBA_INTEGER]: 4, [GL.RGB_INTEGER]: 3, [GL.RG_INTEGER]: 2, [GL.RED_INTEGER]: 1, [GL.DEPTH_COMPONENT]: 1, [GL.DEPTH_STENCIL]: 1 };
const TYPE_BYTES = { [GL.UNSIGNED_BYTE]: 1, [GL.BYTE]: 1, [GL.UNSIGNED_SHORT]: 2, [GL.SHORT]: 2, [GL.HALF_FLOAT]: 2, [HALF_FLOAT_OES]: 2, [GL.UNSIGNED_INT]: 4, [GL.INT]: 4, [GL.FLOAT]: 4 };
// Packed types hold a whole pixel.
const PACKED = { [GL.UNSIGNED_SHORT_5_6_5]: 2, [GL.UNSIGNED_SHORT_4_4_4_4]: 2, [GL.UNSIGNED_SHORT_5_5_5_1]: 2, [GL.UNSIGNED_INT_24_8]: 4, [GL.UNSIGNED_INT_2_10_10_10_REV]: 4, [GL.UNSIGNED_INT_10F_11F_11F_REV]: 4, [GL.UNSIGNED_INT_5_9_9_9_REV]: 4 };
const pixelBytes = (format, type) => PACKED[type] ?? (CHANNELS[format] ?? 4) * (TYPE_BYTES[type] ?? 1);

/** GLSL ES (1.00 or 3.00) as GLSL 4.10 core takes it. */
export function translateShader(source, vertex) {
  let src = String(source).replace(/^﻿/, "");
  const es3 = /^\s*#version\s+300\s+es\b/m.test(src);
  src = src.replace(/^[ \t]*#version[^\n]*/m, "");
  // Precision on opaque types is ES-only.
  src = src.replace(/^[ \t]*precision\s+\w+\s+[iu]?sampler\w*\s*;/gm, "");
  // What an ES compiler predefines; shaders test them (`#ifdef GL_ES` picks Pixi's WebGL1 branch).
  // GL may not `#define` GL_ names, so they're renamed.
  src = src.replace(/\b(GL_ES|GL_FRAGMENT_PRECISION_HIGH)\b/g, "gpuijs_$1");
  const version = `#version 410 core\n#define gpuijs_GL_ES 1\n${vertex ? "" : "#define gpuijs_GL_FRAGMENT_PRECISION_HIGH 1\n"}`;
  if (es3) return version + src.replace(/\bgl_FragColor\b/g, "gpuijs_FragColor");
  // 1.00's extensions are core GL 4.1.
  src = src.replace(/^[ \t]*#extension\s+GL_(OES_standard_derivatives|EXT_shader_texture_lod|EXT_frag_depth|EXT_draw_buffers)\b[^\n]*/gm, "");
  src = src.replace(/\bgl_FragDepthEXT\b/g, "gl_FragDepth");
  let head = version;
  head += vertex ? "#define attribute in\n#define varying out\n" : "#define varying in\n";
  for (const [from, to] of [["texture2DProjLodEXT", "textureProjLod"], ["texture2DLodEXT", "textureLod"], ["textureCubeLodEXT", "textureLod"], ["texture2DGradEXT", "textureGrad"], ["texture2DProjGradEXT", "textureProjGrad"], ["textureCubeGradEXT", "textureGrad"], ["texture2DProjLod", "textureProjLod"], ["texture2DProj", "textureProj"], ["texture2DLod", "textureLod"], ["texture2D", "texture"], ["textureCubeLod", "textureLod"], ["textureCube", "texture"]]) {
    head += `#define ${from} ${to}\n`;
  }
  if (!vertex) {
    const data = new Set([...src.matchAll(/\bgl_FragData\s*\[\s*(\d+)\s*\]/g)].map((m) => Number(m[1])));
    if (data.size) {
      src = src.replace(/\bgl_FragData\s*\[\s*(\d+)\s*\]/g, "gpuijs_FragData$1");
      for (const n of data) head += `layout(location = ${n}) out vec4 gpuijs_FragData${n};\n`;
    } else if (/\bgl_FragColor\b/.test(src)) {
      src = src.replace(/\bgl_FragColor\b/g, "gpuijs_FragColor");
      head += "out vec4 gpuijs_FragColor;\n";
    }
  }
  return head + src;
}

// Pixels of what `texImage2D` takes besides arrays: ImageData, canvases, <img>/`Image`s, Skia
// images. Straight-alpha RGBA rows, top first.
// shortcut: no <video> frames; read them from the host when a scene textures one.
function sourcePixels(source) {
  if (source?.data && source.width != null && !source.dom) return { width: source.width, height: source.height, data: bytes(source.data) };
  if (source?.type === "canvas" && source.dom) return source.pixels() ?? { width: 0, height: 0, data: new Uint8Array(0) };
  const image = source?.type === "img" && source.dom ? source.decoded() : source;
  if (!image?.width) throw new TypeError("gpuijs: this texture source has no pixels (a <video>, or an image still loading)");
  const { createCanvas } = skia();
  const canvas = createCanvas(image.width, image.height);
  const ctx = canvas.getContext("2d");
  ctx.drawImage(image, 0, 0);
  return { width: image.width, height: image.height, data: bytes(ctx.getImageData(0, 0, image.width, image.height).data) };
}

// RGBA bytes as `format`/`type` wants them (unsigned bytes, floats or half floats).
function convertPixels(rgba, format, type) {
  const channels = { [GL.RGBA]: [0, 1, 2, 3], [GL.RGB]: [0, 1, 2], [GL.LUMINANCE]: [0], [RED]: [0], [GL.RED]: [0], [GL.ALPHA]: [3], [GL.LUMINANCE_ALPHA]: [0, 3], [RG]: [0, 1] }[format] ?? [0, 1, 2, 3];
  const n = rgba.length / 4;
  const float = type === GL.FLOAT;
  const half = type === GL.HALF_FLOAT || type === HALF_FLOAT_OES;
  if (channels.length === 4 && !float && !half) return rgba;
  const out = float ? new Float32Array(n * channels.length) : half ? new Uint16Array(n * channels.length) : new Uint8Array(n * channels.length);
  for (let i = 0, o = 0; i < n; i++) {
    for (const c of channels) {
      const v = rgba[i * 4 + c];
      out[o++] = float ? v / 255 : half ? toHalf(v / 255) : v;
    }
  }
  return out;
}

const halfView = new DataView(new ArrayBuffer(4));
function toHalf(v) {
  halfView.setFloat32(0, v);
  const x = halfView.getUint32(0);
  const exp = ((x >> 23) & 0xff) - 112;
  if (exp <= 0) return (x >> 16) & 0x8000;
  return ((x >> 16) & 0x8000) | (Math.min(exp, 31) << 10) | ((x >> 13) & 0x3ff);
}

// Rows top to bottom, `rowBytes` apart.
function flipRows(data, rowBytes, rows) {
  const out = new data.constructor(data.length);
  const per = rowBytes / data.BYTES_PER_ELEMENT;
  for (let y = 0; y < rows; y++) out.set(data.subarray(y * per, (y + 1) * per), (rows - 1 - y) * per);
  return out;
}

function premultiply(rgba) {
  const out = new Uint8Array(rgba);
  for (let i = 0; i < out.length; i += 4) {
    const a = out[i + 3] / 255;
    out[i] *= a;
    out[i + 1] *= a;
    out[i + 2] *= a;
  }
  return out;
}

class WebGLRenderingContextBase {
  constructor(canvas, attributes = {}, version) {
    this.canvas = canvas;
    this._version = version;
    this._attrs = { alpha: true, depth: true, stencil: false, antialias: true, premultipliedAlpha: true, preserveDrawingBuffer: false, powerPreference: "default", failIfMajorPerformanceCaveat: false, desynchronized: false, ...attributes };
    this._cgl = createCGL();
    this._unpack = { flipY: false, premultiply: false, colorspace: BROWSER_DEFAULT };
    // GL name -> wrapper, per class, for getParameter's object answers.
    this._objects = new Map();
    this._extensions = new Map();
    use(this);
    const g = gl();
    // WebGL's vertex array 0: core GL has none to draw with.
    this._vao = genOne(g.glGenVertexArrays);
    g.glBindVertexArray(this._vao);
    // WebGL sizes points from the shader and samples cube maps seamlessly.
    g.glEnable(PROGRAM_POINT_SIZE);
    g.glEnable(TEXTURE_CUBE_MAP_SEAMLESS);
    this._fbo = genOne(g.glGenFramebuffers);
    this._color = genOne(g.glGenRenderbuffers);
    this._depth = genOne(g.glGenRenderbuffers);
    this._samples = this._attrs.antialias ? 4 : 0;
    // Multisampled drawing resolves into this for reading.
    if (this._samples) {
      this._resolveFbo = genOne(g.glGenFramebuffers);
      this._resolveColor = genOne(g.glGenRenderbuffers);
    }
    this._resizeDrawingBuffer();
    g.glBindFramebuffer(GL.FRAMEBUFFER, this._fbo);
    g.glViewport(0, 0, this.drawingBufferWidth, this.drawingBufferHeight);
    g.glScissor(0, 0, this.drawingBufferWidth, this.drawingBufferHeight);
  }

  get drawingBufferWidth() {
    return Math.max(1, this.canvas.width);
  }
  get drawingBufferHeight() {
    return Math.max(1, this.canvas.height);
  }
  get drawingBufferColorSpace() {
    return "srgb";
  }

  /** (Re)allocates the default framebuffer at the canvas's size; bindings stay as they were. */
  _resizeDrawingBuffer() {
    use(this);
    const g = gl();
    const [w, h] = [this.drawingBufferWidth, this.drawingBufferHeight];
    if (this._size?.[0] === w && this._size?.[1] === h) return;
    this._size = [w, h];
    const fb = getInt(g.glGetIntegerv, GL.FRAMEBUFFER_BINDING);
    const rb = getInt(g.glGetIntegerv, GL.RENDERBUFFER_BINDING);
    const storage = (rb, format, samples) => {
      g.glBindRenderbuffer(GL.RENDERBUFFER, rb);
      samples ? g.glRenderbufferStorageMultisample(GL.RENDERBUFFER, samples, format, w, h) : g.glRenderbufferStorage(GL.RENDERBUFFER, format, w, h);
    };
    // Without alpha, an RGB buffer reads back opaque.
    const color = this._attrs.alpha ? GL.RGBA8 : GL.RGB8;
    storage(this._color, color, this._samples);
    storage(this._depth, GL.DEPTH24_STENCIL8, this._samples);
    g.glBindFramebuffer(GL.FRAMEBUFFER, this._fbo);
    g.glFramebufferRenderbuffer(GL.FRAMEBUFFER, GL.COLOR_ATTACHMENT0, GL.RENDERBUFFER, this._color);
    if (this._attrs.depth || this._attrs.stencil) g.glFramebufferRenderbuffer(GL.FRAMEBUFFER, GL.DEPTH_STENCIL_ATTACHMENT, GL.RENDERBUFFER, this._depth);
    if (this._samples) {
      storage(this._resolveColor, color, 0);
      g.glBindFramebuffer(GL.FRAMEBUFFER, this._resolveFbo);
      g.glFramebufferRenderbuffer(GL.FRAMEBUFFER, GL.COLOR_ATTACHMENT0, GL.RENDERBUFFER, this._resolveColor);
    }
    g.glBindRenderbuffer(GL.RENDERBUFFER, rb);
    g.glBindFramebuffer(GL.FRAMEBUFFER, fb || this._fbo);
  }

  // The default framebuffer, resolved if multisampled, bound for reading; returns what to restore.
  _readDefault() {
    const g = gl();
    const read = getInt(g.glGetIntegerv, GL.READ_FRAMEBUFFER_BINDING);
    const draw = getInt(g.glGetIntegerv, GL.DRAW_FRAMEBUFFER_BINDING);
    if (this._samples) {
      // Blits are scissored.
      const scissor = g.glIsEnabled(GL.SCISSOR_TEST);
      if (scissor) g.glDisable(GL.SCISSOR_TEST);
      const [w, h] = this._size;
      g.glBindFramebuffer(GL.READ_FRAMEBUFFER, this._fbo);
      g.glBindFramebuffer(GL.DRAW_FRAMEBUFFER, this._resolveFbo);
      g.glBlitFramebuffer(0, 0, w, h, 0, 0, w, h, GL.COLOR_BUFFER_BIT, GL.NEAREST);
      if (scissor) g.glEnable(GL.SCISSOR_TEST);
    }
    g.glBindFramebuffer(GL.READ_FRAMEBUFFER, this._samples ? this._resolveFbo : this._fbo);
    return () => {
      g.glBindFramebuffer(GL.READ_FRAMEBUFFER, read);
      g.glBindFramebuffer(GL.DRAW_FRAMEBUFFER, draw);
    };
  }

  /** The drawing buffer for the host: straight-alpha RGBA rows, top first. */
  _snapshot() {
    use(this);
    const g = gl();
    this._resizeDrawingBuffer();
    const [width, height] = this._size;
    const restore = this._readDefault();
    const pack = getInt(g.glGetIntegerv, GL.PIXEL_PACK_BUFFER_BINDING);
    const alignment = getInt(g.glGetIntegerv, GL.PACK_ALIGNMENT);
    if (pack) g.glBindBuffer(GL.PIXEL_PACK_BUFFER, 0);
    g.glPixelStorei(GL.PACK_ALIGNMENT, 4);
    const raw = new Uint8Array(width * height * 4);
    g.glReadPixels(0, 0, width, height, GL.RGBA, GL.UNSIGNED_BYTE, raw);
    g.glPixelStorei(GL.PACK_ALIGNMENT, alignment);
    if (pack) g.glBindBuffer(GL.PIXEL_PACK_BUFFER, pack);
    restore();
    const data = flipRows(raw, width * 4, height);
    if (this._attrs.alpha && this._attrs.premultipliedAlpha) {
      for (let i = 0; i < data.length; i += 4) {
        const a = data[i + 3];
        if (a && a < 255) for (let c = i; c < i + 3; c++) data[c] = Math.min(255, Math.round((data[c] * 255) / a));
      }
    }
    // shortcut: the buffer is kept after it's shown, as with `preserveDrawingBuffer: true`; apps clear each frame anyway.
    return { width, height, data };
  }

  _drew() {
    if (this._defaultBound !== false) this.canvas.dom.paint(this.canvas);
  }

  _wrap(Class, id) {
    if (!id) return null;
    const key = Class.name;
    let byId = this._objects.get(key);
    if (!byId) this._objects.set(key, (byId = new Map()));
    let o = byId.get(id);
    if (!o) byId.set(id, (o = new Class(this, id)));
    return o;
  }
  _forget(o) {
    this._objects.get(o.constructor.name)?.delete(o._id);
  }
  _create(Class, gen) {
    use(this);
    return this._wrap(Class, gen());
  }
  _delete(o, fn) {
    if (!o || o._deleted) return;
    use(this);
    o._deleted = true;
    fn(o._id);
    this._forget(o);
  }

  getContextAttributes() {
    return { ...this._attrs };
  }
  isContextLost() {
    return false;
  }
  getError() {
    use(this);
    return gl().glGetError();
  }
  getSupportedExtensions() {
    return Object.keys(this._extensionTable());
  }
  getExtension(name) {
    const make = this._extensionTable()[name];
    if (!make) return null;
    if (!this._extensions.has(name)) this._extensions.set(name, make());
    return this._extensions.get(name);
  }
  _extensionTable() {
    const constants = (name) => ({ ...EXTENSIONS[name] });
    const common = {
      EXT_texture_filter_anisotropic: () => constants("EXT_texture_filter_anisotropic"),
      WEBGL_debug_renderer_info: () => constants("WEBGL_debug_renderer_info"),
      WEBGL_compressed_texture_s3tc: () => constants("WEBGL_compressed_texture_s3tc"),
      WEBGL_lose_context: () => ({ loseContext() {}, restoreContext() {} }),
      OES_texture_float_linear: () => ({}),
      EXT_float_blend: () => ({}),
    };
    if (this._version === 2) return { ...common, EXT_color_buffer_float: () => ({}), EXT_color_buffer_half_float: () => constants("EXT_color_buffer_half_float") };
    const gl1 = this;
    return {
      ...common,
      ANGLE_instanced_arrays: () => ({
        ...constants("ANGLE_instanced_arrays"),
        drawArraysInstancedANGLE: (...a) => gl1._drawArraysInstanced(...a),
        drawElementsInstancedANGLE: (...a) => gl1._drawElementsInstanced(...a),
        vertexAttribDivisorANGLE: (i, d) => (use(gl1), gl().glVertexAttribDivisor(i, d)),
      }),
      OES_vertex_array_object: () => ({
        ...constants("OES_vertex_array_object"),
        createVertexArrayOES: () => gl1._createVertexArray(),
        deleteVertexArrayOES: (v) => gl1._deleteVertexArray(v),
        isVertexArrayOES: (v) => gl1._isVertexArray(v),
        bindVertexArrayOES: (v) => gl1._bindVertexArray(v),
      }),
      WEBGL_draw_buffers: () => ({ ...constants("WEBGL_draw_buffers"), drawBuffersWEBGL: (b) => gl1._drawBuffers(b) }),
      OES_element_index_uint: () => ({}),
      OES_texture_float: () => ({}),
      OES_texture_half_float: () => constants("OES_texture_half_float"),
      OES_texture_half_float_linear: () => ({}),
      OES_standard_derivatives: () => constants("OES_standard_derivatives"),
      WEBGL_depth_texture: () => constants("WEBGL_depth_texture"),
      EXT_blend_minmax: () => constants("EXT_blend_minmax"),
      EXT_shader_texture_lod: () => ({}),
      EXT_frag_depth: () => ({}),
      EXT_sRGB: () => constants("EXT_sRGB"),
    };
  }

  getParameter(pname) {
    use(this);
    const g = gl();
    switch (pname) {
      case GL.VENDOR:
        return "WebKit";
      case GL.RENDERER:
        return "WebKit WebGL";
      case GL.VERSION:
        return this._version === 2 ? "WebGL 2.0 (OpenGL ES 3.0 gpuijs)" : "WebGL 1.0 (OpenGL ES 2.0 gpuijs)";
      case GL.SHADING_LANGUAGE_VERSION:
        return this._version === 2 ? "WebGL GLSL ES 3.00 (gpuijs)" : "WebGL GLSL ES 1.0 (gpuijs)";
      case UNMASKED_VENDOR:
        return new CString(g.glGetString(GL.VENDOR)).toString();
      case UNMASKED_RENDERER:
        return new CString(g.glGetString(GL.RENDERER)).toString();
      case UNPACK_FLIP_Y:
        return this._unpack.flipY;
      case UNPACK_PREMULTIPLY_ALPHA:
        return this._unpack.premultiply;
      case UNPACK_COLORSPACE_CONVERSION:
        return this._unpack.colorspace;
      case MAX_CLIENT_WAIT_TIMEOUT:
        return 0;
      case GL.MAX_ELEMENT_INDEX:
        return 2 ** 32 - 1;
      case GL.COLOR_WRITEMASK: {
        const v = new Uint8Array(4);
        g.glGetBooleanv(pname, v);
        return [...v].map(Boolean);
      }
      case GL.COMPRESSED_TEXTURE_FORMATS:
        return new Uint32Array(this._extensions.has("WEBGL_compressed_texture_s3tc") ? Object.values(EXTENSIONS.WEBGL_compressed_texture_s3tc) : []);
      case GL.READ_BUFFER:
        return this._defaultBound !== false ? BACK : getInt(g.glGetIntegerv, pname);
    }
    if (pname >= GL.DRAW_BUFFER0 && pname <= GL.DRAW_BUFFER15 && this._defaultBound !== false) return pname === GL.DRAW_BUFFER0 ? BACK : GL.NONE;
    const Class = BINDINGS[pname];
    if (Class) {
      const id = getInt(g.glGetIntegerv, pname);
      if (Class === WebGLFramebuffer && (id === this._fbo || id === this._resolveFbo)) return null;
      if (Class === WebGLVertexArrayObject && id === this._vao) return null;
      return this._wrap(Class, id);
    }
    if (BOOLEANS.has(pname)) {
      const v = new Uint8Array(1);
      g.glGetBooleanv(pname, v);
      return Boolean(v[0]);
    }
    if (FLOATS.has(pname)) {
      const v = new Float32Array(1);
      g.glGetFloatv(pname, v);
      return v[0];
    }
    if (INT64S.has(pname)) {
      const v = new BigInt64Array(1);
      g.glGetInteger64v(pname, v);
      return Number(v[0]);
    }
    const vector = VECTORS[pname];
    if (vector) {
      const [Type, n] = vector;
      const v = new Type(n);
      (Type === Float32Array ? g.glGetFloatv : g.glGetIntegerv)(pname, v);
      return v;
    }
    return getInt(g.glGetIntegerv, pname);
  }

  // Objects
  createBuffer() {
    return this._create(WebGLBuffer, () => genOne(gl().glGenBuffers));
  }
  createFramebuffer() {
    return this._create(WebGLFramebuffer, () => genOne(gl().glGenFramebuffers));
  }
  createRenderbuffer() {
    return this._create(WebGLRenderbuffer, () => genOne(gl().glGenRenderbuffers));
  }
  createTexture() {
    return this._create(WebGLTexture, () => genOne(gl().glGenTextures));
  }
  createProgram() {
    return this._create(WebGLProgram, () => gl().glCreateProgram());
  }
  createShader(type) {
    const shader = this._create(WebGLShader, () => gl().glCreateShader(type));
    if (shader) shader._type = type;
    return shader;
  }
  deleteBuffer(o) {
    this._delete(o, (id) => deleteOne(gl().glDeleteBuffers, id));
  }
  deleteFramebuffer(o) {
    this._delete(o, (id) => deleteOne(gl().glDeleteFramebuffers, id));
  }
  deleteRenderbuffer(o) {
    this._delete(o, (id) => deleteOne(gl().glDeleteRenderbuffers, id));
  }
  deleteTexture(o) {
    this._delete(o, (id) => deleteOne(gl().glDeleteTextures, id));
  }
  deleteProgram(o) {
    this._delete(o, (id) => gl().glDeleteProgram(id));
  }
  deleteShader(o) {
    this._delete(o, (id) => gl().glDeleteShader(id));
  }
  _is(o, Class, fn) {
    if (!(o instanceof Class) || o._deleted || o._ctx !== this) return false;
    use(this);
    return Boolean(fn(o._id));
  }
  isBuffer(o) {
    return this._is(o, WebGLBuffer, gl().glIsBuffer);
  }
  isFramebuffer(o) {
    return this._is(o, WebGLFramebuffer, gl().glIsFramebuffer);
  }
  isRenderbuffer(o) {
    return this._is(o, WebGLRenderbuffer, gl().glIsRenderbuffer);
  }
  isTexture(o) {
    return this._is(o, WebGLTexture, gl().glIsTexture);
  }
  isProgram(o) {
    return this._is(o, WebGLProgram, gl().glIsProgram);
  }
  isShader(o) {
    return this._is(o, WebGLShader, gl().glIsShader);
  }

  // Bindings
  bindBuffer(target, buffer) {
    use(this);
    gl().glBindBuffer(target, buffer?._id ?? 0);
  }
  bindFramebuffer(target, framebuffer) {
    use(this);
    gl().glBindFramebuffer(target, framebuffer?._id ?? this._fbo);
    if (target !== GL.READ_FRAMEBUFFER) this._defaultBound = !framebuffer;
  }
  bindRenderbuffer(target, renderbuffer) {
    use(this);
    gl().glBindRenderbuffer(target, renderbuffer?._id ?? 0);
  }
  bindTexture(target, texture) {
    use(this);
    gl().glBindTexture(target, texture?._id ?? 0);
  }
  _bindVertexArray(vao) {
    use(this);
    gl().glBindVertexArray(vao?._id ?? this._vao);
  }
  _createVertexArray() {
    return this._create(WebGLVertexArrayObject, () => genOne(gl().glGenVertexArrays));
  }
  _deleteVertexArray(o) {
    this._delete(o, (id) => deleteOne(gl().glDeleteVertexArrays, id));
  }
  _isVertexArray(o) {
    return this._is(o, WebGLVertexArrayObject, gl().glIsVertexArray);
  }

  // State
  clearColor(r, g, b, a) {
    use(this);
    gl().glClearColor(r, g, b, a);
  }
  clearDepth(depth) {
    use(this);
    gl().glClearDepth(depth);
  }
  depthRange(near, far) {
    use(this);
    gl().glDepthRange(near, far);
  }
  colorMask(r, g, b, a) {
    use(this);
    gl().glColorMask(+!!r, +!!g, +!!b, +!!a);
  }
  depthMask(flag) {
    use(this);
    gl().glDepthMask(+!!flag);
  }
  sampleCoverage(value, invert) {
    use(this);
    gl().glSampleCoverage(value, +!!invert);
  }
  // Core GL draws only 1px lines.
  lineWidth(width) {
    use(this);
    gl().glLineWidth(Math.min(1, Math.max(0, width)));
  }
  isEnabled(cap) {
    use(this);
    return Boolean(gl().glIsEnabled(cap));
  }
  pixelStorei(pname, param) {
    if (pname === UNPACK_FLIP_Y) return void (this._unpack.flipY = Boolean(param));
    if (pname === UNPACK_PREMULTIPLY_ALPHA) return void (this._unpack.premultiply = Boolean(param));
    if (pname === UNPACK_COLORSPACE_CONVERSION) return void (this._unpack.colorspace = param);
    use(this);
    gl().glPixelStorei(pname, Number(param));
  }

  // Drawing
  clear(mask) {
    use(this);
    gl().glClear(mask);
    this._drew();
  }
  drawArrays(mode, first, count) {
    use(this);
    gl().glDrawArrays(mode, first, count);
    this._drew();
  }
  drawElements(mode, count, type, offset) {
    use(this);
    gl().glDrawElements(mode, count, type, offset);
    this._drew();
  }
  _drawArraysInstanced(mode, first, count, instances) {
    use(this);
    gl().glDrawArraysInstanced(mode, first, count, instances);
    this._drew();
  }
  _drawElementsInstanced(mode, count, type, offset, instances) {
    use(this);
    gl().glDrawElementsInstanced(mode, count, type, offset, instances);
    this._drew();
  }
  // To the default framebuffer, WebGL's BACK is its one color attachment.
  _drawBuffers(buffers) {
    use(this);
    const list = Uint32Array.from(buffers, (b) => (b === BACK ? GL.COLOR_ATTACHMENT0 : b));
    gl().glDrawBuffers(list.length, list);
  }

  // Buffers
  bufferData(target, data, usage, srcOffset = 0, length = 0) {
    use(this);
    if (typeof data === "number") return void gl().glBufferData(target, data, null, usage);
    const view = ArrayBuffer.isView(data) && !(data instanceof DataView) ? slice(data, srcOffset, length) : data;
    const b = bytes(view);
    gl().glBufferData(target, b.byteLength, b.byteLength ? b : null, usage);
  }
  bufferSubData(target, offset, data, srcOffset = 0, length = 0) {
    use(this);
    const b = bytes(ArrayBuffer.isView(data) && !(data instanceof DataView) ? slice(data, srcOffset, length) : data);
    if (b.byteLength) gl().glBufferSubData(target, offset, b.byteLength, b);
  }
  getBufferParameter(target, pname) {
    use(this);
    return getInt(gl().glGetBufferParameteriv, target, pname);
  }

  // Shaders and programs
  shaderSource(shader, source) {
    use(this);
    shader._source = String(source);
    const text = cstr(translateShader(source, shader._type === GL.VERTEX_SHADER));
    gl().glShaderSource(shader._id, 1, new BigUint64Array([BigInt(ptr(text))]), null);
  }
  getShaderSource(shader) {
    return shader?._source ?? null;
  }
  compileShader(shader) {
    use(this);
    gl().glCompileShader(shader._id);
  }
  getShaderParameter(shader, pname) {
    use(this);
    if (pname === GL.SHADER_TYPE) return shader._type;
    const v = getInt(gl().glGetShaderiv, shader._id, pname);
    return SHADER_BOOLEANS.has(pname) ? Boolean(v) : v;
  }
  _log(id, getiv, getLog) {
    use(this);
    const n = getInt(getiv, id, INFO_LOG_LENGTH);
    if (!n) return "";
    const out = new Uint8Array(n);
    getLog(id, n, null, out);
    return Buffer.from(out.subarray(0, n - 1)).toString();
  }
  getShaderInfoLog(shader) {
    return this._log(shader._id, gl().glGetShaderiv, gl().glGetShaderInfoLog);
  }
  getProgramInfoLog(program) {
    return this._log(program._id, gl().glGetProgramiv, gl().glGetProgramInfoLog);
  }
  attachShader(program, shader) {
    use(this);
    gl().glAttachShader(program._id, shader._id);
  }
  detachShader(program, shader) {
    use(this);
    gl().glDetachShader(program._id, shader._id);
  }
  getAttachedShaders(program) {
    use(this);
    const ids = new Uint32Array(16);
    gl().glGetAttachedShaders(program._id, 16, one, ids);
    return [...ids.subarray(0, one[0])].map((id) => this._wrap(WebGLShader, id));
  }
  linkProgram(program) {
    use(this);
    gl().glLinkProgram(program._id);
  }
  validateProgram(program) {
    use(this);
    gl().glValidateProgram(program._id);
  }
  useProgram(program) {
    use(this);
    gl().glUseProgram(program?._id ?? 0);
  }
  getProgramParameter(program, pname) {
    use(this);
    const v = getInt(gl().glGetProgramiv, program._id, pname);
    return PROGRAM_BOOLEANS.has(pname) ? Boolean(v) : v;
  }
  bindAttribLocation(program, index, name) {
    use(this);
    gl().glBindAttribLocation(program._id, index, cstr(name));
  }
  getAttribLocation(program, name) {
    use(this);
    return gl().glGetAttribLocation(program._id, cstr(name));
  }
  getUniformLocation(program, name) {
    use(this);
    const id = gl().glGetUniformLocation(program._id, cstr(name));
    return id < 0 ? null : new WebGLUniformLocation(program, id, name);
  }
  _active(fn, program, index) {
    use(this);
    const name = new Uint8Array(512);
    const size = new Int32Array(1);
    const type = new Uint32Array(1);
    fn(program._id, index, 512, one, size, type, name);
    if (!one[0] && !size[0]) return null;
    return new WebGLActiveInfo(Buffer.from(name.subarray(0, one[0])).toString(), size[0], type[0]);
  }
  getActiveAttrib(program, index) {
    return this._active(gl().glGetActiveAttrib, program, index);
  }
  getActiveUniform(program, index) {
    return this._active(gl().glGetActiveUniform, program, index);
  }
  getShaderPrecisionFormat(shaderType, precisionType) {
    use(this);
    const range = new Int32Array(2);
    gl().glGetShaderPrecisionFormat(shaderType, precisionType, range, one);
    return new WebGLShaderPrecisionFormat(range[0], range[1], one[0]);
  }
  // A uniform's value: its type comes from the program's active uniforms.
  getUniform(program, location) {
    use(this);
    const count = getInt(gl().glGetProgramiv, program._id, GL.ACTIVE_UNIFORMS);
    const base = location._name.replace(/\[0\]$/, "");
    let type = GL.FLOAT;
    for (let i = 0; i < count; i++) {
      const info = this.getActiveUniform(program, i);
      if (info && info.name.replace(/\[0\]$/, "") === base.replace(/\[\d+\]$/, "")) type = info.type;
    }
    const [n, Kind] = UNIFORM_SHAPES[type] ?? [1, "f"];
    const out = Kind === "f" ? new Float32Array(16) : Kind === "u" ? new Uint32Array(16) : new Int32Array(16);
    (Kind === "f" ? gl().glGetUniformfv : Kind === "u" ? gl().glGetUniformuiv : gl().glGetUniformiv)(program._id, location._id, out);
    if (Kind === "b") return n === 1 ? Boolean(out[0]) : [...out.subarray(0, n)].map(Boolean);
    return n === 1 ? out[0] : out.slice(0, n);
  }

  // Uniforms: a null location is ignored, as in WebGL.
  _uniform(fn, location, ...v) {
    if (!location) return;
    use(this);
    fn(location._id, ...v);
  }
  _uniformv(fn, Type, size, location, data, srcOffset = 0, length = 0) {
    if (!location) return;
    use(this);
    const values = slice(data instanceof Type ? data : Type.from(data), srcOffset, length);
    fn(location._id, Math.floor(values.length / size), values);
  }
  _uniformMatrix(fn, size, location, transpose, data, srcOffset = 0, length = 0) {
    if (!location) return;
    use(this);
    const values = slice(data instanceof Float32Array ? data : Float32Array.from(data), srcOffset, length);
    fn(location._id, Math.floor(values.length / size), +!!transpose, values);
  }

  // Vertex attributes
  vertexAttribPointer(index, size, type, normalized, stride, offset) {
    use(this);
    gl().glVertexAttribPointer(index, size, type, +!!normalized, stride, offset);
  }
  getVertexAttrib(index, pname) {
    use(this);
    const g = gl();
    if (pname === GL.CURRENT_VERTEX_ATTRIB) {
      const v = new Float32Array(4);
      g.glGetVertexAttribfv(index, pname, v);
      return v;
    }
    const v = getInt(g.glGetVertexAttribiv, index, pname);
    if (pname === GL.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING) return this._wrap(WebGLBuffer, v);
    if ([GL.VERTEX_ATTRIB_ARRAY_ENABLED, GL.VERTEX_ATTRIB_ARRAY_NORMALIZED, GL.VERTEX_ATTRIB_ARRAY_INTEGER].includes(pname)) return Boolean(v);
    return v;
  }
  getVertexAttribOffset(index, pname) {
    use(this);
    const v = new BigUint64Array(1);
    gl().glGetVertexAttribPointerv(index, pname, v);
    return Number(v[0]);
  }

  // Textures
  texParameteri(target, pname, param) {
    use(this);
    gl().glTexParameteri(target, pname, param);
  }
  texParameterf(target, pname, param) {
    use(this);
    gl().glTexParameterf(target, pname, param);
  }
  getTexParameter(target, pname) {
    use(this);
    if (pname === MAX_ANISOTROPY || pname === GL.TEXTURE_MIN_LOD || pname === GL.TEXTURE_MAX_LOD) {
      const v = new Float32Array(1);
      gl().glGetTexParameterfv(target, pname, v);
      return v[0];
    }
    const v = getInt(gl().glGetTexParameteriv, target, pname);
    return pname === GL.TEXTURE_IMMUTABLE_FORMAT ? Boolean(v) : v;
  }
  // The unpack state an upload of our own pixels needs: tight rows, nothing skipped, no buffer.
  _ownUnpack(run) {
    const g = gl();
    const saved = [GL.UNPACK_ALIGNMENT, GL.UNPACK_ROW_LENGTH, GL.UNPACK_SKIP_ROWS, GL.UNPACK_SKIP_PIXELS, GL.UNPACK_IMAGE_HEIGHT, GL.UNPACK_SKIP_IMAGES].map((p) => [p, getInt(g.glGetIntegerv, p)]);
    const buffer = getInt(g.glGetIntegerv, GL.PIXEL_UNPACK_BUFFER_BINDING);
    if (buffer) g.glBindBuffer(GL.PIXEL_UNPACK_BUFFER, 0);
    for (const [p] of saved) g.glPixelStorei(p, p === GL.UNPACK_ALIGNMENT ? 1 : 0);
    run();
    for (const [p, v] of saved) g.glPixelStorei(p, v);
    if (buffer) g.glBindBuffer(GL.PIXEL_UNPACK_BUFFER, buffer);
  }
  // Array pixels with WebGL's unpack flags applied (they apply to arrays too).
  _unpackArray(data, width, height, format, type, depth = 1) {
    if (!data || (!this._unpack.flipY && !this._unpack.premultiply)) return data;
    let out = bytes(data);
    if (this._unpack.premultiply && format === GL.RGBA && type === GL.UNSIGNED_BYTE) out = premultiply(out);
    if (this._unpack.flipY) {
      const alignment = getInt(gl().glGetIntegerv, GL.UNPACK_ALIGNMENT);
      const row = Math.ceil((width * pixelBytes(format, type)) / alignment) * alignment;
      const image = row * height;
      const flipped = new Uint8Array(out.length);
      for (let z = 0; z < depth; z++) flipped.set(flipRows(out.subarray(z * image, (z + 1) * image), row, height), z * image);
      out = flipped;
    }
    return out;
  }
  // A DOM source's pixels as `format`/`type` wants them, flipped and premultiplied as asked.
  _sourceData(source, format, type) {
    let { width, height, data } = sourcePixels(source);
    if (this._unpack.premultiply) data = premultiply(data);
    if (this._unpack.flipY) data = flipRows(data, width * 4, height);
    return { width, height, data: convertPixels(data, format, type) };
  }
  _swizzle(target, swizzle) {
    const t = target >= GL.TEXTURE_CUBE_MAP_POSITIVE_X && target <= GL.TEXTURE_CUBE_MAP_NEGATIVE_Z ? GL.TEXTURE_CUBE_MAP : target;
    const values = swizzle ?? [RED, GREEN, BLUE, GL.ALPHA];
    TEXTURE_SWIZZLE.forEach((p, i) => gl().glTexParameteri(t, p, values[i]));
  }
  texImage2D(target, level, internalformat, ...rest) {
    use(this);
    const g = gl();
    if (rest.length === 3) {
      // (format, type, source)
      const [format, type, source] = rest;
      const [ifmt, fmt, ty, swizzle] = coreFormat(internalformat, format, type);
      const { width, height, data } = this._sourceData(source, fmt === RED || fmt === RG ? format : fmt, ty);
      this._ownUnpack(() => g.glTexImage2D(target, level, ifmt, width, height, 0, fmt, ty, data));
      if (level === 0) this._swizzle(target, swizzle);
      return;
    }
    const [width, height, border, format, type, pixels, srcOffset = 0] = rest;
    const [ifmt, fmt, ty, swizzle] = coreFormat(internalformat, format, type);
    if (pixels != null && typeof pixels === "object" && !ArrayBuffer.isView(pixels)) {
      const { data } = this._sourceData(pixels, fmt === RED || fmt === RG ? format : fmt, ty);
      this._ownUnpack(() => g.glTexImage2D(target, level, ifmt, width, height, border, fmt, ty, data));
    } else {
      const data = typeof pixels === "number" ? pixels : this._unpackArray(pixels && slice(pixels, srcOffset), width, height, format, type);
      g.glTexImage2D(target, level, ifmt, width, height, border, fmt, ty, data ?? null);
    }
    if (level === 0) this._swizzle(target, swizzle);
  }
  texSubImage2D(target, level, xoffset, yoffset, ...rest) {
    use(this);
    const g = gl();
    if (rest.length === 3) {
      const [format, type, source] = rest;
      const [, fmt, ty] = coreFormat(format, format, type);
      const { width, height, data } = this._sourceData(source, format, ty);
      return this._ownUnpack(() => g.glTexSubImage2D(target, level, xoffset, yoffset, width, height, fmt, ty, data));
    }
    const [width, height, format, type, pixels, srcOffset = 0] = rest;
    const [, fmt, ty] = coreFormat(format, format, type);
    if (pixels != null && typeof pixels === "object" && !ArrayBuffer.isView(pixels)) {
      const { data } = this._sourceData(pixels, format, ty);
      return this._ownUnpack(() => g.glTexSubImage2D(target, level, xoffset, yoffset, width, height, fmt, ty, data));
    }
    const data = typeof pixels === "number" ? pixels : this._unpackArray(slice(pixels, srcOffset), width, height, format, type);
    g.glTexSubImage2D(target, level, xoffset, yoffset, width, height, fmt, ty, data);
  }
  compressedTexImage2D(target, level, internalformat, width, height, border, data, srcOffset = 0, length = 0) {
    use(this);
    if (typeof data === "number") return void gl().glCompressedTexImage2D(target, level, internalformat, width, height, border, data, srcOffset);
    const b = bytes(slice(data, srcOffset, length));
    gl().glCompressedTexImage2D(target, level, internalformat, width, height, border, b.byteLength, b);
  }
  compressedTexSubImage2D(target, level, x, y, width, height, format, data, srcOffset = 0, length = 0) {
    use(this);
    if (typeof data === "number") return void gl().glCompressedTexSubImage2D(target, level, x, y, width, height, format, data, srcOffset);
    const b = bytes(slice(data, srcOffset, length));
    gl().glCompressedTexSubImage2D(target, level, x, y, width, height, format, b.byteLength, b);
  }

  // Framebuffers and renderbuffers
  framebufferTexture2D(target, attachment, textarget, texture, level) {
    use(this);
    gl().glFramebufferTexture2D(target, attachment, textarget, texture?._id ?? 0, level);
  }
  framebufferRenderbuffer(target, attachment, renderbuffertarget, renderbuffer) {
    use(this);
    gl().glFramebufferRenderbuffer(target, attachment, renderbuffertarget, renderbuffer?._id ?? 0);
  }
  // WebGL1's DEPTH_STENCIL renderbuffer is a packed 24/8 one.
  renderbufferStorage(target, internalformat, width, height) {
    use(this);
    gl().glRenderbufferStorage(target, internalformat === GL.DEPTH_STENCIL ? GL.DEPTH24_STENCIL8 : internalformat, width, height);
  }
  getRenderbufferParameter(target, pname) {
    use(this);
    return getInt(gl().glGetRenderbufferParameteriv, target, pname);
  }
  getFramebufferAttachmentParameter(target, attachment, pname) {
    use(this);
    // The default framebuffer's attachments have WebGL names.
    const bound = this._defaultBound !== false || target === GL.READ_FRAMEBUFFER;
    const a = bound && attachment === BACK ? GL.COLOR_ATTACHMENT0 : bound && attachment === GL.DEPTH ? GL.DEPTH_ATTACHMENT : bound && attachment === GL.STENCIL ? GL.STENCIL_ATTACHMENT : attachment;
    const v = getInt(gl().glGetFramebufferAttachmentParameteriv, target, a, pname);
    if (pname !== GL.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME) return v;
    const kind = getInt(gl().glGetFramebufferAttachmentParameteriv, target, a, GL.FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE);
    return this._wrap(kind === GL.TEXTURE ? WebGLTexture : WebGLRenderbuffer, v);
  }
  checkFramebufferStatus(target) {
    use(this);
    return gl().glCheckFramebufferStatus(target);
  }
  // Reading the default framebuffer reads its resolved pixels.
  readPixels(x, y, width, height, format, type, dst, dstOffset = 0) {
    use(this);
    const g = gl();
    const restore = getInt(g.glGetIntegerv, GL.READ_FRAMEBUFFER_BINDING) === this._fbo ? this._readDefault() : null;
    g.glReadPixels(x, y, width, height, format, type, typeof dst === "number" ? dst : slice(dst, dstOffset));
    restore?.();
  }
}

// [components, kind] per uniform type: f float, i int, u uint, b bool.
const UNIFORM_SHAPES = {
  [GL.FLOAT]: [1, "f"], [GL.FLOAT_VEC2]: [2, "f"], [GL.FLOAT_VEC3]: [3, "f"], [GL.FLOAT_VEC4]: [4, "f"],
  [GL.FLOAT_MAT2]: [4, "f"], [GL.FLOAT_MAT3]: [9, "f"], [GL.FLOAT_MAT4]: [16, "f"],
  [GL.FLOAT_MAT2x3]: [6, "f"], [GL.FLOAT_MAT2x4]: [8, "f"], [GL.FLOAT_MAT3x2]: [6, "f"], [GL.FLOAT_MAT3x4]: [12, "f"], [GL.FLOAT_MAT4x2]: [8, "f"], [GL.FLOAT_MAT4x3]: [12, "f"],
  [GL.INT]: [1, "i"], [GL.INT_VEC2]: [2, "i"], [GL.INT_VEC3]: [3, "i"], [GL.INT_VEC4]: [4, "i"],
  [GL.UNSIGNED_INT]: [1, "u"], [GL.UNSIGNED_INT_VEC2]: [2, "u"], [GL.UNSIGNED_INT_VEC3]: [3, "u"], [GL.UNSIGNED_INT_VEC4]: [4, "u"],
  [GL.BOOL]: [1, "b"], [GL.BOOL_VEC2]: [2, "b"], [GL.BOOL_VEC3]: [3, "b"], [GL.BOOL_VEC4]: [4, "b"],
};

// Methods that pass their numbers straight through to the GL function of the same name.
const STRAIGHT = "activeTexture blendColor blendEquation blendEquationSeparate blendFunc blendFuncSeparate clearStencil cullFace depthFunc disable disableVertexAttribArray enable enableVertexAttribArray finish flush frontFace generateMipmap hint polygonOffset scissor stencilFunc stencilFuncSeparate stencilMask stencilMaskSeparate stencilOp stencilOpSeparate viewport vertexAttrib1f vertexAttrib2f vertexAttrib3f vertexAttrib4f copyTexImage2D copyTexSubImage2D";
for (const name of STRAIGHT.split(" ")) {
  const symbol = "gl" + name[0].toUpperCase() + name.slice(1);
  WebGLRenderingContextBase.prototype[name] = function (...args) {
    use(this);
    return gl()[symbol](...args);
  };
}
for (const n of [1, 2, 3, 4]) {
  for (const [suffix, Type] of [["f", Float32Array], ["i", Int32Array]]) {
    const symbol = `glUniform${n}${suffix}`;
    WebGLRenderingContextBase.prototype[`uniform${n}${suffix}`] = function (location, ...v) {
      this._uniform(gl()[symbol], location, ...v);
    };
    WebGLRenderingContextBase.prototype[`uniform${n}${suffix}v`] = function (location, data, srcOffset, length) {
      this._uniformv(gl()[symbol + "v"], Type, n, location, data, srcOffset, length);
    };
  }
  WebGLRenderingContextBase.prototype[`vertexAttrib${n}fv`] = function (index, values) {
    use(this);
    gl()[`glVertexAttrib${n}fv`](index, values instanceof Float32Array ? values : Float32Array.from(values));
  };
}
for (const [shape, size] of [["2", 4], ["3", 9], ["4", 16], ["2x3", 6], ["2x4", 8], ["3x2", 6], ["3x4", 12], ["4x2", 8], ["4x3", 12]]) {
  WebGLRenderingContextBase.prototype[`uniformMatrix${shape}fv`] = function (location, transpose, data, srcOffset, length) {
    this._uniformMatrix(gl()[`glUniformMatrix${shape}fv`], size, location, transpose, data, srcOffset, length);
  };
}

export class WebGLRenderingContext extends WebGLRenderingContextBase {
  constructor(canvas, attributes) {
    super(canvas, attributes, 1);
  }
}

export class WebGL2RenderingContext extends WebGLRenderingContextBase {
  constructor(canvas, attributes) {
    super(canvas, attributes, 2);
  }
  createVertexArray() {
    return this._createVertexArray();
  }
  deleteVertexArray(v) {
    this._deleteVertexArray(v);
  }
  isVertexArray(v) {
    return this._isVertexArray(v);
  }
  bindVertexArray(v) {
    this._bindVertexArray(v);
  }
  drawArraysInstanced(...a) {
    this._drawArraysInstanced(...a);
  }
  drawElementsInstanced(...a) {
    this._drawElementsInstanced(...a);
  }
  drawRangeElements(mode, start, end, count, type, offset) {
    use(this);
    gl().glDrawRangeElements(mode, start, end, count, type, offset);
    this._drew();
  }
  drawBuffers(buffers) {
    this._drawBuffers(buffers);
  }
  vertexAttribDivisor(index, divisor) {
    use(this);
    gl().glVertexAttribDivisor(index, divisor);
  }
  vertexAttribIPointer(index, size, type, stride, offset) {
    use(this);
    gl().glVertexAttribIPointer(index, size, type, stride, offset);
  }
  vertexAttribI4i(index, x, y, z, w) {
    use(this);
    gl().glVertexAttribI4i(index, x, y, z, w);
  }
  vertexAttribI4ui(index, x, y, z, w) {
    use(this);
    gl().glVertexAttribI4ui(index, x, y, z, w);
  }
  vertexAttribI4iv(index, v) {
    use(this);
    gl().glVertexAttribI4iv(index, Int32Array.from(v));
  }
  vertexAttribI4uiv(index, v) {
    use(this);
    gl().glVertexAttribI4uiv(index, Uint32Array.from(v));
  }
  readBuffer(src) {
    use(this);
    gl().glReadBuffer(src === BACK ? GL.COLOR_ATTACHMENT0 : src);
  }
  blitFramebuffer(...a) {
    use(this);
    gl().glBlitFramebuffer(...a);
    this._drew();
  }
  renderbufferStorageMultisample(target, samples, internalformat, width, height) {
    use(this);
    gl().glRenderbufferStorageMultisample(target, samples, internalformat, width, height);
  }
  framebufferTextureLayer(target, attachment, texture, level, layer) {
    use(this);
    gl().glFramebufferTextureLayer(target, attachment, texture?._id ?? 0, level, layer);
  }
  // Not in GL 4.1; it's only a hint.
  invalidateFramebuffer() {}
  invalidateSubFramebuffer() {}
  getInternalformatParameter(target, internalformat, pname) {
    use(this);
    const count = getInt(gl().glGetInternalformativ, target, internalformat, NUM_SAMPLE_COUNTS, 1);
    const samples = new Int32Array(count);
    if (count) gl().glGetInternalformativ(target, internalformat, pname, count, samples);
    return samples;
  }
  clearBufferfv(buffer, drawbuffer, values, srcOffset = 0) {
    use(this);
    gl().glClearBufferfv(buffer, drawbuffer, slice(values instanceof Float32Array ? values : Float32Array.from(values), srcOffset));
    this._drew();
  }
  clearBufferiv(buffer, drawbuffer, values, srcOffset = 0) {
    use(this);
    gl().glClearBufferiv(buffer, drawbuffer, slice(values instanceof Int32Array ? values : Int32Array.from(values), srcOffset));
    this._drew();
  }
  clearBufferuiv(buffer, drawbuffer, values, srcOffset = 0) {
    use(this);
    gl().glClearBufferuiv(buffer, drawbuffer, slice(values instanceof Uint32Array ? values : Uint32Array.from(values), srcOffset));
    this._drew();
  }
  clearBufferfi(buffer, drawbuffer, depth, stencil) {
    use(this);
    gl().glClearBufferfi(buffer, drawbuffer, depth, stencil);
    this._drew();
  }
  bindBufferBase(target, index, buffer) {
    use(this);
    gl().glBindBufferBase(target, index, buffer?._id ?? 0);
  }
  bindBufferRange(target, index, buffer, offset, size) {
    use(this);
    gl().glBindBufferRange(target, index, buffer?._id ?? 0, offset, size);
  }
  copyBufferSubData(readTarget, writeTarget, readOffset, writeOffset, size) {
    use(this);
    gl().glCopyBufferSubData(readTarget, writeTarget, readOffset, writeOffset, size);
  }
  getBufferSubData(target, srcByteOffset, dst, dstOffset = 0, length = 0) {
    use(this);
    const b = bytes(slice(dst, dstOffset, length));
    gl().glGetBufferSubData(target, srcByteOffset, b.byteLength, b);
  }
  getIndexedParameter(target, index) {
    use(this);
    const v = getInt(gl().glGetIntegeri_v, target, index);
    return target === GL.TRANSFORM_FEEDBACK_BUFFER_BINDING || target === GL.UNIFORM_BUFFER_BINDING ? this._wrap(WebGLBuffer, v) : v;
  }
  texStorage2D(target, levels, internalformat, width, height) {
    use(this);
    gl().glTexStorage2D(target, levels, internalformat, width, height);
  }
  texStorage3D(target, levels, internalformat, width, height, depth) {
    use(this);
    gl().glTexStorage3D(target, levels, internalformat, width, height, depth);
  }
  texImage3D(target, level, internalformat, width, height, depth, border, format, type, pixels, srcOffset = 0) {
    use(this);
    const [ifmt, fmt, ty, swizzle] = coreFormat(internalformat, format, type);
    if (pixels != null && typeof pixels === "object" && !ArrayBuffer.isView(pixels)) throw new TypeError("gpuijs: texImage3D takes arrays or a pixel unpack buffer offset, not image sources");
    const data = typeof pixels === "number" ? pixels : this._unpackArray(pixels && slice(pixels, srcOffset), width, height, format, type, depth);
    gl().glTexImage3D(target, level, ifmt, width, height, depth, border, fmt, ty, data ?? null);
    if (level === 0) this._swizzle(target, swizzle);
  }
  texSubImage3D(target, level, x, y, z, width, height, depth, format, type, pixels, srcOffset = 0) {
    use(this);
    const [, fmt, ty] = coreFormat(format, format, type);
    if (pixels != null && typeof pixels === "object" && !ArrayBuffer.isView(pixels)) throw new TypeError("gpuijs: texSubImage3D takes arrays or a pixel unpack buffer offset, not image sources");
    const data = typeof pixels === "number" ? pixels : this._unpackArray(slice(pixels, srcOffset), width, height, format, type, depth);
    gl().glTexSubImage3D(target, level, x, y, z, width, height, depth, fmt, ty, data);
  }
  copyTexSubImage3D(...a) {
    use(this);
    gl().glCopyTexSubImage3D(...a);
  }
  compressedTexImage3D(target, level, internalformat, width, height, depth, border, data, srcOffset = 0, length = 0) {
    use(this);
    if (typeof data === "number") return void gl().glCompressedTexImage3D(target, level, internalformat, width, height, depth, border, data, srcOffset);
    const b = bytes(slice(data, srcOffset, length));
    gl().glCompressedTexImage3D(target, level, internalformat, width, height, depth, border, b.byteLength, b);
  }
  compressedTexSubImage3D(target, level, x, y, z, width, height, depth, format, data, srcOffset = 0, length = 0) {
    use(this);
    if (typeof data === "number") return void gl().glCompressedTexSubImage3D(target, level, x, y, z, width, height, depth, format, data, srcOffset);
    const b = bytes(slice(data, srcOffset, length));
    gl().glCompressedTexSubImage3D(target, level, x, y, z, width, height, depth, format, b.byteLength, b);
  }
  getFragDataLocation(program, name) {
    use(this);
    return gl().glGetFragDataLocation(program._id, cstr(name));
  }
  // Uniform blocks
  getUniformBlockIndex(program, name) {
    use(this);
    return gl().glGetUniformBlockIndex(program._id, cstr(name));
  }
  uniformBlockBinding(program, index, binding) {
    use(this);
    gl().glUniformBlockBinding(program._id, index, binding);
  }
  getActiveUniformBlockName(program, index) {
    use(this);
    const name = new Uint8Array(512);
    gl().glGetActiveUniformBlockName(program._id, index, 512, one, name);
    return Buffer.from(name.subarray(0, one[0])).toString();
  }
  getActiveUniformBlockParameter(program, index, pname) {
    use(this);
    const g = gl();
    if (pname === GL.UNIFORM_BLOCK_ACTIVE_UNIFORM_INDICES) {
      const n = getInt(g.glGetActiveUniformBlockiv, program._id, index, GL.UNIFORM_BLOCK_ACTIVE_UNIFORMS);
      const out = new Uint32Array(n);
      if (n) g.glGetActiveUniformBlockiv(program._id, index, pname, out);
      return out;
    }
    const v = getInt(g.glGetActiveUniformBlockiv, program._id, index, pname);
    return pname === GL.UNIFORM_BLOCK_REFERENCED_BY_VERTEX_SHADER || pname === GL.UNIFORM_BLOCK_REFERENCED_BY_FRAGMENT_SHADER ? Boolean(v) : v;
  }
  getUniformIndices(program, names) {
    use(this);
    const strings = names.map(cstr);
    const out = new Uint32Array(names.length);
    gl().glGetUniformIndices(program._id, names.length, new BigUint64Array(strings.map((s) => BigInt(ptr(s)))), out);
    return [...out];
  }
  getActiveUniforms(program, indices, pname) {
    use(this);
    const out = new Int32Array(indices.length);
    gl().glGetActiveUniformsiv(program._id, indices.length, Uint32Array.from(indices), pname, out);
    return pname === GL.UNIFORM_IS_ROW_MAJOR ? [...out].map(Boolean) : [...out];
  }
  // Queries
  createQuery() {
    return this._create(WebGLQuery, () => genOne(gl().glGenQueries));
  }
  deleteQuery(o) {
    this._delete(o, (id) => deleteOne(gl().glDeleteQueries, id));
  }
  isQuery(o) {
    return this._is(o, WebGLQuery, gl().glIsQuery);
  }
  // Core GL 4.1 has no conservative occlusion query; the exact one answers the same question.
  beginQuery(target, query) {
    use(this);
    gl().glBeginQuery(target === ANY_SAMPLES_PASSED_CONSERVATIVE ? GL.ANY_SAMPLES_PASSED : target, query._id);
  }
  endQuery(target) {
    use(this);
    gl().glEndQuery(target === ANY_SAMPLES_PASSED_CONSERVATIVE ? GL.ANY_SAMPLES_PASSED : target);
  }
  getQuery(target, pname) {
    use(this);
    return this._wrap(WebGLQuery, getInt(gl().glGetQueryiv, target === ANY_SAMPLES_PASSED_CONSERVATIVE ? GL.ANY_SAMPLES_PASSED : target, pname));
  }
  getQueryParameter(query, pname) {
    use(this);
    const v = new Uint32Array(1);
    gl().glGetQueryObjectuiv(query._id, pname, v);
    return pname === GL.QUERY_RESULT_AVAILABLE ? Boolean(v[0]) : v[0];
  }
  // Samplers
  createSampler() {
    return this._create(WebGLSampler, () => genOne(gl().glGenSamplers));
  }
  deleteSampler(o) {
    this._delete(o, (id) => deleteOne(gl().glDeleteSamplers, id));
  }
  isSampler(o) {
    return this._is(o, WebGLSampler, gl().glIsSampler);
  }
  bindSampler(unit, sampler) {
    use(this);
    gl().glBindSampler(unit, sampler?._id ?? 0);
  }
  samplerParameteri(sampler, pname, param) {
    use(this);
    gl().glSamplerParameteri(sampler._id, pname, param);
  }
  samplerParameterf(sampler, pname, param) {
    use(this);
    gl().glSamplerParameterf(sampler._id, pname, param);
  }
  getSamplerParameter(sampler, pname) {
    use(this);
    if (pname === GL.TEXTURE_MIN_LOD || pname === GL.TEXTURE_MAX_LOD || pname === MAX_ANISOTROPY) {
      const v = new Float32Array(1);
      gl().glGetSamplerParameterfv(sampler._id, pname, v);
      return v[0];
    }
    return getInt(gl().glGetSamplerParameteriv, sampler._id, pname);
  }
  // Sync objects
  fenceSync(condition, flags) {
    use(this);
    const sync = new WebGLSync(this, gl().glFenceSync(condition, flags));
    return sync._id ? sync : null;
  }
  clientWaitSync(sync, flags, timeout) {
    use(this);
    return gl().glClientWaitSync(sync._id, flags, BigInt(Math.max(0, timeout)));
  }
  waitSync(sync, flags) {
    use(this);
    gl().glWaitSync(sync._id, flags, 0xffffffffffffffffn);
  }
  deleteSync(sync) {
    if (!sync || sync._deleted) return;
    use(this);
    sync._deleted = true;
    gl().glDeleteSync(sync._id);
  }
  isSync(sync) {
    if (!(sync instanceof WebGLSync) || sync._deleted) return false;
    use(this);
    return Boolean(gl().glIsSync(sync._id));
  }
  getSyncParameter(sync, pname) {
    use(this);
    gl().glGetSynciv(sync._id, pname, 1, null, one);
    return one[0];
  }
  // Transform feedback
  createTransformFeedback() {
    return this._create(WebGLTransformFeedback, () => genOne(gl().glGenTransformFeedbacks));
  }
  deleteTransformFeedback(o) {
    this._delete(o, (id) => deleteOne(gl().glDeleteTransformFeedbacks, id));
  }
  isTransformFeedback(o) {
    return this._is(o, WebGLTransformFeedback, gl().glIsTransformFeedback);
  }
  bindTransformFeedback(target, tf) {
    use(this);
    gl().glBindTransformFeedback(target, tf?._id ?? 0);
  }
  beginTransformFeedback(mode) {
    use(this);
    gl().glBeginTransformFeedback(mode);
  }
  endTransformFeedback() {
    use(this);
    gl().glEndTransformFeedback();
  }
  pauseTransformFeedback() {
    use(this);
    gl().glPauseTransformFeedback();
  }
  resumeTransformFeedback() {
    use(this);
    gl().glResumeTransformFeedback();
  }
  transformFeedbackVaryings(program, varyings, mode) {
    use(this);
    const strings = varyings.map(cstr);
    gl().glTransformFeedbackVaryings(program._id, varyings.length, new BigUint64Array(strings.map((s) => BigInt(ptr(s)))), mode);
  }
  getTransformFeedbackVarying(program, index) {
    return this._active((id, i, n, len, size, type, name) => gl().glGetTransformFeedbackVarying(id, i, n, len, size, type, name), program, index);
  }
}
for (const n of [1, 2, 3, 4]) {
  const symbol = `glUniform${n}ui`;
  WebGL2RenderingContext.prototype[`uniform${n}ui`] = function (location, ...v) {
    this._uniform(gl()[symbol], location, ...v);
  };
  WebGL2RenderingContext.prototype[`uniform${n}uiv`] = function (location, data, srcOffset, length) {
    this._uniformv(gl()[symbol + "v"], Uint32Array, n, location, data, srcOffset, length);
  };
}
Object.assign(WebGLRenderingContext.prototype, WEBGL1);
Object.assign(WebGLRenderingContext, WEBGL1);
Object.assign(WebGL2RenderingContext.prototype, WEBGL1, WEBGL2);
Object.assign(WebGL2RenderingContext, WEBGL1, WEBGL2);

/** `canvas.getContext(kind)` for the WebGL kinds; null for others. */
export function createWebGLContext(canvas, kind, attributes) {
  if (kind === "webgl2") return new WebGL2RenderingContext(canvas, attributes);
  if (kind === "webgl" || kind === "experimental-webgl") return new WebGLRenderingContext(canvas, attributes);
  return null;
}

/** A file path or its bytes as a Skia image, for texture uploads and `<img>`. */
export function decodeImage(file) {
  const { Image } = skia();
  const image = new Image();
  image.src = typeof file === "string" ? readFileSync(file) : file;
  return image;
}
