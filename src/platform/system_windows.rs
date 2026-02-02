//! Windows system event monitor
//!
//! Implements sleep/wake and session notifications using a hidden message window.
#![cfg(target_os = "windows")]

use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND};
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
    WTS_SESSION_LOCK, WTS_SESSION_LOGOFF, WTS_SESSION_LOGON, WTS_SESSION_REMOTE_CONNECT,
    WTS_SESSION_REMOTE_CONTROL, WTS_SESSION_REMOTE_DISCONNECT, WTS_SESSION_UNLOCK,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage,
    PostThreadMessageW, RegisterClassExW, SetWindowLongPtrW, TranslateMessage, CS_HREDRAW,
    CS_VREDRAW, GWLP_USERDATA, HWND_MESSAGE, MSG, SC_MONITORPOWER, SC_SCREENSAVE, WM_DESTROY,
    WM_POWERBROADCAST, WM_QUIT, WM_SYSCOMMAND, WM_WTSSESSION_CHANGE, WNDCLASSEXW,
};

use super::{
    SessionEvent, SleepEvent, SystemError, SystemEvent, SystemEventHandler, SystemMonitor,
};

const WINDOW_CLASS_NAME: &str = "MicroundSystemMonitorWindow\0";

struct HandlerState {
    handler: Box<dyn SystemEventHandler>,
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let user_data = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA);

    let handler = if user_data != 0 {
        Some(&mut *(user_data as *mut HandlerState))
    } else {
        None
    };

    match msg {
        WM_POWERBROADCAST => {
            if let Some(state) = handler {
                match wparam.0 as u32 {
                    PBT_APMSUSPEND => {
                        state
                            .handler
                            .on_system_event(SystemEvent::Sleep(SleepEvent::WillSleep));
                    }
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
                        state
                            .handler
                            .on_system_event(SystemEvent::Sleep(SleepEvent::DidWake));
                    }
                    _ => {}
                }
            }
            LRESULT(1)
        }
        WM_WTSSESSION_CHANGE => {
            if let Some(state) = handler {
                match wparam.0 as u32 {
                    WTS_SESSION_LOCK => {
                        state
                            .handler
                            .on_system_event(SystemEvent::Session(SessionEvent::ScreenLocking));
                    }
                    WTS_SESSION_UNLOCK => {
                        state
                            .handler
                            .on_system_event(SystemEvent::Session(SessionEvent::ScreenUnlocked));
                    }
                    WTS_SESSION_LOGOFF => {
                        state
                            .handler
                            .on_system_event(SystemEvent::Session(SessionEvent::LoggingOut));
                    }
                    WTS_SESSION_LOGON => {
                        state
                            .handler
                            .on_system_event(SystemEvent::Session(SessionEvent::SessionActivated));
                    }
                    WTS_SESSION_REMOTE_CONNECT
                    | WTS_SESSION_REMOTE_DISCONNECT
                    | WTS_SESSION_REMOTE_CONTROL => {
                        state
                            .handler
                            .on_system_event(SystemEvent::Session(SessionEvent::SwitchingUser));
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_SYSCOMMAND => {
            let cmd = (wparam.0 as u32) & 0xFFF0;
            if cmd == SC_SCREENSAVE || cmd == SC_MONITORPOWER {
                if let Some(state) = handler {
                    state
                        .handler
                        .on_system_event(SystemEvent::Session(SessionEvent::ScreenLocking));
                }
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DESTROY => {
            if user_data != 0 {
                let _ = Box::from_raw(user_data as *mut HandlerState);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn create_message_window(handler: Box<dyn SystemEventHandler>) -> Result<HWND, SystemError> {
    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null())
            .map_err(|e| SystemError::Platform(format!("GetModuleHandle failed: {:?}", e)))?;

        let class_name: Vec<u16> = WINDOW_CLASS_NAME.encode_utf16().collect();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
            ..Default::default()
        };

        if RegisterClassExW(&wc) == 0 {
            return Err(SystemError::RegistrationFailed(
                "RegisterClassExW failed".into(),
            ));
        }

        let hwnd = CreateWindowExW(
            Default::default(),
            PCWSTR::from_raw(class_name.as_ptr()),
            PCWSTR::from_raw(class_name.as_ptr()),
            Default::default(),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            hinstance.into(),
            ptr::null_mut(),
        );

        if hwnd.0 == 0 {
            return Err(SystemError::RegistrationFailed(
                "CreateWindowExW failed".into(),
            ));
        }

        let state = Box::new(HandlerState { handler });
        let state_ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);

        if !WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION).as_bool() {
            let _ = Box::from_raw(state_ptr);
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            return Err(SystemError::RegistrationFailed(
                "WTSRegisterSessionNotification failed".into(),
            ));
        }

        Ok(hwnd)
    }
}

pub struct WindowsSystemMonitor {
    handler: Option<Box<dyn SystemEventHandler>>,
    thread_handle: Option<JoinHandle<()>>,
    thread_id: Option<u32>,
    stop_signal: Arc<AtomicBool>,
}

impl WindowsSystemMonitor {
    pub fn new(handler: Box<dyn SystemEventHandler>) -> Self {
        Self {
            handler: Some(handler),
            thread_handle: None,
            thread_id: None,
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_monitor_thread(
        handler: Box<dyn SystemEventHandler>,
        stop_signal: Arc<AtomicBool>,
        ready_tx: Sender<Result<u32, SystemError>>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let thread_id = unsafe { GetCurrentThreadId() };

            let hwnd = match create_message_window(handler) {
                Ok(hwnd) => {
                    let _ = ready_tx.send(Ok(thread_id));
                    hwnd
                }
                Err(err) => {
                    let _ = ready_tx.send(Err(err));
                    return;
                }
            };

            unsafe {
                let mut msg = MSG::default();
                loop {
                    if stop_signal.load(Ordering::Acquire) {
                        break;
                    }
                    let result = GetMessageW(&mut msg, HWND(0), 0, 0);
                    if result.0 == -1 {
                        break;
                    }
                    if msg.message == WM_QUIT {
                        break;
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                let _ = WTSUnRegisterSessionNotification(hwnd);
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            }
        })
    }
}

impl SystemMonitor for WindowsSystemMonitor {
    fn start(&mut self) -> Result<(), SystemError> {
        if self.thread_handle.is_some() {
            return Ok(());
        }

        let handler = self
            .handler
            .take()
            .ok_or_else(|| SystemError::RegistrationFailed("No handler provided".into()))?;

        let (ready_tx, ready_rx) = mpsc::channel();
        let stop_signal = self.stop_signal.clone();
        let thread_handle = Self::spawn_monitor_thread(handler, stop_signal, ready_tx);

        let thread_id = ready_rx
            .recv()
            .map_err(|_| SystemError::RegistrationFailed("Monitor thread failed".into()))??;

        self.thread_id = Some(thread_id);
        self.thread_handle = Some(thread_handle);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), SystemError> {
        self.stop_signal.store(true, Ordering::Release);

        if let Some(thread_id) = self.thread_id.take() {
            unsafe {
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        Ok(())
    }

    fn power_state(&self) -> Result<super::PowerState, SystemError> {
        Err(SystemError::Unsupported)
    }

    fn thermal_state(&self) -> Result<super::ThermalState, SystemError> {
        Err(SystemError::Unsupported)
    }

    fn prevent_sleep(&self, _reason: &str) -> Result<super::SleepPrevention, SystemError> {
        Ok(super::SleepPrevention::noop())
    }
}
