//! Windows UIAutomation Enter-Capture.
//! Two-thread architecture:
//!   - Hook-thread: WH_KEYBOARD_LL + message loop, signals worker on Enter
//!   - Worker-thread: UIAutomation COM calls, event emission

use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use tauri::Manager;
use tracing::{error, warn};

#[cfg(target_os = "windows")]
use windows::{
    core::*, Win32::Foundation::*, Win32::System::Com::*, Win32::UI::Accessibility::*,
    Win32::UI::WindowsAndMessaging::*,
};

pub use crate::state::EnterCaptureState;

const VK_RETURN_CODE: u32 = 0x0D;

/// Called from paste_text() in lib.rs.
pub fn record_paste(state: &EnterCaptureState, text: &str) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if let Ok(mut guard) = state.last_paste.lock() {
        *guard = Some((text.to_owned(), now_ms));
    }
}

thread_local! {
    static HOOK_TX: RefCell<Option<mpsc::SyncSender<()>>> = const { RefCell::new(None) };
}

/// Start both threads. Call once from app setup.
pub fn start_hook_thread(app: tauri::AppHandle) {
    let (signal_tx, signal_rx) = mpsc::sync_channel::<()>(1);
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    let state = app.state::<crate::state::AppState>();
    if let Ok(mut guard) = state.enter_capture.shutdown_tx.lock() {
        *guard = Some(shutdown_tx);
    }

    let app_for_worker = app.clone();
    crate::util::spawn_guarded("enter-capture-worker", move || {
        #[cfg(target_os = "windows")]
        run_worker_loop(app_for_worker, signal_rx, shutdown_rx);
    });

    let hook_state_ref = app.clone();
    crate::util::spawn_guarded("enter-capture-hook", move || {
        #[cfg(target_os = "windows")]
        run_hook_loop(hook_state_ref, signal_tx);
    });
}

#[cfg(target_os = "windows")]
fn run_hook_loop(app: tauri::AppHandle, signal_tx: mpsc::SyncSender<()>) {
    unsafe {
        let tid = windows_sys::Win32::System::Threading::GetCurrentThreadId();
        app.state::<crate::state::AppState>()
            .enter_capture
            .hook_thread_id
            .store(tid, Ordering::Release);

        HOOK_TX.with(|cell| {
            *cell.borrow_mut() = Some(signal_tx);
        });

        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), None, 0) {
            Ok(h) => h,
            Err(e) => {
                error!("[enter-capture] SetWindowsHookExW failed: {e} - Enter-Capture is disabled");
                return;
            }
        };

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hook);
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn ll_keyboard_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode >= 0 && wparam.0 as u32 == WM_KEYDOWN {
        let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if kbd.vkCode == VK_RETURN_CODE {
            HOOK_TX.with(|cell| {
                if let Some(tx) = cell.borrow().as_ref() {
                    let _ = tx.try_send(());
                }
            });
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

#[cfg(target_os = "windows")]
fn run_worker_loop(
    app: tauri::AppHandle,
    signal_rx: mpsc::Receiver<()>,
    shutdown_rx: mpsc::Receiver<()>,
) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let automation: IUIAutomation2 =
            match CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => a,
                Err(e) => {
                    error!("[enter-capture] UIAutomation init failed: {e}");
                    CoUninitialize();
                    return;
                }
            };

        loop {
            if shutdown_rx.try_recv().is_ok()
                || matches!(
                    shutdown_rx.try_recv(),
                    Err(mpsc::TryRecvError::Disconnected)
                )
            {
                break;
            }

            match signal_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(()) => handle_enter_signal(&app, &automation),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        drop(automation);
        CoUninitialize();
    }
}

#[cfg(target_os = "windows")]
fn handle_enter_signal(app: &tauri::AppHandle, automation: &IUIAutomation2) {
    let state = app.state::<crate::state::AppState>();

    let (pasted, last_ms) = match state.enter_capture.last_paste.lock() {
        Ok(guard) => match guard.clone() {
            Some(tuple) => tuple,
            None => return,
        },
        Err(_) => return,
    };

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if last_ms == 0 || now_ms.saturating_sub(last_ms) > 60_000 {
        return;
    }

    let current_text = match unsafe { read_focused_value(automation) } {
        Some(t) => t,
        None => return,
    };
    if current_text.is_empty() || current_text == pasted {
        return;
    }

    if let Ok(mut guard) = state.enter_capture.last_paste.lock() {
        *guard = None;
    }

    #[derive(Clone, serde::Serialize)]
    struct EditPayload {
        pasted: String,
        submitted: String,
    }

    use tauri::Emitter;
    if let Err(e) = app.emit(
        "enter_capture:edit_detected",
        EditPayload {
            pasted,
            submitted: current_text,
        },
    ) {
        warn!("[enter-capture] emit failed: {e}");
    }
}

#[cfg(target_os = "windows")]
unsafe fn read_focused_value(automation: &IUIAutomation2) -> Option<String> {
    let element = automation.GetFocusedElement().ok()?;

    if let Ok(raw) = element.GetCurrentPattern(UIA_ValuePatternId) {
        if let Ok(vp) = raw.cast::<IUIAutomationValuePattern>() {
            if let Ok(bstr) = vp.CurrentValue() {
                let s = bstr.to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }

    if let Ok(raw) = element.GetCurrentPattern(UIA_TextPatternId) {
        if let Ok(tp) = raw.cast::<IUIAutomationTextPattern>() {
            if let Ok(doc) = tp.DocumentRange() {
                if let Ok(bstr) = doc.GetText(-1) {
                    let s = bstr.to_string();
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
        }
    }

    None
}

/// Called from cleanup_managed_processes in lib.rs on app exit.
pub fn shutdown(state: &EnterCaptureState) {
    if let Ok(mut guard) = state.shutdown_tx.lock() {
        drop(guard.take());
    }

    #[cfg(target_os = "windows")]
    {
        let tid = state.hook_thread_id.load(Ordering::Acquire);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }
}
