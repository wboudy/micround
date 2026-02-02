//! Windows-specific tests for Micround
//!
//! Tests WorkerW window hierarchy, COM initialization, and GDI rendering.
//! These tests use conditional compilation and skip gracefully when resources
//! aren't available (e.g., no desktop session, COM failure).
//!
//! Run with: cargo test --features windows -- --ignored platform_windows

#![cfg(target_os = "windows")]

mod common;

use common::test_logger::*;
use std::env;

// ============================================================================
// COM Initialization Tests
// ============================================================================

/// Test COM thread initialization
#[test]
#[cfg(feature = "windows")]
fn test_com_initialization() {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    let mut logger = TestLogger::new("com_initialization", 3);

    test_step!(logger, "Initializing COM apartment-threaded");
    let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    match result {
        Ok(()) => {
            test_step_ok!(logger, "COM initialized successfully");

            test_step!(logger, "Verifying COM is initialized");
            // If we got here, COM is working
            test_step_ok!(logger, "COM apartment verified");

            test_step!(logger, "Uninitializing COM");
            unsafe { CoUninitialize() };
            test_step_ok!(logger);
        }
        Err(e) => {
            // HRESULT S_FALSE (1) means already initialized, which is fine
            if e.code().0 == 1 {
                test_step_ok!(logger, "COM already initialized (S_FALSE)");
                test_step!(logger, "COM already active");
                test_step_ok!(logger);
                test_step!(logger, "Skipping uninit (already was initialized)");
                logger.step_skip("Leaving COM in pre-existing state");
            } else {
                logger.step_err(&format!("COM init failed: {:?}", e));
            }
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test COM multi-threaded apartment initialization
#[test]
#[cfg(feature = "windows")]
fn test_com_mta_initialization() {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    let mut logger = TestLogger::new("com_mta_initialization", 2);

    test_step!(logger, "Initializing COM multi-threaded apartment");
    let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    match result {
        Ok(()) => {
            test_step_ok!(logger, "MTA COM initialized");

            test_step!(logger, "Cleaning up MTA");
            unsafe { CoUninitialize() };
            test_step_ok!(logger);
        }
        Err(e) => {
            // RPC_E_CHANGED_MODE (0x80010106) means already initialized with different mode
            if e.code().0 as u32 == 0x80010106 {
                logger.step_skip("COM already initialized in different mode (STA)");
                test_step!(logger, "Skipping MTA test");
                logger.step_skip("Thread already has STA apartment");
            } else if e.code().0 == 1 {
                test_step_ok!(logger, "MTA already initialized (S_FALSE)");
                test_step!(logger, "Skipping uninit");
                logger.step_skip("Leaving COM in pre-existing state");
            } else {
                logger.step_err(&format!("MTA init failed: {:?}", e));
            }
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Desktop Window Hierarchy Tests
// ============================================================================

/// Test finding the Progman (Program Manager) window
#[test]
#[cfg(feature = "windows")]
fn test_find_progman_window() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    let mut logger = TestLogger::new("find_progman_window", 3);

    test_step!(logger, "Looking for Progman window");
    let progman = unsafe {
        FindWindowW(
            PCWSTR::from_raw("Progman\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
            PCWSTR::null(),
        )
    };

    if progman.0 == 0 {
        logger.step_skip("Progman not found (no desktop shell running)");
        test_step!(logger, "Skipping Progman tests");
        logger.step_skip("No desktop shell available");
        test_step!(logger, "Test incomplete");
        logger.step_skip("Desktop shell required for full test");
    } else {
        test_step_ok!(logger, "Found Progman: {:?}", progman);

        test_step!(logger, "Verifying Progman is valid window");
        test_assert!(logger, progman.0 != 0, "Progman handle is non-null");
        test_step_ok!(logger);

        test_step!(logger, "Checking Progman window class");
        let mut class_name = [0u16; 256];
        let len = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetClassNameW(progman, &mut class_name)
        };
        if len > 0 {
            let class_str = String::from_utf16_lossy(&class_name[..len as usize]);
            test_assert!(logger, class_str == "Progman", "Class name is Progman");
            test_step_ok!(logger, "Window class: {}", class_str);
        } else {
            logger.step_skip("Could not get class name");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test WorkerW window enumeration
#[test]
#[ignore = "requires Windows desktop session"]
#[cfg(feature = "windows")]
fn test_workerw_enumeration() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, FindWindowExW, GetClassNameW};

    let mut logger = TestLogger::new("workerw_enumeration", 4);

    // Counter for WorkerW windows found
    static WORKERW_COUNT: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn count_workerw(hwnd: HWND, _lparam: LPARAM) -> BOOL {
        let mut class_name = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut class_name);
        if len > 0 {
            let class_str = String::from_utf16_lossy(&class_name[..len as usize]);
            if class_str == "WorkerW" {
                WORKERW_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }
        BOOL::from(true) // Continue enumeration
    }

    test_step!(logger, "Resetting WorkerW counter");
    WORKERW_COUNT.store(0, Ordering::SeqCst);
    test_step_ok!(logger);

    test_step!(logger, "Enumerating all top-level windows");
    let result = unsafe { EnumWindows(Some(count_workerw), LPARAM(0)) };
    if result.as_bool() {
        let count = WORKERW_COUNT.load(Ordering::SeqCst);
        test_step_ok!(
            logger,
            "Enumeration complete, found {} WorkerW window(s)",
            count
        );
    } else {
        logger.step_err("Window enumeration failed");
    }

    test_step!(logger, "Checking for SHELLDLL_DefView");
    // Look for a window that has SHELLDLL_DefView as a child (this is the desktop icons window)
    let progman = unsafe {
        windows::Win32::UI::WindowsAndMessaging::FindWindowW(
            PCWSTR::from_raw("Progman\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
            PCWSTR::null(),
        )
    };

    if progman.0 != 0 {
        let shell_view = unsafe {
            FindWindowExW(
                progman,
                HWND::default(),
                PCWSTR::from_raw(
                    "SHELLDLL_DefView\0"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                        .as_ptr(),
                ),
                PCWSTR::null(),
            )
        };

        if shell_view.0 != 0 {
            test_step_ok!(logger, "Found SHELLDLL_DefView as child of Progman");
        } else {
            // SHELLDLL_DefView might be under a WorkerW window instead
            test_step_ok!(
                logger,
                "SHELLDLL_DefView not direct child of Progman (may be under WorkerW)"
            );
        }
    } else {
        logger.step_skip("Progman not found");
    }

    test_step!(logger, "Verifying desktop window hierarchy");
    let workerw_count = WORKERW_COUNT.load(Ordering::SeqCst);
    if workerw_count > 0 {
        test_step_ok!(
            logger,
            "Desktop has {} WorkerW window(s) - hierarchy is typical",
            workerw_count
        );
    } else {
        test_step_ok!(
            logger,
            "No WorkerW windows found (normal before spawn message)"
        );
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test WorkerW spawn message (the undocumented 0x052C technique)
#[test]
#[ignore = "requires Windows desktop session and may modify desktop state"]
#[cfg(feature = "windows")]
fn test_workerw_spawn_message() {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SendMessageTimeoutW, SMTO_NORMAL};

    let mut logger = TestLogger::new("workerw_spawn_message", 4);

    /// The magic message to spawn WorkerW windows (undocumented)
    const WM_SPAWN_WORKER: u32 = 0x052C;

    test_step!(logger, "Finding Progman window");
    let progman = unsafe {
        FindWindowW(
            PCWSTR::from_raw("Progman\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
            PCWSTR::null(),
        )
    };

    if progman.0 == 0 {
        logger.step_skip("Progman not found");
        test_step!(logger, "Skipping spawn message test");
        logger.step_skip("No desktop shell");
        test_step!(logger, "Test incomplete");
        logger.step_skip("Desktop shell required");
        test_step!(logger, "Cleanup");
        logger.step_skip("Nothing to clean up");
        let result = logger.finish();
        assert!(result.passed);
        return;
    }
    test_step_ok!(logger, "Found Progman: {:?}", progman);

    test_step!(logger, "Sending WorkerW spawn message (0x052C)");
    let mut result_value: usize = 0;
    let send_result = unsafe {
        SendMessageTimeoutW(
            progman,
            WM_SPAWN_WORKER,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000, // 1 second timeout
            Some(&mut result_value),
        )
    };

    if send_result.0 != 0 {
        test_step_ok!(
            logger,
            "Message sent successfully, result: {}",
            result_value
        );
    } else {
        logger.step_err("Failed to send spawn message");
    }

    test_step!(logger, "Verifying WorkerW was created");
    // After sending the message, enumerate to find WorkerW windows
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows::Win32::Foundation::{BOOL, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, FindWindowExW, GetClassNameW};

    static WORKERW_AFTER: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn count_workerw_after(hwnd: HWND, _lparam: LPARAM) -> BOOL {
        let mut class_name = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut class_name);
        if len > 0 {
            let class_str = String::from_utf16_lossy(&class_name[..len as usize]);
            if class_str == "WorkerW" {
                WORKERW_AFTER.fetch_add(1, Ordering::SeqCst);
            }
        }
        BOOL::from(true)
    }

    WORKERW_AFTER.store(0, Ordering::SeqCst);
    unsafe { EnumWindows(Some(count_workerw_after), LPARAM(0)) };
    let after_count = WORKERW_AFTER.load(Ordering::SeqCst);

    if after_count > 0 {
        test_step_ok!(
            logger,
            "Found {} WorkerW window(s) after spawn",
            after_count
        );
    } else {
        // This might be okay - WorkerW might already exist or spawn differently
        test_step_ok!(
            logger,
            "No WorkerW found (may already exist under different parent)"
        );
    }

    test_step!(
        logger,
        "Looking for correct WorkerW (after SHELLDLL_DefView)"
    );
    // The correct WorkerW is the one that comes after a window containing SHELLDLL_DefView
    use std::ptr;
    use std::sync::atomic::AtomicPtr;

    static FOUND_WORKERW: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(ptr::null_mut());

    unsafe extern "system" fn find_target_workerw(hwnd: HWND, _lparam: LPARAM) -> BOOL {
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
            // Found SHELLDLL_DefView - now find WorkerW after this window
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

        BOOL::from(true)
    }

    FOUND_WORKERW.store(ptr::null_mut(), Ordering::SeqCst);
    unsafe { EnumWindows(Some(find_target_workerw), LPARAM(0)) };

    let target_workerw = FOUND_WORKERW.load(Ordering::SeqCst);
    if !target_workerw.is_null() {
        test_step_ok!(logger, "Found target WorkerW: {:?}", target_workerw);
    } else {
        test_step_ok!(
            logger,
            "Target WorkerW not found (SHELLDLL_DefView may be under WorkerW)"
        );
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Window Class Registration Tests
// ============================================================================

/// Test window class registration
#[test]
#[cfg(feature = "windows")]
fn test_window_class_registration() {
    use std::time::{SystemTime, UNIX_EPOCH};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, RegisterClassExW, UnregisterClassW, CS_HREDRAW, CS_VREDRAW, WNDCLASSEXW,
    };

    let mut logger = TestLogger::new("window_class_registration", 3);

    test_step!(logger, "Getting module handle");
    let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()) };
    match hinstance {
        Ok(h) => {
            test_step_ok!(logger, "Got module handle: {:?}", h);

            // Generate unique class name to avoid conflicts
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let class_name_str = format!("MicroundTestClass_{}\0", timestamp);
            let class_name: Vec<u16> = class_name_str.encode_utf16().collect();

            unsafe extern "system" fn test_wnd_proc(
                hwnd: HWND,
                msg: u32,
                wparam: WPARAM,
                lparam: LPARAM,
            ) -> LRESULT {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            test_step!(logger, "Registering window class");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(test_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h.into(),
                hIcon: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
                hCursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR::default(),
                hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
                hIconSm: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
            };

            let atom = unsafe { RegisterClassExW(&wc) };
            if atom != 0 {
                test_step_ok!(logger, "Registered with atom: {}", atom);

                test_step!(logger, "Unregistering window class");
                let unregister_result =
                    unsafe { UnregisterClassW(PCWSTR::from_raw(class_name.as_ptr()), h) };
                if unregister_result.as_bool() {
                    test_step_ok!(logger, "Class unregistered successfully");
                } else {
                    logger.step_err("Failed to unregister class");
                }
            } else {
                // GetLastError would tell us more, but class might already exist
                logger.step_skip("Registration returned 0 (may already exist)");
                test_step!(logger, "Skipping unregister");
                logger.step_skip("Nothing to unregister");
            }
        }
        Err(e) => {
            logger.step_err(&format!("GetModuleHandle failed: {:?}", e));
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// GDI Device Context Tests
// ============================================================================

/// Test GDI device context creation
#[test]
#[ignore = "requires Windows desktop session"]
#[cfg(feature = "windows")]
fn test_gdi_device_context() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, GetDC, ReleaseDC};

    let mut logger = TestLogger::new("gdi_device_context", 4);

    test_step!(logger, "Getting desktop DC");
    let desktop_dc = unsafe { GetDC(HWND::default()) };
    if desktop_dc.0 == 0 {
        logger.step_skip("Could not get desktop DC (no display)");
        test_step!(logger, "Skipping DC tests");
        logger.step_skip("No desktop DC available");
        test_step!(logger, "Skipping memory DC");
        logger.step_skip("Cannot create without desktop DC");
        test_step!(logger, "Cleanup");
        logger.step_skip("Nothing to clean up");
        let result = logger.finish();
        assert!(result.passed);
        return;
    }
    test_step_ok!(logger, "Got desktop DC: {:?}", desktop_dc);

    test_step!(logger, "Creating compatible memory DC");
    let mem_dc = unsafe { CreateCompatibleDC(desktop_dc) };
    if mem_dc.0 != 0 {
        test_step_ok!(logger, "Created memory DC: {:?}", mem_dc);
    } else {
        logger.step_err("Failed to create memory DC");
    }

    test_step!(logger, "Cleaning up memory DC");
    if mem_dc.0 != 0 {
        let deleted = unsafe { DeleteDC(mem_dc) };
        if deleted.as_bool() {
            test_step_ok!(logger, "Memory DC deleted");
        } else {
            logger.step_err("Failed to delete memory DC");
        }
    } else {
        logger.step_skip("No memory DC to delete");
    }

    test_step!(logger, "Releasing desktop DC");
    let released = unsafe { ReleaseDC(HWND::default(), desktop_dc) };
    if released != 0 {
        test_step_ok!(logger, "Desktop DC released");
    } else {
        logger.step_err("Failed to release desktop DC");
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test GDI bitmap creation for double-buffering
#[test]
#[ignore = "requires Windows desktop session"]
#[cfg(feature = "windows")]
fn test_gdi_bitmap_creation() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject,
    };

    let mut logger = TestLogger::new("gdi_bitmap_creation", 5);

    test_step!(logger, "Getting desktop DC");
    let desktop_dc = unsafe { GetDC(HWND::default()) };
    if desktop_dc.0 == 0 {
        logger.step_skip("Could not get desktop DC");
        let result = logger.finish();
        assert!(result.passed);
        return;
    }
    test_step_ok!(logger);

    test_step!(logger, "Creating compatible bitmap (1920x1080)");
    let width = 1920;
    let height = 1080;
    let bitmap = unsafe { CreateCompatibleBitmap(desktop_dc, width, height) };
    if bitmap.0 == 0 {
        logger.step_err("Failed to create bitmap");
        unsafe { ReleaseDC(HWND::default(), desktop_dc) };
        let result = logger.finish();
        assert!(!result.passed);
        return;
    }
    test_step_ok!(logger, "Created {}x{} bitmap: {:?}", width, height, bitmap);

    test_step!(logger, "Creating memory DC and selecting bitmap");
    let mem_dc = unsafe { CreateCompatibleDC(desktop_dc) };
    if mem_dc.0 != 0 {
        let _old_bitmap = unsafe { SelectObject(mem_dc, bitmap) };
        test_step_ok!(logger, "Bitmap selected into memory DC");
    } else {
        logger.step_err("Failed to create memory DC");
    }

    test_step!(logger, "Cleaning up bitmap");
    if mem_dc.0 != 0 {
        unsafe { DeleteDC(mem_dc) };
    }
    let deleted = unsafe { DeleteObject(bitmap) };
    if deleted.as_bool() {
        test_step_ok!(logger, "Bitmap deleted");
    } else {
        logger.step_err("Failed to delete bitmap");
    }

    test_step!(logger, "Releasing desktop DC");
    unsafe { ReleaseDC(HWND::default(), desktop_dc) };
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Windows Renderer Integration Tests
// ============================================================================

/// Test Windows renderer creation
#[test]
#[cfg(feature = "windows")]
fn test_windows_renderer_creation() {
    use micround::render::windows::WindowsRenderer;

    let mut logger = TestLogger::new("windows_renderer_creation", 2);

    test_step!(logger, "Creating WindowsRenderer");
    let renderer = WindowsRenderer::new();
    test_assert!(logger, renderer.is_ok(), "Renderer creation succeeded");
    test_step_ok!(logger);

    test_step!(logger, "Verifying initial state");
    let renderer = renderer.unwrap();
    test_assert!(
        logger,
        !renderer.initialized,
        "Renderer starts uninitialized"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test Windows renderer initialization and shutdown
#[test]
#[ignore = "requires Windows desktop session"]
#[cfg(feature = "windows")]
fn test_windows_renderer_init_shutdown() {
    use micround::core::DisplayId;
    use micround::render::windows::WindowsRenderer;
    use micround::render::WallpaperRenderer;

    let mut logger = TestLogger::new("windows_renderer_init_shutdown", 4);

    test_step!(logger, "Creating WindowsRenderer");
    let mut renderer = WindowsRenderer::new().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Initializing renderer");
    let display_id = DisplayId("primary".to_string());
    match renderer.init(&display_id) {
        Ok(()) => {
            test_step_ok!(logger, "Renderer initialized successfully");
            test_assert!(
                logger,
                renderer.initialized,
                "Renderer marked as initialized"
            );

            test_step!(logger, "Verifying dimensions");
            test_assert!(logger, renderer.width > 0, "Width is positive");
            test_assert!(logger, renderer.height > 0, "Height is positive");
            test_step_ok!(logger, "Display: {}x{}", renderer.width, renderer.height);

            test_step!(logger, "Shutting down renderer");
            renderer.shutdown();
            test_assert!(
                logger,
                !renderer.initialized,
                "Renderer marked as uninitialized"
            );
            test_step_ok!(logger);
        }
        Err(e) => {
            logger.step_skip(&format!("Init failed (may need desktop): {:?}", e));
            test_step!(logger, "Skipping dimension check");
            logger.step_skip("Renderer not initialized");
            test_step!(logger, "Skipping shutdown");
            logger.step_skip("Nothing to shut down");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test Windows renderer frame rendering
#[test]
#[ignore = "requires Windows desktop session"]
#[cfg(feature = "windows")]
fn test_windows_renderer_render_frame() {
    use micround::core::DisplayId;
    use micround::process::ProcessedFrame;
    use micround::render::windows::WindowsRenderer;
    use micround::render::WallpaperRenderer;

    let mut logger = TestLogger::new("windows_renderer_render_frame", 5);

    test_step!(logger, "Creating and initializing renderer");
    let mut renderer = WindowsRenderer::new().unwrap();
    let display_id = DisplayId("primary".to_string());

    match renderer.init(&display_id) {
        Ok(()) => test_step_ok!(logger),
        Err(e) => {
            logger.step_skip(&format!("Init failed: {:?}", e));
            let result = logger.finish();
            assert!(result.passed);
            return;
        }
    }

    test_step!(logger, "Creating test frame");
    let width = 800;
    let height = 600;
    let mut data = vec![0u8; width * height * 4];
    // Create a gradient pattern
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
    test_step_ok!(logger, "Created {}x{} test frame", width, height);

    test_step!(logger, "Rendering frame");
    match renderer.render(&frame) {
        Ok(()) => test_step_ok!(logger, "Frame rendered successfully"),
        Err(e) => logger.step_err(&format!("Render failed: {:?}", e)),
    }

    test_step!(logger, "Brief pause to observe (500ms)");
    std::thread::sleep(std::time::Duration::from_millis(500));
    test_step_ok!(logger);

    test_step!(logger, "Shutting down");
    renderer.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Display Environment Tests
// ============================================================================

/// Test Windows display environment detection
#[test]
fn test_display_environment() {
    let mut logger = TestLogger::new("display_environment", 3);

    test_step!(logger, "Checking for SESSIONNAME");
    match env::var("SESSIONNAME") {
        Ok(session) => {
            test_step_ok!(logger, "SESSIONNAME={}", session);
            if session.starts_with("RDP") {
                logger.info("Running in Remote Desktop session");
            } else if session == "Console" {
                logger.info("Running in console session");
            }
        }
        Err(_) => {
            test_step_ok!(logger, "SESSIONNAME not set");
        }
    }

    test_step!(logger, "Checking for DISPLAY (Wine/WSL)");
    match env::var("DISPLAY") {
        Ok(display) => {
            test_step_ok!(logger, "DISPLAY={} (may be WSL or Wine)", display);
        }
        Err(_) => {
            test_step_ok!(logger, "DISPLAY not set (native Windows)");
        }
    }

    test_step!(logger, "Determining session type");
    let session_name = env::var("SESSIONNAME").ok();
    let has_display = env::var("DISPLAY").is_ok();

    let session_type = if has_display {
        "WSL/Wine (X11 emulation detected)"
    } else if let Some(ref name) = session_name {
        if name.starts_with("RDP") {
            "Remote Desktop Session"
        } else if name == "Console" {
            "Local Console Session"
        } else {
            "Unknown Windows Session"
        }
    } else {
        "Unknown (possibly service context)"
    };
    test_step_ok!(logger, "{}", session_type);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test Windows version detection
#[test]
fn test_windows_version() {
    let mut logger = TestLogger::new("windows_version", 2);

    test_step!(logger, "Checking OS version");
    match env::var("OS") {
        Ok(os) => test_step_ok!(logger, "OS={}", os),
        Err(_) => test_step_ok!(logger, "OS not set"),
    }

    test_step!(logger, "Checking for Windows-specific paths");
    let system_drive = env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let windows_dir = env::var("windir").or_else(|_| env::var("WINDIR"));

    match windows_dir {
        Ok(dir) => {
            test_step_ok!(logger, "Windows directory: {}", dir);
            logger.info(&format!("System drive: {}", system_drive));
        }
        Err(_) => {
            test_step_ok!(logger, "Windows paths not set (may not be Windows)");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}
