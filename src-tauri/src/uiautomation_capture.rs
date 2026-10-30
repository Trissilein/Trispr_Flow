//! Windows UIAutomation Enter-Capture and re-paste gesture.
//! Two-thread architecture:
//!   - Hook-thread: WH_KEYBOARD_LL / WH_MOUSE_LL + message loop
//!   - Worker-thread: UIAutomation COM calls, event emission, re-paste

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
#[cfg(target_os = "windows")]
use crate::state::TargetIdentity;

const VK_RETURN_CODE: u32 = 0x0D;
const VK_CONTROL_CODE: u32 = 0x11;
const VK_MENU_CODE: u32 = 0x12;
const VK_LCONTROL_CODE: u32 = 0xA2;
const VK_RCONTROL_CODE: u32 = 0xA3;
const VK_LMENU_CODE: u32 = 0xA4;
const VK_RMENU_CODE: u32 = 0xA5;
const WM_KEYDOWN_CODE: u32 = 0x0100;
const WM_KEYUP_CODE: u32 = 0x0101;
const WM_SYSKEYDOWN_CODE: u32 = 0x0104;
const WM_SYSKEYUP_CODE: u32 = 0x0105;
const WM_MBUTTONUP_CODE: u32 = 0x0208;

#[derive(Clone, Copy)]
enum HookSignal {
    Enter,
    Repaste,
}

/// Called from paste_text() in lib.rs. Captures both the pasted text and the
/// foreground window/process identity, so the Enter-handler can validate the
/// user is still editing the same target. Pastes into Trispr's own window are
/// ignored — we never want to learn from edits in our own UI.
pub fn record_paste(state: &EnterCaptureState, text: &str) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    #[cfg(target_os = "windows")]
    let target = unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return;
        }
        let mut pid: u32 = 0;
        let _tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let our_pid = std::process::id();
        if pid == our_pid || pid == 0 {
            return;
        }
        TargetIdentity {
            hwnd: hwnd.0 as isize,
            pid,
            timestamp_ms: now_ms,
        }
    };

    if let Ok(mut guard) = state.last_paste.lock() {
        *guard = Some((text.to_owned(), now_ms));
    }
    #[cfg(target_os = "windows")]
    if let Ok(mut guard) = state.last_paste_target.lock() {
        *guard = Some(target);
    }
}

thread_local! {
    static HOOK_TX: RefCell<Option<mpsc::SyncSender<HookSignal>>> = const { RefCell::new(None) };
    static CTRL_HELD: RefCell<bool> = const { RefCell::new(false) };
    static ALT_HELD: RefCell<bool> = const { RefCell::new(false) };
}

/// Start both threads. Call once from app setup.
pub fn start_hook_thread(app: tauri::AppHandle) {
    let (signal_tx, signal_rx) = mpsc::sync_channel::<HookSignal>(1);
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
fn run_hook_loop(app: tauri::AppHandle, signal_tx: mpsc::SyncSender<HookSignal>) {
    unsafe {
        let tid = windows_sys::Win32::System::Threading::GetCurrentThreadId();
        app.state::<crate::state::AppState>()
            .enter_capture
            .hook_thread_id
            .store(tid, Ordering::Release);

        HOOK_TX.with(|cell| {
            *cell.borrow_mut() = Some(signal_tx);
        });

        let keyboard_hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), None, 0)
        {
            Ok(h) => h,
            Err(e) => {
                error!("[enter-capture] SetWindowsHookExW failed: {e} - Enter-Capture is disabled");
                return;
            }
        };
        let mouse_hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(ll_mouse_proc), None, 0) {
            Ok(h) => Some(h),
            Err(e) => {
                error!("[repaste] SetWindowsHookExW failed: {e} - re-paste is disabled");
                None
            }
        };

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if let Some(mouse_hook) = mouse_hook {
            let _ = UnhookWindowsHookEx(mouse_hook);
        }
        let _ = UnhookWindowsHookEx(keyboard_hook);
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn ll_keyboard_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode >= 0 {
        let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let message = wparam.0 as u32;
        let is_down = matches!(message, WM_KEYDOWN_CODE | WM_SYSKEYDOWN_CODE);
        let is_up = matches!(message, WM_KEYUP_CODE | WM_SYSKEYUP_CODE);
        if is_down || is_up {
            match kbd.vkCode {
                VK_CONTROL_CODE | VK_LCONTROL_CODE | VK_RCONTROL_CODE => {
                    CTRL_HELD.with(|held| *held.borrow_mut() = is_down);
                }
                VK_MENU_CODE | VK_LMENU_CODE | VK_RMENU_CODE => {
                    ALT_HELD.with(|held| *held.borrow_mut() = is_down);
                }
                _ => {}
            }
        }
        if message == WM_KEYDOWN_CODE && kbd.vkCode == VK_RETURN_CODE {
            HOOK_TX.with(|cell| {
                if let Some(tx) = cell.borrow().as_ref() {
                    let _ = tx.try_send(HookSignal::Enter);
                }
            });
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn ll_mouse_proc(ncode: i32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    if ncode >= 0
        && is_repaste_gesture(
            wparam.0 as u32,
            CTRL_HELD.with(|held| *held.borrow()),
            ALT_HELD.with(|held| *held.borrow()),
        )
    {
        HOOK_TX.with(|cell| {
            if let Some(tx) = cell.borrow().as_ref() {
                let _ = tx.try_send(HookSignal::Repaste);
            }
        });
    }
    CallNextHookEx(None, ncode, wparam, _lparam)
}

fn is_repaste_gesture(message: u32, ctrl_held: bool, alt_held: bool) -> bool {
    message == WM_MBUTTONUP_CODE && ctrl_held && alt_held
}

#[cfg(target_os = "windows")]
fn run_worker_loop(
    app: tauri::AppHandle,
    signal_rx: mpsc::Receiver<HookSignal>,
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
                Ok(HookSignal::Enter) => handle_enter_signal(&app, &automation),
                Ok(HookSignal::Repaste) => crate::repaste_last_text(&app),
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

    // Identity-Validation: only emit if the user is still in the same window/process
    // as at paste-time. Without this, pressing Enter in any window after pasting
    // somewhere would feed the learner with unrelated focused text.
    let stored_target = state
        .enter_capture
        .last_paste_target
        .lock()
        .ok()
        .and_then(|g| g.clone());
    let Some(stored_target) = stored_target else {
        return;
    };
    let same_target = unsafe {
        let current_hwnd = GetForegroundWindow();
        if current_hwnd.0.is_null() {
            false
        } else {
            let mut current_pid: u32 = 0;
            let _ = GetWindowThreadProcessId(current_hwnd, Some(&mut current_pid));
            current_hwnd.0 as isize == stored_target.hwnd && current_pid == stored_target.pid
        }
    };
    if !same_target {
        if let Ok(mut guard) = state.enter_capture.last_paste_target.lock() {
            *guard = None;
        }
        return;
    }

    let (current_text, pattern_used) = match unsafe { read_focused_value(automation) } {
        Some(t) => t,
        None => return,
    };
    if current_text.is_empty() || current_text == pasted {
        return;
    }

    if let Ok(mut guard) = state.enter_capture.last_paste.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = state.enter_capture.last_paste_target.lock() {
        *guard = None;
    }

    #[derive(Clone, serde::Serialize)]
    struct EditPayload {
        pasted: String,
        submitted: String,
        same_target: bool,
        pattern_used: String,
    }

    use tauri::Emitter;
    if let Err(e) = app.emit(
        "enter_capture:edit_detected",
        EditPayload {
            pasted,
            submitted: current_text,
            same_target: true,
            pattern_used: pattern_used.to_string(),
        },
    ) {
        warn!("[enter-capture] emit failed: {e}");
    }
}

/// Read the user-visible text of the focused element. Returns the text plus a
/// label identifying which UIA pattern produced it — useful for diagnostics
/// and for downstream confidence weighting.
///
/// Strategy hierarchy:
/// 1. **ValuePattern** — plain inputs (Slack/Teams chat fields, browser inputs).
/// 2. **TextPattern + GetSelection + ExpandToEnclosingUnit(Line)** — Monaco /
///    VS-Code / Antigravity: returns just the line containing the caret instead
///    of the entire document, sidesteps the `findEditWindow` heuristic.
/// 3. **TextPattern + DocumentRange** — fallback; downstream `findEditWindow`
///    in vocab-auto-learn shrinks oversized output.
#[cfg(target_os = "windows")]
unsafe fn read_focused_value(automation: &IUIAutomation2) -> Option<(String, &'static str)> {
    let element = automation.GetFocusedElement().ok()?;

    if let Ok(raw) = element.GetCurrentPattern(UIA_ValuePatternId) {
        if let Ok(vp) = raw.cast::<IUIAutomationValuePattern>() {
            if let Ok(bstr) = vp.CurrentValue() {
                let s = bstr.to_string();
                if !s.is_empty() {
                    return Some((s, "value"));
                }
            }
        }
    }

    if let Ok(raw) = element.GetCurrentPattern(UIA_TextPatternId) {
        if let Ok(tp) = raw.cast::<IUIAutomationTextPattern>() {
            if let Ok(selection) = tp.GetSelection() {
                if let Ok(count) = selection.Length() {
                    if count > 0 {
                        if let Ok(range) = selection.GetElement(0) {
                            let _ = range.ExpandToEnclosingUnit(TextUnit_Line);
                            if let Ok(bstr) = range.GetText(-1) {
                                let s = bstr.to_string();
                                if !s.is_empty() {
                                    return Some((s, "selection-line"));
                                }
                            }
                        }
                    }
                }
            }
            if let Ok(doc) = tp.DocumentRange() {
                if let Ok(bstr) = doc.GetText(-1) {
                    let s = bstr.to_string();
                    if !s.is_empty() {
                        return Some((s, "document"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repaste_requires_ctrl_alt_middle_button_release() {
        assert!(is_repaste_gesture(WM_MBUTTONUP_CODE, true, true));
        assert!(!is_repaste_gesture(WM_MBUTTONUP_CODE, true, false));
        assert!(!is_repaste_gesture(WM_MBUTTONUP_CODE, false, true));
        assert!(!is_repaste_gesture(WM_MBUTTONUP_CODE, false, false));
        assert!(!is_repaste_gesture(WM_KEYUP_CODE, true, true));
    }
}
