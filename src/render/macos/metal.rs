//! Metal-accelerated rendering for macOS
//!
//! This module provides GPU-accelerated frame rendering using Apple's Metal framework.
//! Metal is required for modern macOS (OpenGL is deprecated) and provides excellent
//! performance for real-time video rendering.
//!
//! # Architecture
//!
//! The Metal renderer uses:
//! - `MTLDevice`: Represents the GPU
//! - `CAMetalLayer`: Vends drawables for rendering
//! - `MTLCommandQueue`: Submits work to GPU
//! - `MTLTexture`: Holds frame data on GPU
//! - `MTLRenderPipelineState`: Compiles shader program
//!
//! # Frame Flow
//!
//! 1. CPU frame data → MTLTexture (upload)
//! 2. CAMetalLayer → nextDrawable (get render target)
//! 3. MTLRenderCommandEncoder → draw textured quad
//! 4. Present drawable (display)
//!
//! # Platform Requirements
//!
//! - macOS 10.15+ (Catalina) or later
//! - Metal-capable GPU (all Macs since 2012)
//! - The `macos` feature must be enabled

use std::ffi::c_void;

use crate::core::RenderError;
use crate::process::ProcessedFrame;

// ============================================================================
// Metal Shader Source
// ============================================================================

/// Metal shader for rendering textured quads
///
/// This is a simple passthrough shader that:
/// - Renders a full-screen quad using vertex_id
/// - Samples the frame texture
/// - Outputs directly (no color correction)
#[cfg(all(target_os = "macos", feature = "macos"))]
const METAL_SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 texCoord;
};

// Fullscreen triangle/quad using vertex_id
vertex VertexOut vertex_main(uint vertex_id [[vertex_id]]) {
    // Generate a full-screen triangle (oversized, clipped by rasterizer)
    // vertex_id 0: (-1, -1), vertex_id 1: (3, -1), vertex_id 2: (-1, 3)
    VertexOut out;

    float2 positions[3] = {
        float2(-1.0, -1.0),
        float2( 3.0, -1.0),
        float2(-1.0,  3.0)
    };

    float2 texCoords[3] = {
        float2(0.0, 1.0),  // Bottom-left (flipped Y for correct orientation)
        float2(2.0, 1.0),  // Off-screen right
        float2(0.0, -1.0)  // Off-screen top
    };

    out.position = float4(positions[vertex_id], 0.0, 1.0);
    out.texCoord = texCoords[vertex_id];

    return out;
}

fragment float4 fragment_main(VertexOut in [[stage_in]],
                               texture2d<float> tex [[texture(0)]],
                               sampler samp [[sampler(0)]]) {
    return tex.sample(samp, in.texCoord);
}
"#;

// ============================================================================
// Metal Renderer
// ============================================================================

/// Metal-accelerated renderer for macOS
///
/// Uses CAMetalLayer and MTLDevice to render frames with GPU acceleration.
/// This provides much better performance than software rendering with NSImageView.
#[cfg(all(target_os = "macos", feature = "macos"))]
pub struct MetalRenderer {
    /// Metal device (GPU)
    device: *mut c_void,
    /// Command queue for GPU work
    command_queue: *mut c_void,
    /// Render pipeline state (compiled shaders)
    pipeline_state: *mut c_void,
    /// Texture sampler
    sampler_state: *mut c_void,
    /// Texture for frame data
    frame_texture: Option<*mut c_void>,
    /// CAMetalLayer for drawable vending
    metal_layer: *mut c_void,
    /// Frame dimensions
    width: u32,
    height: u32,
    /// Whether the renderer is initialized
    initialized: bool,
}

/// Placeholder for non-macOS platforms
#[cfg(not(all(target_os = "macos", feature = "macos")))]
pub struct MetalRenderer {
    initialized: bool,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::rc::autoreleasepool;
#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::runtime::{AnyObject, Bool};
#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::{class, msg_send};

#[cfg(all(target_os = "macos", feature = "macos"))]
impl MetalRenderer {
    /// Create a new Metal renderer
    ///
    /// This creates the Metal device and command queue but doesn't
    /// initialize rendering until a layer is attached.
    pub fn new() -> Result<Self, RenderError> {
        unsafe {
            autoreleasepool(|_| {
                // Get default Metal device
                // Note: MTLCreateSystemDefaultDevice is a C function, not an Objective-C class
                extern "C" {
                    fn MTLCreateSystemDefaultDevice() -> *mut AnyObject;
                }

                let device = MTLCreateSystemDefaultDevice();
                if device.is_null() {
                    return Err(RenderError::Gpu("No Metal device available".into()));
                }

                // Create command queue
                let command_queue: *mut AnyObject = msg_send![device, newCommandQueue];
                if command_queue.is_null() {
                    return Err(RenderError::Gpu("Failed to create command queue".into()));
                }

                Ok(Self {
                    device: device as *mut c_void,
                    command_queue: command_queue as *mut c_void,
                    pipeline_state: std::ptr::null_mut(),
                    sampler_state: std::ptr::null_mut(),
                    frame_texture: None,
                    metal_layer: std::ptr::null_mut(),
                    width: 0,
                    height: 0,
                    initialized: false,
                })
            })
        }
    }

    /// Attach to a CAMetalLayer and initialize rendering
    ///
    /// The layer should already be set up on an NSView's layer property.
    pub fn attach_to_layer(
        &mut self,
        layer: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        if layer.is_null() {
            return Err(RenderError::SurfaceCreation("Null layer provided".into()));
        }

        unsafe {
            autoreleasepool(|_| {
                let layer_obj = layer as *mut AnyObject;

                // Configure the layer
                let _: () = msg_send![layer_obj, setDevice: self.device as *mut AnyObject];
                let _: () = msg_send![layer_obj, setPixelFormat: 80u64]; // MTLPixelFormatBGRA8Unorm
                let _: () = msg_send![layer_obj, setFramebufferOnly: Bool::YES];

                self.metal_layer = layer;
                self.width = width;
                self.height = height;

                // Create render pipeline
                self.create_pipeline()?;

                // Create sampler
                self.create_sampler()?;

                self.initialized = true;
                tracing::info!(
                    "Metal renderer attached to layer: {}x{}, device={:?}",
                    width,
                    height,
                    self.device
                );

                Ok(())
            })
        }
    }

    /// Create the render pipeline with our shader
    fn create_pipeline(&mut self) -> Result<(), RenderError> {
        unsafe {
            autoreleasepool(|_| {
                let device = self.device as *mut AnyObject;

                // Create shader library from source
                let source = nsstring(METAL_SHADER_SOURCE);
                let options: *const AnyObject = std::ptr::null();
                let error: *mut *mut AnyObject = std::ptr::null_mut();

                let library: *mut AnyObject =
                    msg_send![device, newLibraryWithSource:source options:options error:error];

                if library.is_null() {
                    return Err(RenderError::Gpu("Failed to compile Metal shaders".into()));
                }

                // Get vertex and fragment functions
                let vertex_name = nsstring("vertex_main");
                let fragment_name = nsstring("fragment_main");

                let vertex_fn: *mut AnyObject = msg_send![library, newFunctionWithName: vertex_name];
                let fragment_fn: *mut AnyObject =
                    msg_send![library, newFunctionWithName: fragment_name];

                if vertex_fn.is_null() || fragment_fn.is_null() {
                    return Err(RenderError::Gpu("Failed to get shader functions".into()));
                }

                // Create pipeline descriptor
                let descriptor_class = class!(MTLRenderPipelineDescriptor);
                let descriptor: *mut AnyObject = msg_send![descriptor_class, new];

                let _: () = msg_send![descriptor, setVertexFunction: vertex_fn];
                let _: () = msg_send![descriptor, setFragmentFunction: fragment_fn];

                // Configure color attachment (matches layer's pixel format)
                let color_attachments: *mut AnyObject = msg_send![descriptor, colorAttachments];
                let attachment0: *mut AnyObject =
                    msg_send![color_attachments, objectAtIndexedSubscript: 0usize];
                let _: () = msg_send![attachment0, setPixelFormat: 80u64]; // MTLPixelFormatBGRA8Unorm

                // Create pipeline state
                let pipeline: *mut AnyObject =
                    msg_send![device, newRenderPipelineStateWithDescriptor:descriptor error:error];

                if pipeline.is_null() {
                    return Err(RenderError::Gpu("Failed to create pipeline state".into()));
                }

                self.pipeline_state = pipeline as *mut c_void;

                // Release intermediate objects
                let _: () = msg_send![library, release];
                let _: () = msg_send![vertex_fn, release];
                let _: () = msg_send![fragment_fn, release];
                let _: () = msg_send![descriptor, release];

                Ok(())
            })
        }
    }

    /// Create texture sampler
    fn create_sampler(&mut self) -> Result<(), RenderError> {
        unsafe {
            autoreleasepool(|_| {
                let device = self.device as *mut AnyObject;

                let descriptor_class = class!(MTLSamplerDescriptor);
                let descriptor: *mut AnyObject = msg_send![descriptor_class, new];

                // Linear filtering for smooth scaling
                let _: () = msg_send![descriptor, setMinFilter: 1u64]; // MTLSamplerMinMagFilterLinear
                let _: () = msg_send![descriptor, setMagFilter: 1u64];

                // Clamp to edge (don't wrap)
                let _: () = msg_send![descriptor, setSAddressMode: 2u64]; // MTLSamplerAddressModeClampToEdge
                let _: () = msg_send![descriptor, setTAddressMode: 2u64];

                let sampler: *mut AnyObject =
                    msg_send![device, newSamplerStateWithDescriptor: descriptor];

                if sampler.is_null() {
                    return Err(RenderError::Gpu("Failed to create sampler state".into()));
                }

                self.sampler_state = sampler as *mut c_void;
                let _: () = msg_send![descriptor, release];

                Ok(())
            })
        }
    }

    /// Create or update the frame texture
    fn update_texture(&mut self, frame: &ProcessedFrame) -> Result<*mut AnyObject, RenderError> {
        unsafe {
            autoreleasepool(|_| {
                let device = self.device as *mut AnyObject;

                // Check if we need a new texture (size changed)
                let needs_new = self
                    .frame_texture
                    .map(|t| {
                        let tex = t as *mut AnyObject;
                        let w: u64 = msg_send![tex, width];
                        let h: u64 = msg_send![tex, height];
                        w != frame.width as u64 || h != frame.height as u64
                    })
                    .unwrap_or(true);

                let texture = if needs_new {
                    // Release old texture
                    if let Some(old) = self.frame_texture.take() {
                        let _: () = msg_send![old as *mut AnyObject, release];
                    }

                    // Create texture descriptor
                    let descriptor_class = class!(MTLTextureDescriptor);
                    let descriptor: *mut AnyObject = msg_send![
                        descriptor_class,
                        texture2DDescriptorWithPixelFormat: 80u64  // BGRA8Unorm
                        width: frame.width as u64
                        height: frame.height as u64
                        mipmapped: Bool::NO
                    ];

                    // We'll update this texture from CPU, so it needs CPU write access
                    let _: () = msg_send![descriptor, setUsage: 1u64]; // MTLTextureUsageShaderRead

                    let texture: *mut AnyObject = msg_send![device, newTextureWithDescriptor: descriptor];

                    if texture.is_null() {
                        return Err(RenderError::Gpu("Failed to create texture".into()));
                    }

                    self.frame_texture = Some(texture as *mut c_void);
                    texture
                } else {
                    self.frame_texture.unwrap() as *mut AnyObject
                };

                // Upload frame data to texture
                // Frame data is RGBA, texture is BGRA - we need to swizzle
                // For now, assume input is already BGRA or we'll handle it in shader later
                let region = MTLRegion {
                    origin: MTLOrigin { x: 0, y: 0, z: 0 },
                    size: MTLSize {
                        width: frame.width as u64,
                        height: frame.height as u64,
                        depth: 1,
                    },
                };

                let bytes_per_row = frame.width as u64 * 4;
                let _: () = msg_send![
                    texture,
                    replaceRegion: region
                    mipmapLevel: 0u64
                    withBytes: frame.data.as_ptr() as *const c_void
                    bytesPerRow: bytes_per_row
                ];

                Ok(texture)
            })
        }
    }

    /// Render a frame to the layer
    pub fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        if !self.initialized {
            return Err(RenderError::Platform("Metal renderer not initialized".into()));
        }

        unsafe {
            autoreleasepool(|_| {
                // Update texture with frame data
                let texture = self.update_texture(frame)?;

                // Get next drawable from layer
                let layer = self.metal_layer as *mut AnyObject;
                let drawable: *mut AnyObject = msg_send![layer, nextDrawable];

                if drawable.is_null() {
                    // No drawable available - skip this frame
                    tracing::debug!("No drawable available, skipping frame");
                    return Ok(());
                }

                // Get drawable's texture
                let drawable_texture: *mut AnyObject = msg_send![drawable, texture];

                // Create command buffer
                let queue = self.command_queue as *mut AnyObject;
                let command_buffer: *mut AnyObject = msg_send![queue, commandBuffer];

                if command_buffer.is_null() {
                    return Err(RenderError::Gpu("Failed to create command buffer".into()));
                }

                // Create render pass descriptor
                let pass_descriptor_class = class!(MTLRenderPassDescriptor);
                let pass_descriptor: *mut AnyObject = msg_send![pass_descriptor_class, new];

                // Configure color attachment
                let color_attachments: *mut AnyObject = msg_send![pass_descriptor, colorAttachments];
                let attachment0: *mut AnyObject =
                    msg_send![color_attachments, objectAtIndexedSubscript: 0usize];

                let _: () = msg_send![attachment0, setTexture: drawable_texture];
                let _: () = msg_send![attachment0, setLoadAction: 2u64]; // MTLLoadActionClear
                let _: () = msg_send![attachment0, setStoreAction: 1u64]; // MTLStoreActionStore

                // Set clear color to black
                let clear_color = MTLClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 1.0,
                };
                let _: () = msg_send![attachment0, setClearColor: clear_color];

                // Create render command encoder
                let encoder: *mut AnyObject =
                    msg_send![command_buffer, renderCommandEncoderWithDescriptor: pass_descriptor];

                if encoder.is_null() {
                    let _: () = msg_send![pass_descriptor, release];
                    return Err(RenderError::Gpu("Failed to create render encoder".into()));
                }

                // Set pipeline state
                let _: () =
                    msg_send![encoder, setRenderPipelineState: self.pipeline_state as *mut AnyObject];

                // Set texture and sampler
                let _: () = msg_send![encoder, setFragmentTexture: texture atIndex: 0usize];
                let _: () =
                    msg_send![encoder, setFragmentSamplerState: self.sampler_state as *mut AnyObject atIndex: 0usize];

                // Draw fullscreen triangle (3 vertices, vertex shader generates positions)
                let _: () = msg_send![
                    encoder,
                    drawPrimitives: 3u64  // MTLPrimitiveTypeTriangle
                    vertexStart: 0u64
                    vertexCount: 3u64
                ];

                // End encoding and present
                let _: () = msg_send![encoder, endEncoding];
                let _: () = msg_send![command_buffer, presentDrawable: drawable];
                let _: () = msg_send![command_buffer, commit];

                // Cleanup
                let _: () = msg_send![pass_descriptor, release];

                Ok(())
            })
        }
    }

    /// Shutdown and release Metal resources
    pub fn shutdown(&mut self) {
        unsafe {
            if let Some(texture) = self.frame_texture.take() {
                let _: () = msg_send![texture as *mut AnyObject, release];
            }
            if !self.sampler_state.is_null() {
                let _: () = msg_send![self.sampler_state as *mut AnyObject, release];
                self.sampler_state = std::ptr::null_mut();
            }
            if !self.pipeline_state.is_null() {
                let _: () = msg_send![self.pipeline_state as *mut AnyObject, release];
                self.pipeline_state = std::ptr::null_mut();
            }
            if !self.command_queue.is_null() {
                let _: () = msg_send![self.command_queue as *mut AnyObject, release];
                self.command_queue = std::ptr::null_mut();
            }
            if !self.device.is_null() {
                let _: () = msg_send![self.device as *mut AnyObject, release];
                self.device = std::ptr::null_mut();
            }
        }
        self.metal_layer = std::ptr::null_mut();
        self.initialized = false;
        tracing::info!("Metal renderer shutdown complete");
    }

    /// Check if Metal is available on this system
    ///
    /// This function safely checks for Metal availability without panicking,
    /// even if the Metal framework isn't properly linked (e.g., in test harness).
    pub fn is_available() -> bool {
        std::panic::catch_unwind(|| unsafe {
            extern "C" {
                fn MTLCreateSystemDefaultDevice() -> *mut AnyObject;
            }
            !MTLCreateSystemDefaultDevice().is_null()
        })
        .unwrap_or(false)
    }
}

// Placeholder implementation for non-macOS
#[cfg(not(all(target_os = "macos", feature = "macos")))]
impl MetalRenderer {
    pub fn new() -> Result<Self, RenderError> {
        Err(RenderError::Platform(
            "Metal is only available on macOS".into(),
        ))
    }

    pub fn attach_to_layer(
        &mut self,
        _layer: *mut c_void,
        _width: u32,
        _height: u32,
    ) -> Result<(), RenderError> {
        Err(RenderError::Platform(
            "Metal is only available on macOS".into(),
        ))
    }

    pub fn render(&mut self, _frame: &ProcessedFrame) -> Result<(), RenderError> {
        Err(RenderError::Platform(
            "Metal is only available on macOS".into(),
        ))
    }

    pub fn shutdown(&mut self) {
        self.initialized = false;
    }

    pub fn is_available() -> bool {
        false
    }
}

impl Default for MetalRenderer {
    fn default() -> Self {
        Self::new().expect("MetalRenderer creation failed")
    }
}

// SAFETY: MetalRenderer manages Objective-C objects that must be accessed from
// the main thread for rendering operations. The Send impl is needed for the
// WallpaperRenderer trait. Callers must ensure render() is called from the main thread.
#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Send for MetalRenderer {}

// ============================================================================
// Metal Types
// ============================================================================

#[cfg(all(target_os = "macos", feature = "macos"))]
#[repr(C)]
struct MTLOrigin {
    x: u64,
    y: u64,
    z: u64,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
#[repr(C)]
struct MTLSize {
    width: u64,
    height: u64,
    depth: u64,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
#[repr(C)]
struct MTLRegion {
    origin: MTLOrigin,
    size: MTLSize,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
#[repr(C)]
#[derive(Clone, Copy)]
struct MTLClearColor {
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::encode::{Encode, Encoding};

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Encode for MTLOrigin {
    const ENCODING: Encoding =
        Encoding::Struct("MTLOrigin", &[const { Encoding::ULongLong }; 3]);
}

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Encode for MTLSize {
    const ENCODING: Encoding = Encoding::Struct("MTLSize", &[const { Encoding::ULongLong }; 3]);
}

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Encode for MTLRegion {
    const ENCODING: Encoding =
        Encoding::Struct("MTLRegion", &[MTLOrigin::ENCODING, MTLSize::ENCODING]);
}

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Encode for MTLClearColor {
    const ENCODING: Encoding = Encoding::Struct("MTLClearColor", &[const { Encoding::Double }; 4]);
}

// ============================================================================
// Helper Functions
// ============================================================================

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe fn nsstring(s: &str) -> *const AnyObject {
    let cstring = std::ffi::CString::new(s).unwrap();
    msg_send![class!(NSString), stringWithUTF8String: cstring.as_ptr()]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metal_availability_check() {
        // This should run on any platform without panicking
        let _available = MetalRenderer::is_available();
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn test_metal_renderer_creation() {
        if !MetalRenderer::is_available() {
            eprintln!("Metal not available, skipping test");
            return;
        }

        let renderer = MetalRenderer::new();
        assert!(renderer.is_ok());

        let mut renderer = renderer.unwrap();
        assert!(!renderer.initialized);

        renderer.shutdown();
    }

    #[test]
    #[cfg(not(all(target_os = "macos", feature = "macos")))]
    fn test_metal_not_available_non_macos() {
        assert!(!MetalRenderer::is_available());
        let renderer = MetalRenderer::new();
        assert!(renderer.is_err());
    }

    #[test]
    fn test_render_without_init() {
        #[cfg(all(target_os = "macos", feature = "macos"))]
        {
            if !MetalRenderer::is_available() {
                return;
            }

            let mut renderer = MetalRenderer::new().unwrap();
            let frame = ProcessedFrame::new(vec![0u8; 100 * 100 * 4], 100, 100);

            let result = renderer.render(&frame);
            assert!(result.is_err());
        }
    }
}
