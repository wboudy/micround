//! Windows wallpaper backend
//!
//! Uses WorkerW window injection to render behind desktop icons.
//!
//! # Implementation Strategy
//!
//! The Windows desktop uses a layered architecture:
//! ```text
//! ┌─────────────────────────────────┐
//! │       Desktop Icons             │  ← SHELLDLL_DefView (top)
//! ├─────────────────────────────────┤
//! │       WorkerW (our target)      │  ← Hidden window we create/use
//! ├─────────────────────────────────┤
//! │       Wallpaper                 │  ← System wallpaper (bottom)
//! └─────────────────────────────────┘
//! ```
//!
//! # The WorkerW Trick
//!
//! 1. Send message to Progman to spawn WorkerW windows (0x052C)
//! 2. Find the WorkerW between icons and wallpaper
//! 3. Parent our window to this WorkerW
//! 4. Render to our window - appears as wallpaper
//!
//! # Rendering Backend
//!
//! Uses Direct3D 11 for GPU-accelerated rendering with:
//! - FLIP model swap chain for low latency
//! - Double buffering for smooth playback
//! - Falls back to GDI if D3D11 unavailable
//!
//! # Risk Note
//!
//! This relies on undocumented Windows internals. The WorkerW technique
//! has been stable since Windows 8 but could potentially break with
//! future Windows updates.

pub mod d3d11;

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;
use crate::render::WallpaperRenderer;

#[cfg(all(target_os = "windows", feature = "windows"))]
use self::d3d11::D3D11Renderer;

#[cfg(all(target_os = "windows", feature = "windows"))]
use windows::{
    core::{PCSTR, PCWSTR},
    Win32::{
        Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
            EndPaint, GetDC, ReleaseDC, SelectObject, SetDIBitsToDevice, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, PAINTSTRUCT, RGBQUAD, SRCCOPY,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
            FindWindowExW, FindWindowW, GetClassNameW, GetClientRect, GetWindowLongPtrW,
            PeekMessageW, RegisterClassExW, SendMessageTimeoutW, SetParent, SetWindowLongPtrW,
            SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
            GWLP_USERDATA, HWND_BOTTOM, MSG, PM_REMOVE, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOSIZE, SWP_NOZORDER, SW_SHOW, WINDOW_EX_STYLE, WM_DESTROY, WM_PAINT, WNDCLASSEXW,
            WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
            WS_POPUP, WS_VISIBLE,
        },
    },
};

#[cfg(all(target_os = "windows", feature = "windows"))]
use std::ptr;
#[cfg(all(target_os = "windows", feature = "windows"))]
use std::sync::atomic::{AtomicPtr, Ordering};

/// The magic message to spawn WorkerW windows
/// This is an undocumented Windows message that tells Progman to create
/// WorkerW windows in the correct Z-order for our purposes.
#[cfg(all(target_os = "windows", feature = "windows"))]
const WM_SPAWN_WORKER: u32 = 0x052C;

/// Global storage for the found WorkerW handle during enumeration
#[cfg(all(target_os = "windows", feature = "windows"))]
static FOUND_WORKERW: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(ptr::null_mut());

/// Windows wallpaper renderer using WorkerW technique
pub struct WindowsRenderer {
    /// Handle to the Progman window
    #[cfg(all(target_os = "windows", feature = "windows"))]
    progman: Option<HWND>,

    /// Handle to the WorkerW window where we parent our content
    #[cfg(all(target_os = "windows", feature = "windows"))]
    workerw: Option<HWND>,

    /// Our rendering window (child of WorkerW)
    #[cfg(all(target_os = "windows", feature = "windows"))]
    render_window: Option<HWND>,

    /// Direct3D 11 renderer (preferred, GPU-accelerated)
    #[cfg(all(target_os = "windows", feature = "windows"))]
    d3d11_renderer: Option<D3D11Renderer>,

    /// Double-buffer bitmap for GDI fallback rendering
    #[cfg(all(target_os = "windows", feature = "windows"))]
    back_buffer: Option<HBITMAP>,

    /// Memory DC for GDI fallback double-buffering
    #[cfg(all(target_os = "windows", feature = "windows"))]
    mem_dc: Option<HDC>,

    /// Whether to use D3D11 (true) or GDI fallback (false)
    #[cfg(all(target_os = "windows", feature = "windows"))]
    use_d3d11: bool,

    /// Reusable buffer for RGBA→BGRA conversion (GDI fallback only)
    /// Avoids allocating ~8MB per frame at 30fps (240MB/s allocation pressure)
    #[cfg(all(target_os = "windows", feature = "windows"))]
    conversion_buffer: Vec<u8>,

    /// Current display width
    width: u32,
    /// Current display height
    height: u32,

    /// Whether the renderer is initialized
    initialized: bool,

    /// Non-windows placeholder
    #[cfg(not(all(target_os = "windows", feature = "windows")))]
    _placeholder: (),
}

impl WindowsRenderer {
    /// Create a new Windows renderer
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            #[cfg(all(target_os = "windows", feature = "windows"))]
            progman: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            workerw: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            render_window: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            d3d11_renderer: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            back_buffer: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            mem_dc: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            use_d3d11: true, // Prefer D3D11 by default
            #[cfg(all(target_os = "windows", feature = "windows"))]
            conversion_buffer: Vec::new(), // Allocated lazily on first use
            width: 0,
            height: 0,
            initialized: false,
            #[cfg(not(all(target_os = "windows", feature = "windows")))]
            _placeholder: (),
        })
    }

    /// Find the Progman window
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn find_progman(&self) -> Result<HWND, RenderError> {
        unsafe {
            // Find the Progman window - the Program Manager that owns the desktop
            let progman = FindWindowW(
                PCWSTR::from_raw("Progman\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
                PCWSTR::null(),
            );

            if progman.0 == 0 {
                return Err(RenderError::Platform(
                    "Failed to find Progman window".into(),
                ));
            }

            Ok(progman)
        }
    }

    /// Send the magic message to spawn WorkerW windows
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn spawn_workerw(&self, progman: HWND) -> Result<(), RenderError> {
        unsafe {
            // Send the undocumented message that spawns WorkerW windows
            // This is the key to the technique - it creates a WorkerW window
            // in the correct Z-order position between icons and wallpaper
            let mut result: usize = 0;
            let send_result = SendMessageTimeoutW(
                progman,
                WM_SPAWN_WORKER,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut result),
            );

            if send_result.0 == 0 {
                return Err(RenderError::Platform(
                    "Failed to send WorkerW spawn message".into(),
                ));
            }

            tracing::debug!("Sent WorkerW spawn message to Progman, result: {}", result);
            Ok(())
        }
    }

    /// Find the correct WorkerW window by enumerating all windows
    ///
    /// After sending the spawn message, there may be multiple WorkerW windows.
    /// We need to find the one that:
    /// 1. Has SHELLDLL_DefView as a child (or sibling)
    /// 2. Is in the correct Z-order position
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn find_workerw(&self) -> Result<HWND, RenderError> {
        unsafe {
            // Reset the global storage
            FOUND_WORKERW.store(ptr::null_mut(), Ordering::SeqCst);

            // Enumerate all top-level windows to find the right WorkerW
            let _ = EnumWindows(Some(enum_windows_callback), LPARAM(0));

            let workerw_ptr = FOUND_WORKERW.load(Ordering::SeqCst);
            if workerw_ptr.is_null() {
                return Err(RenderError::Platform(
                    "Failed to find WorkerW window after enumeration".into(),
                ));
            }

            let workerw = HWND(workerw_ptr as isize);
            tracing::debug!("Found WorkerW window: {:?}", workerw);
            Ok(workerw)
        }
    }

    /// Create our render window as a child of WorkerW
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn create_render_window(&mut self, workerw: HWND) -> Result<HWND, RenderError> {
        unsafe {
            // Get the WorkerW window dimensions
            let mut rect = RECT::default();
            if !GetClientRect(workerw, &mut rect).as_bool() {
                return Err(RenderError::Platform(
                    "Failed to get WorkerW client rect".into(),
                ));
            }

            self.width = (rect.right - rect.left) as u32;
            self.height = (rect.bottom - rect.top) as u32;

            // Register our window class
            let class_name: Vec<u16> = "MicroundWallpaperClass\0".encode_utf16().collect();

            let hinstance = GetModuleHandleW(PCWSTR::null())
                .map_err(|e| RenderError::Platform(format!("GetModuleHandle failed: {}", e)))?;

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance.into(),
                hIcon: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
                hCursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR::default(),
                hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
                hIconSm: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
            };

            let atom = RegisterClassExW(&wc);
            if atom == 0 {
                // Class may already be registered, that's okay
                tracing::debug!("Window class registration returned 0 (may already exist)");
            }

            // Create the window as a child of WorkerW
            // This is the key - parenting to WorkerW puts us in the right Z-order
            let window = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                PCWSTR::from_raw(class_name.as_ptr()),
                PCWSTR::from_raw(
                    "Micround Wallpaper\0"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                        .as_ptr(),
                ),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                self.width as i32,
                self.height as i32,
                workerw,
                None,
                hinstance,
                None,
            );

            if window.0 == 0 {
                return Err(RenderError::Platform(
                    "Failed to create render window".into(),
                ));
            }

            // Show the window
            ShowWindow(window, SW_SHOW);

            tracing::info!(
                "Created render window: {:?} ({}x{})",
                window,
                self.width,
                self.height
            );

            Ok(window)
        }
    }

    /// Create double-buffering resources
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn create_back_buffer(&mut self, window: HWND) -> Result<(), RenderError> {
        unsafe {
            let hdc = GetDC(window);
            if hdc.0 == 0 {
                return Err(RenderError::Platform("Failed to get window DC".into()));
            }

            // Create compatible DC for double buffering
            let mem_dc = CreateCompatibleDC(hdc);
            if mem_dc.0 == 0 {
                ReleaseDC(window, hdc);
                return Err(RenderError::Platform("Failed to create memory DC".into()));
            }

            // Create compatible bitmap
            let bitmap = CreateCompatibleBitmap(hdc, self.width as i32, self.height as i32);
            if bitmap.0 == 0 {
                DeleteDC(mem_dc);
                ReleaseDC(window, hdc);
                return Err(RenderError::Platform(
                    "Failed to create back buffer bitmap".into(),
                ));
            }

            // Select bitmap into DC
            SelectObject(mem_dc, bitmap);

            ReleaseDC(window, hdc);

            self.mem_dc = Some(mem_dc);
            self.back_buffer = Some(bitmap);

            Ok(())
        }
    }

    /// Render a frame to the window
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn render_frame_to_window(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        let window = self
            .render_window
            .ok_or_else(|| RenderError::Platform("No render window".into()))?;

        let mem_dc = self
            .mem_dc
            .ok_or_else(|| RenderError::Platform("No memory DC".into()))?;

        unsafe {
            // Set up BITMAPINFO for the frame data
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: frame.width as i32,
                    // Negative height for top-down bitmap
                    biHeight: -(frame.height as i32),
                    biPlanes: 1,
                    biBitCount: 32, // RGBA
                    biCompression: BI_RGB.0 as u32,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [RGBQUAD::default()],
            };

            // Convert RGBA to BGRA using reusable buffer (avoids ~8MB allocation per frame)
            rgba_to_bgra_reuse(&frame.data, &mut self.conversion_buffer);
            let bgra_data = &self.conversion_buffer;

            // Draw to memory DC
            let result = SetDIBitsToDevice(
                mem_dc,
                0,
                0,
                frame.width,
                frame.height,
                0,
                0,
                0,
                frame.height,
                bgra_data.as_ptr() as *const _,
                &bmi,
                DIB_RGB_COLORS,
            );

            if result == 0 {
                return Err(RenderError::Platform("SetDIBitsToDevice failed".into()));
            }

            // BitBlt from memory DC to window
            let hdc = GetDC(window);
            if hdc.0 != 0 {
                BitBlt(
                    hdc,
                    0,
                    0,
                    self.width as i32,
                    self.height as i32,
                    mem_dc,
                    0,
                    0,
                    SRCCOPY,
                );
                ReleaseDC(window, hdc);
            }

            Ok(())
        }
    }

    /// Clean up double-buffering resources
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn cleanup_back_buffer(&mut self) {
        unsafe {
            if let Some(bitmap) = self.back_buffer.take() {
                DeleteObject(bitmap);
            }
            if let Some(dc) = self.mem_dc.take() {
                DeleteDC(dc);
            }
        }
    }
}

impl Default for WindowsRenderer {
    fn default() -> Self {
        Self::new().expect("WindowsRenderer creation should not fail")
    }
}

impl WallpaperRenderer for WindowsRenderer {
    fn init(&mut self, _display_id: &DisplayId) -> Result<(), RenderError> {
        #[cfg(all(target_os = "windows", feature = "windows"))]
        {
            // Step 1: Find Progman
            let progman = self.find_progman()?;
            self.progman = Some(progman);
            tracing::debug!("Found Progman window: {:?}", progman);

            // Step 2: Send magic message to spawn WorkerW
            self.spawn_workerw(progman)?;

            // Step 3: Find the correct WorkerW
            let workerw = self.find_workerw()?;
            self.workerw = Some(workerw);

            // Step 4: Create our render window as child of WorkerW
            let window = self.create_render_window(workerw)?;
            self.render_window = Some(window);

            // Step 5: Try to initialize D3D11 for GPU-accelerated rendering
            if self.use_d3d11 {
                match D3D11Renderer::new(window, self.width, self.height) {
                    Ok(d3d11) => {
                        self.d3d11_renderer = Some(d3d11);
                        tracing::info!(
                            "Windows D3D11 renderer initialized: {}x{}",
                            self.width,
                            self.height
                        );
                    }
                    Err(e) => {
                        tracing::warn!("D3D11 initialization failed, falling back to GDI: {}", e);
                        self.use_d3d11 = false;
                    }
                }
            }

            // Step 6: If D3D11 failed or disabled, create GDI back buffer
            if !self.use_d3d11 {
                self.create_back_buffer(window)?;
                tracing::info!(
                    "Windows GDI renderer initialized: {}x{}",
                    self.width,
                    self.height
                );
            }

            self.initialized = true;

            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "windows")))]
        Err(RenderError::Platform(
            "Windows renderer not available on this platform".into(),
        ))
    }

    fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        if !self.initialized {
            return Err(RenderError::Platform("Renderer not initialized".into()));
        }

        #[cfg(all(target_os = "windows", feature = "windows"))]
        {
            // Try D3D11 first (GPU-accelerated)
            if self.use_d3d11 {
                if let Some(ref d3d11) = self.d3d11_renderer {
                    // Check for device removal (GPU reset, driver crash, etc.)
                    if d3d11.check_device_removed() {
                        tracing::warn!("D3D11 device removed, attempting recovery...");
                        self.d3d11_renderer = None;

                        // Try to recreate D3D11 renderer
                        if let Some(window) = self.render_window {
                            match D3D11Renderer::new(window, self.width, self.height) {
                                Ok(new_d3d11) => {
                                    self.d3d11_renderer = Some(new_d3d11);
                                    tracing::info!("D3D11 device recovered successfully");
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "D3D11 recovery failed, falling back to GDI: {}",
                                        e
                                    );
                                    self.use_d3d11 = false;
                                    // Initialize GDI fallback
                                    self.create_back_buffer(window)?;
                                }
                            }
                        }
                    }
                }

                if let Some(ref d3d11) = self.d3d11_renderer {
                    return d3d11.render(frame);
                }
            }

            // GDI fallback
            return self.render_frame_to_window(frame);
        }

        #[cfg(not(all(target_os = "windows", feature = "windows")))]
        {
            let _ = frame;
            Err(RenderError::Platform(
                "Windows renderer not available on this platform".into(),
            ))
        }
    }

    fn restore(&mut self, _config: &AppConfig) -> Result<(), RenderError> {
        // On Windows, closing our window should restore the normal wallpaper
        // The system wallpaper is still there, just behind our window
        tracing::debug!("Restoring Windows wallpaper (closing render window)");
        Ok(())
    }

    fn shutdown(&mut self) {
        #[cfg(all(target_os = "windows", feature = "windows"))]
        {
            // Clean up D3D11 renderer first (releases swap chain, etc.)
            self.d3d11_renderer = None;

            // Clean up GDI back buffer resources
            self.cleanup_back_buffer();

            // Destroy our window
            if let Some(window) = self.render_window.take() {
                unsafe {
                    let _ = DestroyWindow(window);
                }
            }

            self.progman = None;
            self.workerw = None;
        }

        self.initialized = false;
        tracing::info!("Windows renderer shutdown complete");
    }
}

/// Window enumeration callback to find the correct WorkerW
///
/// This is called for each top-level window. We're looking for a WorkerW
/// window that has SHELLDLL_DefView as a child (or is adjacent to one).
#[cfg(all(target_os = "windows", feature = "windows"))]
unsafe extern "system" fn enum_windows_callback(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    // Look for SHELLDLL_DefView as a child of this window
    let shell_view = FindWindowExW(
        hwnd,
        HWND::default(),
        PCWSTR::from_raw(
            "SHELLDLL_DefView\0"
                .encode_utf16()
                .collect::<Vec<_>>()
                .as_ptr(),
        ),
        PCWSTR::null(),
    );

    if shell_view.0 != 0 {
        // Found SHELLDLL_DefView - now find the WorkerW that comes after this window
        // in the Z-order. This is our target WorkerW.
        let workerw = FindWindowExW(
            HWND::default(),
            hwnd,
            PCWSTR::from_raw("WorkerW\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
            PCWSTR::null(),
        );

        if workerw.0 != 0 {
            FOUND_WORKERW.store(workerw.0 as *mut std::ffi::c_void, Ordering::SeqCst);
        }
    }

    // Continue enumeration
    BOOL::from(true)
}

/// Window procedure for our render window
#[cfg(all(target_os = "windows", feature = "windows"))]
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            // We handle painting via our render function, but we need to
            // validate the paint region to prevent continuous WM_PAINT messages
            let mut ps = PAINTSTRUCT::default();
            let _hdc = BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            // Clean shutdown
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Convert RGBA to BGRA format for Windows GDI
/// Windows bitmaps expect BGRA byte order
#[cfg(all(target_os = "windows", feature = "windows"))]
fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());

    for chunk in rgba.chunks_exact(4) {
        bgra.push(chunk[2]); // B
        bgra.push(chunk[1]); // G
        bgra.push(chunk[0]); // R
        bgra.push(chunk[3]); // A
    }

    bgra
}

/// Convert RGBA to BGRA format, reusing an existing buffer
/// This avoids allocating ~8MB per frame (240MB/s at 30fps)
/// Windows bitmaps expect BGRA byte order
#[cfg(all(target_os = "windows", feature = "windows"))]
fn rgba_to_bgra_reuse(rgba: &[u8], bgra: &mut Vec<u8>) {
    // Ensure buffer has correct capacity
    bgra.clear();
    bgra.reserve(rgba.len());

    for chunk in rgba.chunks_exact(4) {
        bgra.push(chunk[2]); // B
        bgra.push(chunk[1]); // G
        bgra.push(chunk[0]); // R
        bgra.push(chunk[3]); // A
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_creation() {
        let renderer = WindowsRenderer::new();
        assert!(renderer.is_ok());

        let renderer = renderer.unwrap();
        assert!(!renderer.initialized);
    }

    #[test]
    fn test_renderer_shutdown_without_init() {
        let mut renderer = WindowsRenderer::new().unwrap();
        // Should not panic
        renderer.shutdown();
        assert!(!renderer.initialized);
    }

    #[test]
    fn test_render_without_init() {
        let mut renderer = WindowsRenderer::new().unwrap();
        let frame = ProcessedFrame::new(vec![0u8; 100 * 100 * 4], 100, 100);

        let result = renderer.render(&frame);
        assert!(result.is_err());
    }

    #[test]
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn test_rgba_to_bgra() {
        let rgba = vec![255, 128, 64, 255, 0, 0, 0, 128];
        let bgra = rgba_to_bgra(&rgba);

        assert_eq!(bgra.len(), 8);
        // First pixel: R=255, G=128, B=64, A=255 -> B=64, G=128, R=255, A=255
        assert_eq!(bgra[0], 64); // B
        assert_eq!(bgra[1], 128); // G
        assert_eq!(bgra[2], 255); // R
        assert_eq!(bgra[3], 255); // A

        // Second pixel: R=0, G=0, B=0, A=128 -> B=0, G=0, R=0, A=128
        assert_eq!(bgra[4], 0); // B
        assert_eq!(bgra[5], 0); // G
        assert_eq!(bgra[6], 0); // R
        assert_eq!(bgra[7], 128); // A
    }

    // Full integration tests require Windows and a desktop session
    // Run with: cargo test --features windows -- --ignored

    #[test]
    #[ignore = "requires Windows desktop session"]
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn test_workerw_init_and_render() {
        let mut renderer = WindowsRenderer::new().unwrap();

        // Initialize
        let result = renderer.init(&DisplayId("test".to_string()));
        if result.is_err() {
            eprintln!(
                "Windows init failed (may need desktop session): {:?}",
                result
            );
            return;
        }

        assert!(renderer.initialized);
        assert!(renderer.width > 0);
        assert!(renderer.height > 0);

        // Create a test frame (red gradient)
        let width = 800;
        let height = 600;
        let mut data = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                data[idx] = (x * 255 / width) as u8; // R
                data[idx + 1] = (y * 255 / height) as u8; // G
                data[idx + 2] = 128; // B
                data[idx + 3] = 255; // A
            }
        }

        let frame = ProcessedFrame::new(data, width as u32, height as u32);

        // Render
        let result = renderer.render(&frame);
        assert!(result.is_ok());

        // Allow some time to see the result (in interactive testing)
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Cleanup
        renderer.shutdown();
        assert!(!renderer.initialized);
    }
}
