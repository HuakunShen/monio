//! HarmonyOS input monitoring and keyboard-grab lifecycle.

use super::constants::{
    AXIS_ACTION_UPDATE, AXIS_EVENT_TYPE_SCROLL, AXIS_TYPE_SCROLL_HORIZONTAL,
    AXIS_TYPE_SCROLL_VERTICAL,
};
use super::lifecycle::{
    HandlerMode, RegistrationApi, RegistrationMode, register, should_dispatch_original, unregister,
};
use super::result::{hook_start_error, hook_stop_error, platform_error};
use super::translate::{translate_axis, translate_key, translate_mouse};
use crate::error::{Error, Result};
use crate::event::Event;
use crate::hook::{EventHandler, GrabHandler};
use ohos_input_sys::axis_type::{
    InputEvent_AxisAction, InputEvent_AxisEventType, InputEvent_AxisType,
};
use ohos_input_sys::input_manager::{
    Input_AxisEventCallback, Input_KeyEventCallback, Input_MouseEventCallback, Input_Result,
    OH_Input_AddAxisEventMonitorForAll, OH_Input_AddKeyEventHook, OH_Input_AddKeyEventMonitor,
    OH_Input_AddMouseEventMonitor, OH_Input_DispatchToNextHandler, OH_Input_GetAxisEventAction,
    OH_Input_GetAxisEventAxisValue, OH_Input_GetAxisEventGlobalX, OH_Input_GetAxisEventGlobalY,
    OH_Input_GetAxisEventType, OH_Input_GetKeyEventAction, OH_Input_GetKeyEventId,
    OH_Input_GetKeyEventKeyCode, OH_Input_GetMouseEventAction, OH_Input_GetMouseEventButton,
    OH_Input_GetMouseEventGlobalX, OH_Input_GetMouseEventGlobalY,
    OH_Input_RemoveAxisEventMonitorForAll, OH_Input_RemoveKeyEventHook,
    OH_Input_RemoveKeyEventMonitor, OH_Input_RemoveMouseEventMonitor,
};
use ohos_sys_opaque_types::{Input_AxisEvent, Input_KeyEvent, Input_MouseEvent};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const KEY_MONITOR_CALLBACK: Input_KeyEventCallback = Some(key_monitor_callback);
const KEY_HOOK_CALLBACK: Input_KeyEventCallback = Some(key_hook_callback);
const MOUSE_MONITOR_CALLBACK: Input_MouseEventCallback = Some(mouse_monitor_callback);
const AXIS_MONITOR_CALLBACK: Input_AxisEventCallback = Some(axis_monitor_callback);

#[derive(Clone)]
enum ActiveHandler {
    Listen(Arc<dyn EventHandler>),
    Grab(Arc<dyn GrabHandler>),
}

struct Session {
    handler: ActiveHandler,
    background_error: Option<Error>,
    enabled: bool,
}

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn session_slot() -> &'static Mutex<Option<Session>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

fn install_session(handler: ActiveHandler) -> Result<()> {
    let mut slot = session_slot()
        .lock()
        .map_err(|_| Error::ThreadError("HarmonyOS session lock is poisoned".into()))?;
    if slot.is_some() {
        return Err(Error::AlreadyRunning);
    }
    *slot = Some(Session {
        handler,
        background_error: None,
        enabled: false,
    });
    Ok(())
}

fn mark_enabled() -> Result<()> {
    let mut slot = session_slot()
        .lock()
        .map_err(|_| Error::ThreadError("HarmonyOS session lock is poisoned".into()))?;
    let session = slot
        .as_mut()
        .ok_or_else(|| Error::ThreadError("HarmonyOS session disappeared".into()))?;
    session.enabled = true;
    Ok(())
}

fn clone_active_handler() -> std::result::Result<Option<ActiveHandler>, ()> {
    session_slot()
        .lock()
        .map(|slot| slot.as_ref().map(|session| session.handler.clone()))
        .map_err(|_| ())
}

fn record_background_error(error: Error) {
    match session_slot().lock() {
        Ok(mut slot) => {
            if let Some(session) = slot.as_mut()
                && session.background_error.is_none()
            {
                session.background_error = Some(error);
            }
        }
        Err(_) => {
            log::error!("HarmonyOS session lock is poisoned; callback error was: {error}");
        }
    }
}

fn take_background_error() -> Result<Option<Error>> {
    let mut slot = session_slot()
        .lock()
        .map_err(|_| Error::ThreadError("HarmonyOS session lock is poisoned".into()))?;
    Ok(slot
        .as_mut()
        .and_then(|session| session.background_error.take()))
}

fn take_session() -> Result<Option<Session>> {
    session_slot()
        .lock()
        .map(|mut slot| slot.take())
        .map_err(|_| Error::ThreadError("HarmonyOS session lock is poisoned".into()))
}

fn clear_session_after_start_failure() {
    match session_slot().lock() {
        Ok(mut slot) => {
            slot.take();
        }
        Err(_) => log::error!("HarmonyOS session lock is poisoned during startup cleanup"),
    }
    crate::state::reset_mask();
}

fn call_observer(handler: &ActiveHandler, event: &Event) {
    let outcome = match handler {
        ActiveHandler::Listen(handler) => {
            debug_assert!(!should_dispatch_original(
                HandlerMode::Listen,
                false,
                false,
                true,
            ));
            catch_unwind(AssertUnwindSafe(|| handler.handle_event(event)))
        }
        ActiveHandler::Grab(handler) => {
            catch_unwind(AssertUnwindSafe(|| handler.handle_event(event))).map(|_| ())
        }
    };

    if outcome.is_err() {
        log::error!("HarmonyOS input handler panicked; the observed event was dropped");
    }
}

fn deliver_observed(event: Event) {
    match clone_active_handler() {
        Ok(Some(handler)) => call_observer(&handler, &event),
        Ok(None) => log::error!("HarmonyOS callback arrived without an active session"),
        Err(()) => log::error!("HarmonyOS session lock is poisoned; observed event was dropped"),
    }
}

fn input_code(result: Input_Result) -> std::result::Result<(), u32> {
    result.map_err(|error| error.0.get())
}

fn callback_field_error(operation: &str, code: u32) {
    record_background_error(platform_error(operation, code));
}

fn key_event_id(event: *const Input_KeyEvent) -> std::result::Result<i32, u32> {
    let mut event_id = 0;
    // SAFETY: the callback verified that event is non-null and Input Kit keeps
    // the event alive until the callback returns.
    input_code(unsafe { OH_Input_GetKeyEventId(event, &mut event_id) })?;
    Ok(event_id)
}

fn dispatch_original(event_id: i32) {
    // SAFETY: event_id came from OH_Input_GetKeyEventId for the active key hook
    // and is dispatched synchronously within the callback.
    if let Err(code) = input_code(unsafe { OH_Input_DispatchToNextHandler(event_id) }) {
        callback_field_error("OH_Input_DispatchToNextHandler", code);
    }
}

fn key_monitor_impl(event: *const Input_KeyEvent) {
    if event.is_null() {
        log::error!("OH_Input key monitor supplied a null event");
        return;
    }

    // SAFETY: Input Kit owns a non-null event that remains alive until this
    // callback returns. Only primitive values are copied.
    let (action, keycode) = unsafe {
        (
            OH_Input_GetKeyEventAction(event),
            OH_Input_GetKeyEventKeyCode(event),
        )
    };
    if let Some(event) = translate_key(action, keycode) {
        deliver_observed(event);
    }
}

unsafe extern "C" fn key_monitor_callback(event: *const Input_KeyEvent) {
    if catch_unwind(AssertUnwindSafe(|| key_monitor_impl(event))).is_err() {
        log::error!("HarmonyOS key monitor callback panicked; event was dropped");
    }
}

fn key_hook_impl(event: *const Input_KeyEvent) {
    if event.is_null() {
        log::error!("OH_Input key hook supplied a null event");
        return;
    }

    let event_id = match key_event_id(event) {
        Ok(event_id) => event_id,
        Err(code) => {
            callback_field_error("OH_Input_GetKeyEventId", code);
            return;
        }
    };

    // SAFETY: Input Kit owns a non-null event that remains alive until this
    // callback returns. Only primitive values are copied.
    let (action, keycode) = unsafe {
        (
            OH_Input_GetKeyEventAction(event),
            OH_Input_GetKeyEventKeyCode(event),
        )
    };
    let Some(event) = translate_key(action, keycode) else {
        dispatch_original(event_id);
        return;
    };

    let handler = match clone_active_handler() {
        Ok(Some(ActiveHandler::Grab(handler))) => handler,
        Ok(Some(ActiveHandler::Listen(_))) | Ok(None) | Err(()) => {
            dispatch_original(event_id);
            return;
        }
    };

    let outcome = catch_unwind(AssertUnwindSafe(|| handler.handle_event(&event)));
    let handler_panicked = outcome.is_err();
    if handler_panicked {
        log::error!("HarmonyOS keyboard grab handler panicked; event was passed through");
    }
    let handler_returned_some = outcome.as_ref().is_ok_and(|event| event.as_ref().is_some());

    if should_dispatch_original(
        HandlerMode::Grab,
        handler_returned_some,
        handler_panicked,
        true,
    ) {
        dispatch_original(event_id);
    }
}

unsafe extern "C" fn key_hook_callback(event: *const Input_KeyEvent) {
    if catch_unwind(AssertUnwindSafe(|| key_hook_impl(event))).is_err() {
        log::error!("HarmonyOS key hook callback panicked; attempting fail-open dispatch");
        if !event.is_null() {
            match key_event_id(event) {
                Ok(event_id) => dispatch_original(event_id),
                Err(code) => callback_field_error("OH_Input_GetKeyEventId", code),
            }
        }
    }
}

fn mouse_monitor_impl(event: *const Input_MouseEvent) {
    if event.is_null() {
        log::error!("OH_Input mouse monitor supplied a null event");
        return;
    }

    // SAFETY: Input Kit owns a non-null event that remains alive until this
    // callback returns. Only primitive values are copied.
    let (action, button, x, y) = unsafe {
        (
            OH_Input_GetMouseEventAction(event),
            OH_Input_GetMouseEventButton(event),
            OH_Input_GetMouseEventGlobalX(event),
            OH_Input_GetMouseEventGlobalY(event),
        )
    };
    if let Some(event) = translate_mouse(action, button, f64::from(x), f64::from(y)) {
        deliver_observed(event);
    }
}

unsafe extern "C" fn mouse_monitor_callback(event: *const Input_MouseEvent) {
    if catch_unwind(AssertUnwindSafe(|| mouse_monitor_impl(event))).is_err() {
        log::error!("HarmonyOS mouse monitor callback panicked; event was dropped");
    }
}

fn axis_monitor_impl(event: *const Input_AxisEvent) {
    if event.is_null() {
        log::error!("OH_Input axis monitor supplied a null event");
        return;
    }

    let mut action = InputEvent_AxisAction(0);
    let mut event_type = InputEvent_AxisEventType(0);
    let mut x = 0;
    let mut y = 0;

    // SAFETY: Input Kit owns a non-null event that remains alive until this
    // callback returns. Output pointers refer to live stack values.
    let fields = unsafe {
        [
            (
                "OH_Input_GetAxisEventAction",
                input_code(OH_Input_GetAxisEventAction(event, &mut action)),
            ),
            (
                "OH_Input_GetAxisEventType",
                input_code(OH_Input_GetAxisEventType(event, &mut event_type)),
            ),
            (
                "OH_Input_GetAxisEventGlobalX",
                input_code(OH_Input_GetAxisEventGlobalX(event, &mut x)),
            ),
            (
                "OH_Input_GetAxisEventGlobalY",
                input_code(OH_Input_GetAxisEventGlobalY(event, &mut y)),
            ),
        ]
    };

    for (operation, result) in fields {
        if let Err(code) = result {
            callback_field_error(operation, code);
            return;
        }
    }

    if action.0 != AXIS_ACTION_UPDATE || event_type.0 != AXIS_EVENT_TYPE_SCROLL {
        return;
    }

    for axis_type in [AXIS_TYPE_SCROLL_VERTICAL, AXIS_TYPE_SCROLL_HORIZONTAL] {
        let mut value = 0.0;
        // SAFETY: event is alive for the callback, axis_type is a documented
        // Input Kit value, and value is a valid output pointer.
        let result = unsafe {
            OH_Input_GetAxisEventAxisValue(event, InputEvent_AxisType(axis_type), &mut value)
        };
        if result.is_ok()
            && let Some(event) = translate_axis(axis_type, f64::from(x), f64::from(y), value)
        {
            deliver_observed(event);
        }
    }
}

unsafe extern "C" fn axis_monitor_callback(event: *const Input_AxisEvent) {
    if catch_unwind(AssertUnwindSafe(|| axis_monitor_impl(event))).is_err() {
        log::error!("HarmonyOS axis monitor callback panicked; event was dropped");
    }
}

#[derive(Default)]
struct NativeRegistrationApi {
    first_failure: Option<(&'static str, &'static str)>,
}

impl NativeRegistrationApi {
    fn checked(
        &mut self,
        operation: &'static str,
        permission: &'static str,
        result: Input_Result,
    ) -> std::result::Result<(), u32> {
        input_code(result).inspect_err(|_| {
            if self.first_failure.is_none() {
                self.first_failure = Some((operation, permission));
            }
        })
    }

    fn failure_context(&self) -> (&'static str, &'static str) {
        self.first_failure
            .unwrap_or(("HarmonyOS Input Kit registration", "INPUT_MONITORING"))
    }
}

impl RegistrationApi for NativeRegistrationApi {
    fn add_key_hook(&mut self) -> std::result::Result<(), u32> {
        // SAFETY: the static callback has the exact Input Kit ABI and remains
        // valid until the matching remove call.
        let result = unsafe { OH_Input_AddKeyEventHook(KEY_HOOK_CALLBACK) };
        self.checked("OH_Input_AddKeyEventHook", "HOOK_KEY_EVENT", result)
    }

    fn add_key_monitor(&mut self) -> std::result::Result<(), u32> {
        // SAFETY: the static callback has the exact Input Kit ABI and remains
        // valid until the matching remove call.
        let result = unsafe { OH_Input_AddKeyEventMonitor(KEY_MONITOR_CALLBACK) };
        self.checked("OH_Input_AddKeyEventMonitor", "INPUT_MONITORING", result)
    }

    fn add_mouse_monitor(&mut self) -> std::result::Result<(), u32> {
        // SAFETY: the static callback has the exact Input Kit ABI and remains
        // valid until the matching remove call.
        let result = unsafe { OH_Input_AddMouseEventMonitor(MOUSE_MONITOR_CALLBACK) };
        self.checked("OH_Input_AddMouseEventMonitor", "INPUT_MONITORING", result)
    }

    fn add_axis_monitor(&mut self) -> std::result::Result<(), u32> {
        // SAFETY: the static callback has the exact Input Kit ABI and remains
        // valid until the matching remove call.
        let result = unsafe { OH_Input_AddAxisEventMonitorForAll(AXIS_MONITOR_CALLBACK) };
        self.checked(
            "OH_Input_AddAxisEventMonitorForAll",
            "INPUT_MONITORING",
            result,
        )
    }

    fn remove_key_hook(&mut self) -> std::result::Result<(), u32> {
        // SAFETY: this is the same static callback used for registration.
        let result = unsafe { OH_Input_RemoveKeyEventHook(KEY_HOOK_CALLBACK) };
        self.checked("OH_Input_RemoveKeyEventHook", "HOOK_KEY_EVENT", result)
    }

    fn remove_key_monitor(&mut self) -> std::result::Result<(), u32> {
        // SAFETY: this is the same static callback used for registration.
        let result = unsafe { OH_Input_RemoveKeyEventMonitor(KEY_MONITOR_CALLBACK) };
        self.checked("OH_Input_RemoveKeyEventMonitor", "INPUT_MONITORING", result)
    }

    fn remove_mouse_monitor(&mut self) -> std::result::Result<(), u32> {
        // SAFETY: this is the same static callback used for registration.
        let result = unsafe { OH_Input_RemoveMouseEventMonitor(MOUSE_MONITOR_CALLBACK) };
        self.checked(
            "OH_Input_RemoveMouseEventMonitor",
            "INPUT_MONITORING",
            result,
        )
    }

    fn remove_axis_monitor(&mut self) -> std::result::Result<(), u32> {
        // SAFETY: this is the same static callback used for registration.
        let result = unsafe { OH_Input_RemoveAxisEventMonitorForAll(AXIS_MONITOR_CALLBACK) };
        self.checked(
            "OH_Input_RemoveAxisEventMonitorForAll",
            "INPUT_MONITORING",
            result,
        )
    }
}

fn keep_first_error(outcome: &mut Result<()>, error: Error) {
    if outcome.is_ok() {
        *outcome = Err(error);
    }
}

fn run_session(
    running: &Arc<AtomicBool>,
    handler: ActiveHandler,
    mode: RegistrationMode,
) -> Result<()> {
    install_session(handler)?;

    let mut api = NativeRegistrationApi::default();
    let mut registrations = match register(&mut api, mode) {
        Ok(registrations) => registrations,
        Err(code) => {
            let (operation, permission) = api.failure_context();
            clear_session_after_start_failure();
            return Err(hook_start_error(operation, code, permission));
        }
    };

    if let Err(error) = mark_enabled() {
        let _ = unregister(&mut api, &mut registrations);
        clear_session_after_start_failure();
        return Err(error);
    }

    if let Ok(Some(handler)) = clone_active_handler() {
        call_observer(&handler, &Event::hook_enabled());
    }

    let mut outcome = Ok(());
    while running.load(Ordering::SeqCst) {
        match take_background_error() {
            Ok(Some(error)) => {
                outcome = Err(error);
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                outcome = Err(error);
                break;
            }
        }
    }

    if let Ok(Some(error)) = take_background_error() {
        keep_first_error(&mut outcome, error);
    }

    api.first_failure = None;
    if let Err(code) = unregister(&mut api, &mut registrations) {
        let (operation, _) = api.failure_context();
        keep_first_error(&mut outcome, hook_stop_error(operation, code));
    }

    match take_session() {
        Ok(Some(session)) => {
            if session.enabled {
                call_observer(&session.handler, &Event::hook_disabled());
            }
            if let Some(error) = session.background_error {
                keep_first_error(&mut outcome, error);
            }
        }
        Ok(None) => keep_first_error(
            &mut outcome,
            Error::ThreadError("HarmonyOS session disappeared during cleanup".into()),
        ),
        Err(error) => keep_first_error(&mut outcome, error),
    }
    crate::state::reset_mask();

    outcome
}

pub fn run_hook<H: EventHandler + 'static>(running: &Arc<AtomicBool>, handler: H) -> Result<()> {
    run_session(
        running,
        ActiveHandler::Listen(Arc::new(handler)),
        RegistrationMode::Listen,
    )
}

pub fn run_grab_hook<H: GrabHandler + 'static>(
    running: &Arc<AtomicBool>,
    handler: H,
) -> Result<()> {
    run_session(
        running,
        ActiveHandler::Grab(Arc::new(handler)),
        RegistrationMode::Grab,
    )
}

pub fn stop_hook() -> Result<()> {
    Ok(())
}
