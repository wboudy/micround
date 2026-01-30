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
//! # Risk Note
//!
//! This relies on undocumented Windows internals. The WorkerW technique
//! has been stable since Windows 8 but could potentially break with
//! future Windows updates.

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;
use crate::render::WallpaperRenderer;

#[cfg(all(target_os = "windows", feature = "windows"))]
use windows::{
    core::{PCSTR, PCWSTR},
    Win32::{
        Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC,
            DeleteObject, EndPaint, GetDC, ReleaseDC, SelectObject, SetDIBitsToDevice,
            BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
            PAINTSTRUCT, RGBQUAD, SRCCOPY,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
            FindWindowExW, FindWindowW, GetClassNameW, GetClientRect, GetWindowLongPtrW,
            IsWindow, PeekMessageW, RegisterClassExW, SendMessageTimeoutW, SetParent,
            SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW,
            CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HWND_BOTTOM, MSG, PM_REMOVE,
            SMTO_NORMAL, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            WINDOW_EX_STYLE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_PAINT,
            WNDCLASSEXW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
        },
    },
};

#[cfg(all(target_os = "windows", feature = "windows"))]
use std::ptr;
#[cfg(all(target_os = "windows", feature = "windows"))]
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

/// The magic message to spawn WorkerW windows
/// This is an undocumented Windows message that tells Progman to create
/// WorkerW windows in the correct Z-order for our purposes.
#[cfg(all(target_os = "windows", feature = "windows"))]
const WM_SPAWN_WORKER: u32 = 0x052C;

/// Global storage for the found WorkerW handle during enumeration
#[cfg(all(target_os = "windows", feature = "windows"))]
static FOUND_WORKERW: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(ptr::null_mut());

// ========================================================================
// Display change tracking (Windows)
// ========================================================================

#[cfg(all(target_os = "windows", feature = "windows"))]
struct WindowEventState {
    resize_pending: AtomicBool,
    new_width: AtomicU32,
    new_height: AtomicU32,
}

#[cfg(all(target_os = "windows", feature = "windows"))]
impl WindowEventState {
    fn new() -> Self {
        Self {
            resize_pending: AtomicBool::new(false),
            new_width: AtomicU32::new(0),
            new_height: AtomicU32::new(0),
        }
    }

    fn request_resize(&self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.new_width.store(width, Ordering::Release);
        self.new_height.store(height, Ordering::Release);
        self.resize_pending.store(true, Ordering::Release);
    }

    fn take_resize(&self) -> Option<(u32, u32)> {
        if !self.resize_pending.swap(false, Ordering::AcqRel) {
            return None;
        }
        let width = self.new_width.load(Ordering::Acquire);
        let height = self.new_height.load(Ordering::Acquire);
        if width == 0 || height == 0 {
            None
        } else {
            Some((width, height))
        }
    }
}

#[cfg(all(target_os = "windows", feature = "windows"))]
static WINDOW_EVENT_STATE: AtomicPtr<WindowEventState> = AtomicPtr::new(ptr::null_mut());

#[cfg(all(target_os = "windows", feature = "windows"))]
fn with_window_event_state<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&WindowEventState) -> R,
{
    let ptr = WINDOW_EVENT_STATE.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // Safety: pointer is installed in WindowsRenderer::init and cleared on shutdown.
        Some(f(unsafe { &*ptr }))
    }
}

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

    /// Double-buffer bitmap for smooth rendering
    #[cfg(all(target_os = "windows", feature = "windows"))]
    back_buffer: Option<HBITMAP>,

    /// Memory DC for double-buffering
    #[cfg(all(target_os = "windows", feature = "windows"))]
    mem_dc: Option<HDC>,

    /// Previous bitmap selected into the memory DC (for restoration)
    #[cfg(all(target_os = "windows", feature = "windows"))]
    old_bitmap: Option<HGDIOBJ>,

    /// Reusable BGRA conversion buffer to avoid per-frame allocations
    #[cfg(all(target_os = "windows", feature = "windows"))]
    bgra_buffer: Vec<u8>,

    /// Shared event state for display change notifications
    #[cfg(all(target_os = "windows", feature = "windows"))]
    event_state: Option<Box<WindowEventState>>,

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
            back_buffer: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            mem_dc: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            old_bitmap: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            bgra_buffer: Vec::new(),
            #[cfg(all(target_os = "windows", feature = "windows"))]
            event_state: None,
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
                PCWSTR::from_raw("Micround Wallpaper\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
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
            let bitmap =
                CreateCompatibleBitmap(hdc, self.width as i32, self.height as i32);
            if bitmap.0 == 0 {
                DeleteDC(mem_dc);
                ReleaseDC(window, hdc);
                return Err(RenderError::Platform(
                    "Failed to create back buffer bitmap".into(),
                ));
            }

            // Select bitmap into DC
            let old_obj = SelectObject(mem_dc, bitmap);

            ReleaseDC(window, hdc);

            self.mem_dc = Some(mem_dc);
            self.back_buffer = Some(bitmap);
            self.old_bitmap = Some(old_obj);

            Ok(())
        }
    }

    /// Pump the Windows message queue for our thread
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn poll_events(&self) {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, HWND(0), 0, 0, PM_REMOVE).as_bool() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    /// Ensure WorkerW and render window are valid (recreate if Explorer restarted)
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn ensure_workerw(&mut self) -> Result<(), RenderError> {
        let workerw_valid = self
            .workerw
            .map(|handle| unsafe { IsWindow(handle).as_bool() })
            .unwrap_or(false);

        if workerw_valid {
            return Ok(());
        }

        tracing::warn!("WorkerW handle missing or invalid; re-enumerating");

        let progman = self
            .progman
            .ok_or_else(|| RenderError::Platform("No Progman handle".into()))?;

        self.spawn_workerw(progman)?;
        let workerw = self.find_workerw()?;
        self.workerw = Some(workerw);

        self.rebuild_render_window(workerw)?;
        Ok(())
    }

    /// Recreate render window and back buffer for a new WorkerW
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn rebuild_render_window(&mut self, workerw: HWND) -> Result<(), RenderError> {
        self.cleanup_back_buffer();

        if let Some(window) = self.render_window.take() {
            unsafe {
                let _ = DestroyWindow(window);
            }
        }

        let window = self.create_render_window(workerw)?;
        self.render_window = Some(window);
        self.create_back_buffer(window)?;

        Ok(())
    }

    /// Read current WorkerW client dimensions
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn workerw_client_size(&self) -> Result<(u32, u32), RenderError> {
        let workerw = self
            .workerw
            .ok_or_else(|| RenderError::Platform("No WorkerW handle".into()))?;

        unsafe {
            let mut rect = RECT::default();
            if !GetClientRect(workerw, &mut rect).as_bool() {
                return Err(RenderError::Platform("Failed to get WorkerW rect".into()));
            }
            let width = (rect.right - rect.left) as u32;
            let height = (rect.bottom - rect.top) as u32;
            Ok((width, height))
        }
    }

    /// Resize the render window and recreate back buffer
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn resize_to(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        if self.width == width && self.height == height {
            return Ok(());
        }

        let window = self
            .render_window
            .ok_or_else(|| RenderError::Platform("No render window".into()))?;

        unsafe {
            SetWindowPos(
                window,
                HWND_BOTTOM,
                0,
                0,
                width as i32,
                height as i32,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE,
            );
        }

        self.width = width;
        self.height = height;

        self.cleanup_back_buffer();
        self.create_back_buffer(window)?;

        tracing::info!(width, height, "Windows renderer resized");
        Ok(())
    }

    /// Apply pending display change events and refresh size if needed
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn handle_display_changes(&mut self) -> Result<(), RenderError> {
        self.ensure_workerw()?;

        if let Some(state) = self.event_state.as_ref() {
            if let Some((width, height)) = state.take_resize() {
                let target = self.workerw_client_size().unwrap_or((width, height));
                self.resize_to(target.0, target.1)?;
                return Ok(());
            }
        }

        if let Ok((width, height)) = self.workerw_client_size() {
            if width != self.width || height != self.height {
                self.resize_to(width, height)?;
            }
        }

        Ok(())
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

            // Convert RGBA to BGRA for Windows using reusable buffer
            rgba_to_bgra_into(&frame.data, &mut self.bgra_buffer);

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
                self.bgra_buffer.as_ptr() as *const _,
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
            let bitmap = self.back_buffer.take();
            let old_obj = self.old_bitmap.take();
            let dc = self.mem_dc.take();

            if let (Some(dc), Some(old_obj)) = (dc, old_obj) {
                let _ = SelectObject(dc, old_obj);
                if let Some(bitmap) = bitmap {
                    DeleteObject(bitmap);
                }
                DeleteDC(dc);
            } else {
                if let Some(bitmap) = bitmap {
                    DeleteObject(bitmap);
                }
                if let Some(dc) = dc {
                    DeleteDC(dc);
                }
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

            // Step 5: Create double-buffering resources
            self.create_back_buffer(window)?;

            // Step 6: Install display change event state
            let mut event_state = Box::new(WindowEventState::new());
            let state_ptr = &mut *event_state as *mut WindowEventState;
            WINDOW_EVENT_STATE.store(state_ptr, Ordering::SeqCst);
            self.event_state = Some(event_state);

            self.initialized = true;
            tracing::info!(
                "Windows WorkerW renderer initialized: {}x{}",
                self.width,
                self.height
            );

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
            self.poll_events();
            self.handle_display_changes()?;
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
            // Clean up back buffer resources
            self.cleanup_back_buffer();

            if self.event_state.is_some() {
                WINDOW_EVENT_STATE.store(ptr::null_mut(), Ordering::SeqCst);
                self.event_state = None;
            }

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
        tracing::info!("Windows WorkerW renderer shutdown complete");
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
        WM_DISPLAYCHANGE => {
            let (width, height) = size_from_lparam(lparam);
            let _ = with_window_event_state(|state| {
                state.request_resize(width, height);
            });
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let rect_ptr = lparam.0 as *const RECT;
            if !rect_ptr.is_null() {
                let rect = *rect_ptr;
                let width = (rect.right - rect.left) as u32;
                let height = (rect.bottom - rect.top) as u32;
                let _ = with_window_event_state(|state| {
                    state.request_resize(width, height);
                });
            }
            LRESULT(0)
        }
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

#[cfg(all(target_os = "windows", feature = "windows"))]
fn size_from_lparam(lparam: LPARAM) -> (u32, u32) {
    let value = lparam.0 as u32;
    let width = (value & 0xFFFF) as u32;
    let height = ((value >> 16) & 0xFFFF) as u32;
    (width, height)
}

/// Convert RGBA to BGRA format for Windows GDI, reusing a buffer
/// Windows bitmaps expect BGRA byte order
///
/// This version reuses the output buffer to avoid per-frame allocations.
/// At 1080p/30fps, this saves ~240MB/s of allocations.
#[cfg(all(target_os = "windows", feature = "windows"))]
fn rgba_to_bgra_into(rgba: &[u8], bgra: &mut Vec<u8>) {
    bgra.clear();
    bgra.reserve(rgba.len());

    for chunk in rgba.chunks_exact(4) {
        bgra.push(chunk[2]); // B
        bgra.push(chunk[1]); // G
        bgra.push(chunk[0]); // R
        bgra.push(chunk[3]); // A
    }
}

/// Convert RGBA to BGRA format for Windows GDI (allocating version for tests)
/// Windows bitmaps expect BGRA byte order
#[cfg(all(target_os = "windows", feature = "windows"))]
fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());
    rgba_to_bgra_into(rgba, &mut bgra);
    bgra
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
    fn test_window_event_state_resize() {
        let state = WindowEventState::new();
        assert!(state.take_resize().is_none());

        state.request_resize(1280, 720);
        assert_eq!(state.take_resize(), Some((1280, 720)));
        assert!(state.take_resize().is_none());

        state.request_resize(0, 0);
        assert!(state.take_resize().is_none());
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
            eprintln!("Windows init failed (may need desktop session): {:?}", result);
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
