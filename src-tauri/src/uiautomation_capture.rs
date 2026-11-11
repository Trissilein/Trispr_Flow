//! Windows UIAutomation Enter-Capture and re-paste gesture.
//! Two-thread architecture:
//!   - Hook-thread: WH_KEYBOARD_LL / WH_MOUSE_LL + message loop
//!   - Worker-thread: UIAutomation COM calls, event emission, re-paste

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use tauri::Manager;
use tracing::{error, info, warn};

#[cfg(target_os = "windows")]
use windows::{
    core::*, Win32::Foundation::*, Win32::System::Com::*, Win32::UI::Accessibility::*,
    Win32::UI::WindowsAndMessaging::*,
};

pub use crate::state::EnterCaptureState;
#[cfg(target_os = "windows")]
use crate::state::TargetIdentity;

const VK_RETURN_CODE: u32 = 0x0D;
const VK_SHIFT_CODE: u32 = 0x10;
const VK_LSHIFT_CODE: u32 = 0xA0;
const VK_RSHIFT_CODE: u32 = 0xA1;
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
const WM_LBUTTONUP_CODE: u32 = 0x0202;
const WM_RBUTTONUP_CODE: u32 = 0x0205;
const WM_MBUTTONUP_CODE: u32 = 0x0208;

/// How long after a paste an Enter still counts as "message sent". Editing a long dictation and
/// then sending easily takes minutes; the old 60 s window silently dropped those corrections.
const CAPTURE_WINDOW_MS: u64 = 10 * 60 * 1000;
/// Typing pause after which the focused field is read into `last_snapshot`.
const SNAPSHOT_IDLE_MS: u64 = 400;
/// Pastes into the same target are merged into one text (dictation arrives in several segments).
const MAX_ACCUMULATED_PASTE_CHARS: usize = 8_000;

/// Dictations of at most this many words skip the word-overlap gate of `select_baseline`.
const SHORT_SEGMENT_WORDS: usize = 2;
/// Fields up to this many chars are read whole through TextPattern (see `read_element_text`).
const SMALL_FIELD_CHARS: usize = 4_000;

/// Time of the latest content key-down or mouse click anywhere (Enter and bare modifiers excluded). The hook
/// only stores it (cheap); the worker decides.
static KEY_ACTIVITY_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Add a paste to the segments pasted before it into the same target. Every paste lands in the
/// field again (a re-paste or a second "yes" included), so nothing is de-duplicated. The oldest
/// segments go first once the total grows past the cap.
fn push_segment(segments: &mut Vec<String>, text: &str) {
    segments.push(text.to_owned());
    let mut total: usize = segments.iter().map(|s| s.chars().count() + 1).sum();
    while segments.len() > 1 && total > MAX_ACCUMULATED_PASTE_CHARS {
        total -= segments.remove(0).chars().count() + 1;
    }
}

fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Take the words of `segment` out of the field's word counts; returns how many were found.
fn take_words(
    segment: &str,
    field_counts: &mut std::collections::HashMap<String, usize>,
) -> (usize, usize) {
    let words = words_of(segment);
    let mut found = 0;
    for word in &words {
        if let Some(count) = field_counts.get_mut(word) {
            if *count > 0 {
                *count -= 1;
                found += 1;
            }
        }
    }
    (found, words.len())
}

/// The baseline for the diff: the newest segments that are still in the field, oldest first, or
/// `None` when even the newest segment is gone (message sent with the mouse, the field now holds
/// something unrelated: nothing to learn from).
/// The newest segment claims its words first; an older segment stays only if
/// at least half of its words are still unclaimed in the field (edits keep most words, a segment
/// already sent with the Send button, no Enter, leaves the field). Counting occurrences keeps a
/// stale segment from borrowing the words of the live one.
fn select_baseline(segments: &[String], field: &str) -> Option<String> {
    let mut field_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for word in words_of(field) {
        *field_counts.entry(word).or_insert(0) += 1;
    }
    let mut start = segments.len().checked_sub(1)?;
    let (found, total) = take_words(&segments[start], &mut field_counts);
    // A short dictation corrected as a whole (WhatsApp -> WhatsUp) shares no word with the field;
    // the learner's similarity gate judges those. The overlap gate is for longer segments.
    if total > SHORT_SEGMENT_WORDS && found * 2 < total && !field.contains(segments[start].as_str())
    {
        return None;
    }
    while start > 0 {
        let older = &segments[start - 1];
        let (found, total) = take_words(older, &mut field_counts);
        // A short segment glued to its neighbour (`hello` + `world` = `helloworld`) shares no
        // word with the field but is still literally there.
        let glued = total <= SHORT_SEGMENT_WORDS && field.contains(older.as_str());
        if total > 0 && found * 2 < total && !glued {
            break;
        }
        start -= 1;
    }
    Some(join_segments(&segments[start..], field))
}

/// Join segments the way they sit in the field: pasted back to back they are glued together
/// (`hello` + `world` = `helloworld`), so a space is only added where the field shows one.
fn join_segments(segments: &[String], field: &str) -> String {
    let mut out = String::new();
    for (idx, segment) in segments.iter().enumerate() {
        if idx > 0 && !field.contains(&format!("{}{}", segments[idx - 1], segment)) {
            out.push(' ');
        }
        out.push_str(segment);
    }
    out
}

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
    let now_ms = now_ms();

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
        }
    };

    // Dictation often lands in the same message as several segments: keep them together so an
    // edit to an earlier segment is still seen when the message is sent.
    #[cfg(target_os = "windows")]
    let same_target = state
        .last_paste_target
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .map(|t| t.hwnd == target.hwnd && t.pid == target.pid)
        .unwrap_or(false);
    #[cfg(not(target_os = "windows"))]
    let same_target = false;

    if let Ok(mut guard) = state.last_paste.lock() {
        let mut segments = match guard.take() {
            Some((prev, prev_ms))
                if same_target && now_ms.saturating_sub(prev_ms) <= CAPTURE_WINDOW_MS =>
            {
                prev
            }
            _ => Vec::new(),
        };
        push_segment(&mut segments, text);
        *guard = Some((segments, now_ms));
    }
    #[cfg(target_os = "windows")]
    if let Ok(mut guard) = state.last_paste_target.lock() {
        *guard = Some(target);
    }
    if let Ok(mut guard) = state.last_snapshot.lock() {
        *guard = None;
    }
}

/// Forget the pending paste after its Enter was handled, whatever the outcome: that Enter sent
/// the message, so the next dictation starts a fresh capture.
fn consume_capture(state: &EnterCaptureState) {
    if let Ok(mut guard) = state.last_paste.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = state.last_paste_target.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = state.last_snapshot.lock() {
        *guard = None;
    }
}

thread_local! {
    static HOOK_TX: RefCell<Option<mpsc::SyncSender<HookSignal>>> = const { RefCell::new(None) };
    static CTRL_HELD: RefCell<bool> = const { RefCell::new(false) };
    static ALT_HELD: RefCell<bool> = const { RefCell::new(false) };
    static SHIFT_HELD: RefCell<bool> = const { RefCell::new(false) };
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
                VK_SHIFT_CODE | VK_LSHIFT_CODE | VK_RSHIFT_CODE => {
                    SHIFT_HELD.with(|held| *held.borrow_mut() = is_down);
                }
                _ => {}
            }
        }
        // is_down covers WM_SYSKEYDOWN too: AltGr/Alt-held characters arrive that way.
        if is_down
            && !matches!(
                kbd.vkCode,
                VK_RETURN_CODE
                    | VK_SHIFT_CODE
                    | VK_LSHIFT_CODE
                    | VK_RSHIFT_CODE
                    | VK_CONTROL_CODE
                    | VK_LCONTROL_CODE
                    | VK_RCONTROL_CODE
                    | VK_MENU_CODE
                    | VK_LMENU_CODE
                    | VK_RMENU_CODE
            )
        {
            KEY_ACTIVITY_MS.store(now_ms(), Ordering::Relaxed);
        }
        // Shift+Enter is a line break in chat inputs, not "send": it must not consume the capture.
        if message == WM_KEYDOWN_CODE
            && kbd.vkCode == VK_RETURN_CODE
            && !SHIFT_HELD.with(|held| *held.borrow())
        {
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
    // A click can change the field without a key (spell-check pick, context-menu cut/paste):
    // it makes any earlier snapshot stale, like a keystroke.
    if ncode >= 0 && matches!(wparam.0 as u32, WM_LBUTTONUP_CODE | WM_RBUTTONUP_CODE) {
        KEY_ACTIVITY_MS.store(now_ms(), Ordering::Relaxed);
    }
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

        let mut snapshot_activity: u64 = 0;
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
                Ok(HookSignal::Enter) => {
                    // The idle gate may have passed while we waited: take the due snapshot first.
                    maybe_snapshot(&app, &automation, &mut snapshot_activity);
                    handle_enter_signal(&app, &automation)
                }
                Ok(HookSignal::Repaste) => crate::repaste_last_text(&app),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            maybe_snapshot(&app, &automation, &mut snapshot_activity);
        }

        drop(automation);
        CoUninitialize();
    }
}

/// True when the foreground window is still the one the text was pasted into.
#[cfg(target_os = "windows")]
fn foreground_matches(target: &TargetIdentity) -> bool {
    unsafe {
        let current_hwnd = GetForegroundWindow();
        if current_hwnd.0.is_null() {
            return false;
        }
        let mut current_pid: u32 = 0;
        let _ = GetWindowThreadProcessId(current_hwnd, Some(&mut current_pid));
        current_hwnd.0 as isize == target.hwnd && current_pid == target.pid
    }
}

/// While a paste is live, remember what the field looked like after each typing pause. Some apps
/// clear their input in the same instant Enter is pressed; without this the edit is gone before
/// the Enter handler can read it.
#[cfg(target_os = "windows")]
fn maybe_snapshot(app: &tauri::AppHandle, automation: &IUIAutomation2, handled_activity: &mut u64) {
    let activity = KEY_ACTIVITY_MS.load(Ordering::Relaxed);
    if activity == 0 || activity == *handled_activity {
        return;
    }
    let now = now_ms();
    if now.saturating_sub(activity) < SNAPSHOT_IDLE_MS {
        return;
    }
    *handled_activity = activity;

    let state = app.state::<crate::state::AppState>();
    let live = match state.enter_capture.last_paste.lock() {
        Ok(guard) => {
            matches!(&*guard, Some((_, ms)) if now.saturating_sub(*ms) <= CAPTURE_WINDOW_MS)
        }
        Err(_) => false,
    };
    if !live {
        return;
    }
    let target = state
        .enter_capture
        .last_paste_target
        .lock()
        .ok()
        .and_then(|g| g.clone());
    let Some(target) = target else { return };
    if !foreground_matches(&target) {
        return;
    }
    if let Some((text, _)) = unsafe { read_focused_value(automation) } {
        if !text.is_empty() {
            if let Ok(mut guard) = state.enter_capture.last_snapshot.lock() {
                // Tagged with the keystroke it follows: a later keystroke makes it stale.
                *guard = Some((cap_submitted(text), activity));
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn handle_enter_signal(app: &tauri::AppHandle, automation: &IUIAutomation2) {
    let state = app.state::<crate::state::AppState>();

    let (segments, last_ms) = match state.enter_capture.last_paste.lock() {
        Ok(guard) => match guard.clone() {
            Some(tuple) => tuple,
            None => return, // no dictation pending: every Enter on the system lands here, stay quiet
        },
        Err(_) => return,
    };

    if last_ms == 0 || now_ms().saturating_sub(last_ms) > CAPTURE_WINDOW_MS {
        info!(
            "[enter-capture] outcome=expired segments={}",
            segments.len()
        );
        consume_capture(&state.enter_capture);
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
    if !foreground_matches(&stored_target) {
        info!("[enter-capture] outcome=target_changed");
        consume_capture(&state.enter_capture);
        return;
    }

    // A snapshot is only valid if no content key was pressed after it was taken; otherwise a
    // final edit (or a reverted one) would be missed and the old text learned.
    let latest_activity = KEY_ACTIVITY_MS.load(Ordering::Relaxed);
    let snapshot = state
        .enter_capture
        .last_snapshot
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .filter(|(_, activity)| *activity >= latest_activity)
        .map(|(text, _)| text);
    // Live read wins. The snapshot only stands in when the app has already emptied (or hidden)
    // its input by the time we read it.
    let (current_text, pattern_used) = match unsafe { read_focused_value(automation) } {
        Some((text, pattern)) if !text.is_empty() => (text, pattern),
        _ => match snapshot {
            Some(text) if !text.is_empty() => (text, "snapshot"),
            _ => {
                info!(
                    "[enter-capture] outcome=unreadable segments={}",
                    segments.len()
                );
                consume_capture(&state.enter_capture);
                return;
            }
        },
    };
    consume_capture(&state.enter_capture);
    let Some(pasted) = select_baseline(&segments, &current_text) else {
        info!(
            "[enter-capture] outcome=unrelated segments={} pattern={pattern_used}",
            segments.len()
        );
        return;
    };
    if current_text == pasted {
        info!("[enter-capture] outcome=no_edit pattern={pattern_used}");
        return;
    }

    #[derive(Clone, serde::Serialize)]
    struct EditPayload {
        pasted: String,
        submitted: String,
        same_target: bool,
        pattern_used: String,
    }

    info!(
        "[enter-capture] outcome=emitted pattern={pattern_used} segments={} pasted_chars={} submitted_chars={}",
        segments.len(),
        pasted.chars().count(),
        current_text.chars().count()
    );
    use tauri::Emitter;
    if let Err(e) = app.emit(
        "enter_capture:edit_detected",
        EditPayload {
            pasted,
            submitted: cap_submitted(current_text),
            same_target: true,
            pattern_used: pattern_used.to_string(),
        },
    ) {
        warn!("[enter-capture] emit failed: {e}");
    }
}

/// Upper bound (chars) for the text read from the focused element. A whole-document dump from an
/// editor can be huge and unrelated to the pasted dictation; the learner only needs the edited
/// region, so anything beyond this is not sent to the frontend.
const MAX_SUBMITTED_CHARS: usize = 20_000;

fn cap_submitted(text: String) -> String {
    match text.char_indices().nth(MAX_SUBMITTED_CHARS) {
        Some((byte_idx, _)) => text[..byte_idx].to_string(),
        None => text,
    }
}

/// Read the user-visible text of the focused element. Returns the text plus a
/// label identifying which UIA pattern produced it — useful for diagnostics
/// and for downstream confidence weighting. Password fields are never read.
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
    let focused = automation.GetFocusedElement().ok()?;
    // Password fields are never read, and never replaced by a container above them either.
    if is_password_element(&focused) {
        return None;
    }
    if let Some(found) = read_element_text(&focused) {
        return Some(found);
    }
    // Chromium/Electron often focuses a child node without text patterns: the editable
    // container sits a few levels up.
    let walker = automation.ControlViewWalker().ok()?;
    let mut current = focused;
    for _ in 0..3 {
        current = walker.GetParentElement(&current).ok()?;
        if is_password_element(&current) {
            return None;
        }
        if let Some(found) = read_element_text(&current) {
            return Some(found);
        }
    }
    None
}

#[cfg(target_os = "windows")]
unsafe fn is_password_element(element: &IUIAutomationElement) -> bool {
    element
        .CurrentIsPassword()
        .map(|is_password| is_password.as_bool())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
unsafe fn read_element_text(element: &IUIAutomationElement) -> Option<(String, &'static str)> {
    if is_password_element(element) {
        return None;
    }

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
            // A chat-sized field is read whole, so a correction on an earlier line survives the
            // caret sitting on the last one. Larger documents (code editors) keep the caret line.
            if let Ok(doc) = tp.DocumentRange() {
                if let Ok(bstr) = doc.GetText(SMALL_FIELD_CHARS as i32 + 1) {
                    let s = bstr.to_string();
                    if !s.is_empty() && s.chars().count() <= SMALL_FIELD_CHARS {
                        return Some((s, "document"));
                    }
                }
            }
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

    #[test]
    fn segments_are_kept_even_when_repeated_and_capped_oldest_first() {
        let mut segs = Vec::new();
        push_segment(&mut segs, "Ja");
        push_segment(&mut segs, "Ja");
        assert_eq!(segs, vec!["Ja", "Ja"]);
        push_segment(&mut segs, &"x".repeat(MAX_ACCUMULATED_PASTE_CHARS));
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn baseline_drops_segments_that_left_the_field() {
        let segs = vec![
            "Schau dir die WhatsApp Tickets an.".to_string(),
            "Danach machen wir den Deploy fertig.".to_string(),
        ];
        let baseline = |segs: &[String], field: &str| select_baseline(segs, field);
        // Both segments still in the field (one message, edited).
        assert_eq!(
            baseline(
                &segs,
                "Schau dir die WhatsUp Tickets an. Danach machen wir den Deploy fertig."
            )
            .as_deref(),
            Some("Schau dir die WhatsApp Tickets an. Danach machen wir den Deploy fertig.")
        );
        // First segment was sent with the Send button: only the second is the baseline.
        assert_eq!(
            baseline(&segs, "Danach machen wir den Deploy fertig, bitte.").as_deref(),
            Some("Danach machen wir den Deploy fertig.")
        );
        // A stale segment must not borrow the words of the live one (WhatsApp -> WhatsUp case).
        let stale = vec![
            "open WhatsUp please".to_string(),
            "open WhatsApp please".to_string(),
        ];
        assert_eq!(
            baseline(&stale, "open WhatsUp please").as_deref(),
            Some("open WhatsApp please")
        );
    }

    #[test]
    fn short_dictations_and_glued_segments_are_handled() {
        // One word corrected as a whole: no shared word, still a baseline.
        let one = vec!["WhatsApp".to_string()];
        assert_eq!(
            select_baseline(&one, "WhatsUp").as_deref(),
            Some("WhatsApp")
        );
        // Back-to-back pastes stay glued where the field shows no separator.
        let two = vec!["hello".to_string(), "world".to_string()];
        assert_eq!(
            select_baseline(&two, "helloworld").as_deref(),
            Some("helloworld")
        );
        assert_eq!(
            select_baseline(&two, "hello world").as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn baseline_is_none_when_the_newest_segment_is_gone() {
        let one = vec!["Danach machen wir den Deploy fertig.".to_string()];
        // Sent with the mouse, the field now holds an unrelated message.
        assert_eq!(select_baseline(&one, "ganz anderer Text"), None);
        assert_eq!(select_baseline(&[], "egal"), None);
    }

    #[test]
    fn submitted_text_is_capped_on_a_char_boundary() {
        let short = "kurz".to_string();
        assert_eq!(cap_submitted(short.clone()), short);

        // multi-byte chars: the cut must never split a UTF-8 sequence
        let long = "ä".repeat(MAX_SUBMITTED_CHARS + 500);
        let capped = cap_submitted(long);
        assert_eq!(capped.chars().count(), MAX_SUBMITTED_CHARS);
        assert!(capped.chars().all(|c| c == 'ä'));

        let exact = "x".repeat(MAX_SUBMITTED_CHARS);
        assert_eq!(cap_submitted(exact.clone()), exact);
    }
}
