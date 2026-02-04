//! Direct3D 11 rendering for GPU-accelerated wallpaper display
//!
//! Provides low-latency frame presentation using:
//! - D3D11 device and swap chain
//! - FLIP model for minimal latency
//! - Double/triple buffering for smooth playback
//!
//! # Architecture
//!
//! ```text
//! ProcessedFrame (RGBA) -> ID3D11Texture2D -> Swap Chain -> WorkerW Window
//! ```

#[cfg(all(target_os = "windows", feature = "windows"))]
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL},
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView,
                ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE,
                D3D11_BOX, D3D11_CPU_ACCESS_WRITE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_CREATE_DEVICE_DEBUG, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_WRITE_DISCARD,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_DYNAMIC,
            },
            Dxgi::{
                CreateDXGIFactory1, IDXGIFactory2, IDXGISwapChain1, DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_PRESENT, DXGI_SAMPLE_DESC, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
                DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
            Dxgi::Common::DXGI_ALPHA_MODE_IGNORE,
        },
    },
};

use crate::core::RenderError;
use crate::process::ProcessedFrame;

/// Direct3D 11 renderer state
#[cfg(all(target_os = "windows", feature = "windows"))]
pub struct D3D11Renderer {
    /// The D3D11 device
    device: ID3D11Device,
    /// The immediate device context
    context: ID3D11DeviceContext,
    /// Swap chain for presenting frames
    swap_chain: IDXGISwapChain1,
    /// Render target view for the back buffer
    render_target: ID3D11RenderTargetView,
    /// Staging texture for uploading frame data from CPU
    staging_texture: ID3D11Texture2D,
    /// Width of the swap chain
    width: u32,
    /// Height of the swap chain
    height: u32,
    /// Feature level actually supported
    feature_level: D3D_FEATURE_LEVEL,
}

#[cfg(all(target_os = "windows", feature = "windows"))]
impl D3D11Renderer {
    /// Create a new D3D11 renderer for the given window
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self, RenderError> {
        unsafe {
            // Create D3D11 device
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            let mut feature_level = D3D_FEATURE_LEVEL_11_0;

            let feature_levels = [D3D_FEATURE_LEVEL_11_0];

            // Create device without debug layer in release, with debug in debug builds
            let flags = if cfg!(debug_assertions) {
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_DEBUG
            } else {
                D3D11_CREATE_DEVICE_BGRA_SUPPORT
            };

            D3D11CreateDevice(
                None,                    // Default adapter
                D3D_DRIVER_TYPE_HARDWARE,
                None,                    // No software device
                flags,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut feature_level),
                Some(&mut context),
            )
            .map_err(|e| RenderError::Platform(format!("D3D11CreateDevice failed: {}", e)))?;

            let device = device.ok_or_else(|| RenderError::Platform("D3D11 device is None".into()))?;
            let context = context.ok_or_else(|| RenderError::Platform("D3D11 context is None".into()))?;

            tracing::debug!("Created D3D11 device, feature level: {:?}", feature_level);

            // Create DXGI factory for swap chain
            let factory: IDXGIFactory2 = CreateDXGIFactory1()
                .map_err(|e| RenderError::Platform(format!("CreateDXGIFactory1 failed: {}", e)))?;

            // Configure swap chain for low latency
            let swap_chain_desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: false.into(),
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2, // Double buffering
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD, // Low latency FLIP model
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                Flags: 0,
            };

            // Create swap chain for HWND
            let swap_chain = factory
                .CreateSwapChainForHwnd(&device, hwnd, &swap_chain_desc, None, None)
                .map_err(|e| RenderError::Platform(format!("CreateSwapChainForHwnd failed: {}", e)))?;

            tracing::debug!("Created DXGI swap chain ({}x{}) with FLIP_DISCARD", width, height);

            // Get back buffer and create render target view
            let back_buffer: ID3D11Texture2D = swap_chain
                .GetBuffer(0)
                .map_err(|e| RenderError::Platform(format!("GetBuffer failed: {}", e)))?;

            let render_target = device
                .CreateRenderTargetView(&back_buffer, None)
                .map_err(|e| RenderError::Platform(format!("CreateRenderTargetView failed: {}", e)))?;

            // Create staging texture for uploading CPU data
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                MiscFlags: 0,
            };

            let staging_texture = device
                .CreateTexture2D(&staging_desc, None)
                .map_err(|e| RenderError::Platform(format!("CreateTexture2D (staging) failed: {}", e)))?;

            tracing::info!("D3D11 renderer initialized: {}x{} @ feature level {:?}",
                width, height, feature_level);

            Ok(Self {
                device,
                context,
                swap_chain,
                render_target,
                staging_texture,
                width,
                height,
                feature_level,
            })
        }
    }

    /// Render a processed frame to the swap chain
    pub fn render(&self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        unsafe {
            // Map the staging texture for CPU write
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&self.staging_texture, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))
                .map_err(|e| RenderError::Platform(format!("Map staging texture failed: {}", e)))?;

            // Copy frame data (convert RGBA to BGRA as we copy)
            let src = &frame.data;
            let dst = std::slice::from_raw_parts_mut(
                mapped.pData as *mut u8,
                (mapped.RowPitch * self.height) as usize,
            );

            // Handle pitch mismatch (row padding)
            let src_pitch = frame.width as usize * 4;
            let dst_pitch = mapped.RowPitch as usize;

            for y in 0..frame.height.min(self.height) as usize {
                let src_row = &src[y * src_pitch..(y + 1) * src_pitch];
                let dst_row = &mut dst[y * dst_pitch..y * dst_pitch + src_pitch.min(dst_pitch)];

                // Copy with RGBA -> BGRA conversion
                for (chunk_dst, chunk_src) in dst_row.chunks_exact_mut(4).zip(src_row.chunks_exact(4)) {
                    chunk_dst[0] = chunk_src[2]; // B
                    chunk_dst[1] = chunk_src[1]; // G
                    chunk_dst[2] = chunk_src[0]; // R
                    chunk_dst[3] = chunk_src[3]; // A
                }
            }

            self.context.Unmap(&self.staging_texture, 0);

            // Get back buffer for copy destination
            let back_buffer: ID3D11Texture2D = self.swap_chain
                .GetBuffer(0)
                .map_err(|e| RenderError::Platform(format!("GetBuffer failed: {}", e)))?;

            // Copy from staging texture to back buffer
            // Use CopySubresourceRegion to handle size mismatch
            let copy_width = frame.width.min(self.width);
            let copy_height = frame.height.min(self.height);

            let src_box = D3D11_BOX {
                left: 0,
                top: 0,
                front: 0,
                right: copy_width,
                bottom: copy_height,
                back: 1,
            };

            self.context.CopySubresourceRegion(
                &back_buffer,
                0,
                0,
                0,
                0,
                &self.staging_texture,
                0,
                Some(&src_box),
            );

            // Present with vsync (1) for smooth playback, or 0 for lowest latency
            self.swap_chain
                .Present(1, DXGI_PRESENT(0))
                .ok()
                .map_err(|e| RenderError::Platform(format!("Present failed: {}", e)))?;

            Ok(())
        }
    }

    /// Handle resize when display configuration changes
    pub fn resize(&mut self, new_width: u32, new_height: u32) -> Result<(), RenderError> {
        if new_width == self.width && new_height == self.height {
            return Ok(());
        }

        unsafe {
            tracing::debug!("D3D11 resize: {}x{} -> {}x{}",
                self.width, self.height, new_width, new_height);

            // Release the render target view (it holds a reference to the back buffer)
            // We need to drop our reference before ResizeBuffers
            drop(std::mem::replace(&mut self.render_target, std::mem::zeroed()));

            // Resize the swap chain buffers
            self.swap_chain
                .ResizeBuffers(
                    0, // Keep buffer count
                    new_width,
                    new_height,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    0,
                )
                .map_err(|e| RenderError::Platform(format!("ResizeBuffers failed: {}", e)))?;

            // Recreate render target view
            let back_buffer: ID3D11Texture2D = self.swap_chain
                .GetBuffer(0)
                .map_err(|e| RenderError::Platform(format!("GetBuffer after resize failed: {}", e)))?;

            self.render_target = self.device
                .CreateRenderTargetView(&back_buffer, None)
                .map_err(|e| RenderError::Platform(format!("CreateRenderTargetView after resize failed: {}", e)))?;

            // Recreate staging texture with new size
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: new_width,
                Height: new_height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                MiscFlags: 0,
            };

            self.staging_texture = self.device
                .CreateTexture2D(&staging_desc, None)
                .map_err(|e| RenderError::Platform(format!("CreateTexture2D after resize failed: {}", e)))?;

            self.width = new_width;
            self.height = new_height;

            tracing::info!("D3D11 resize complete: {}x{}", new_width, new_height);
            Ok(())
        }
    }

    /// Get the current swap chain dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Check if device is lost and needs recreation
    pub fn check_device_removed(&self) -> bool {
        unsafe {
            let hr = self.device.GetDeviceRemovedReason();
            hr.is_err()
        }
    }
}

/// Stub implementation for non-Windows platforms
#[cfg(not(all(target_os = "windows", feature = "windows")))]
pub struct D3D11Renderer {
    _placeholder: (),
}

#[cfg(not(all(target_os = "windows", feature = "windows")))]
impl D3D11Renderer {
    pub fn new(_hwnd: (), _width: u32, _height: u32) -> Result<Self, RenderError> {
        Err(RenderError::Platform("D3D11 not available on this platform".into()))
    }

    pub fn render(&self, _frame: &ProcessedFrame) -> Result<(), RenderError> {
        Err(RenderError::Platform("D3D11 not available on this platform".into()))
    }

    pub fn resize(&mut self, _width: u32, _height: u32) -> Result<(), RenderError> {
        Err(RenderError::Platform("D3D11 not available on this platform".into()))
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (0, 0)
    }

    pub fn check_device_removed(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_d3d11_stub_on_non_windows() {
        // On non-Windows, creation should fail gracefully
        #[cfg(not(all(target_os = "windows", feature = "windows")))]
        {
            let result = D3D11Renderer::new((), 800, 600);
            assert!(result.is_err());
        }
    }

    #[test]
    #[cfg(all(target_os = "windows", feature = "windows"))]
    #[ignore = "requires Windows with D3D11 support"]
    fn test_d3d11_creation() {
        // This test requires a valid HWND and D3D11 support
        // Run manually on Windows systems
    }
}
