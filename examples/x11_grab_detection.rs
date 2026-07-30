//! Native X11 diagnostic for selective event grabbing.
//!
//! Run with:
//!
//! ```bash
//! cargo run --features x11 --example x11_grab_detection
//! ```

#[cfg(target_os = "linux")]
mod linux {
    use monio::{
        Button, Event, EventType, Hook, Key, key_press, key_release, mouse_move, mouse_press,
        mouse_release,
    };
    use std::error::Error;
    use std::io;
    use std::ptr::null;
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use x11::{keysym, xlib};

    const HOOK_START_TIMEOUT: Duration = Duration::from_secs(5);
    const OBSERVATION_TIMEOUT: Duration = Duration::from_millis(750);
    const POLL_INTERVAL: Duration = Duration::from_millis(5);
    const OBSERVER_X: i32 = 256;
    const OBSERVER_Y: i32 = 256;
    const OBSERVER_POINTER_X: i32 = OBSERVER_X + 32;
    const OBSERVER_POINTER_Y: i32 = OBSERVER_Y + 32;

    struct KeyboardObserver {
        display: *mut xlib::Display,
        window: xlib::Window,
        root: xlib::Window,
        q_keycode: u32,
        w_keycode: u32,
        previous_focus: xlib::Window,
        previous_revert_to: i32,
        previous_pointer_x: i32,
        previous_pointer_y: i32,
    }

    impl KeyboardObserver {
        fn new() -> Result<Self, Box<dyn Error>> {
            unsafe {
                let display = xlib::XOpenDisplay(null());
                if display.is_null() {
                    return Err(io::Error::other("failed to open X display").into());
                }

                let screen = xlib::XDefaultScreen(display);
                let root = xlib::XRootWindow(display, screen);
                let q_keycode = xlib::XKeysymToKeycode(display, keysym::XK_Q as xlib::KeySym);
                let w_keycode = xlib::XKeysymToKeycode(display, keysym::XK_W as xlib::KeySym);
                if q_keycode == 0 || w_keycode == 0 {
                    xlib::XCloseDisplay(display);
                    return Err(io::Error::other("Q or W is unavailable in the X11 keymap").into());
                }

                let window = xlib::XCreateSimpleWindow(
                    display, root, OBSERVER_X, OBSERVER_Y, 64, 64, 0, 0, 0,
                );
                if window == 0 {
                    xlib::XCloseDisplay(display);
                    return Err(io::Error::other("failed to create X11 observer window").into());
                }

                let mut previous_focus = 0;
                let mut previous_revert_to = 0;
                xlib::XGetInputFocus(display, &mut previous_focus, &mut previous_revert_to);
                let mut attributes: xlib::XSetWindowAttributes = std::mem::zeroed();
                attributes.override_redirect = xlib::True;
                xlib::XChangeWindowAttributes(
                    display,
                    window,
                    xlib::CWOverrideRedirect,
                    &mut attributes,
                );
                xlib::XSelectInput(
                    display,
                    window,
                    xlib::KeyPressMask
                        | xlib::KeyReleaseMask
                        | xlib::ButtonPressMask
                        | xlib::ButtonReleaseMask
                        | xlib::PointerMotionMask,
                );
                xlib::XMapWindow(display, window);
                xlib::XSync(display, xlib::False);

                let mut root_return = 0;
                let mut child_return = 0;
                let mut previous_pointer_x = 0;
                let mut previous_pointer_y = 0;
                let mut window_x = 0;
                let mut window_y = 0;
                let mut pointer_mask = 0;
                xlib::XQueryPointer(
                    display,
                    root,
                    &mut root_return,
                    &mut child_return,
                    &mut previous_pointer_x,
                    &mut previous_pointer_y,
                    &mut window_x,
                    &mut window_y,
                    &mut pointer_mask,
                );
                xlib::XWarpPointer(
                    display,
                    0,
                    root,
                    0,
                    0,
                    0,
                    0,
                    OBSERVER_POINTER_X,
                    OBSERVER_POINTER_Y,
                );
                xlib::XSync(display, xlib::False);

                let observer = Self {
                    display,
                    window,
                    root,
                    q_keycode: q_keycode as u32,
                    w_keycode: w_keycode as u32,
                    previous_focus,
                    previous_revert_to,
                    previous_pointer_x,
                    previous_pointer_y,
                };
                observer.focus();
                Ok(observer)
            }
        }

        fn q_keycode(&self) -> u32 {
            self.q_keycode
        }

        fn w_keycode(&self) -> u32 {
            self.w_keycode
        }

        fn begin_active_grab(&self) -> Result<(), Box<dyn Error>> {
            let status = unsafe {
                xlib::XGrabKeyboard(
                    self.display,
                    self.window,
                    xlib::False,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                    xlib::CurrentTime,
                )
            };
            if status != xlib::GrabSuccess {
                return Err(io::Error::other(format!(
                    "X11 observer keyboard grab failed with status {status}"
                ))
                .into());
            }
            Ok(())
        }

        fn focus(&self) {
            unsafe {
                xlib::XSetInputFocus(
                    self.display,
                    self.window,
                    xlib::RevertToPointerRoot,
                    xlib::CurrentTime,
                );
                xlib::XSync(self.display, xlib::False);
            }
        }

        fn routing_state(&self) -> (xlib::Window, xlib::Window, i32, i32, i32) {
            unsafe {
                let mut focus = 0;
                let mut revert_to = 0;
                xlib::XGetInputFocus(self.display, &mut focus, &mut revert_to);

                let mut root_return = 0;
                let mut child_return = 0;
                let mut root_x = 0;
                let mut root_y = 0;
                let mut window_x = 0;
                let mut window_y = 0;
                let mut pointer_mask = 0;
                xlib::XQueryPointer(
                    self.display,
                    self.root,
                    &mut root_return,
                    &mut child_return,
                    &mut root_x,
                    &mut root_y,
                    &mut window_x,
                    &mut window_y,
                    &mut pointer_mask,
                );

                (focus, child_return, root_x, root_y, pointer_mask as i32)
            }
        }

        fn end_active_grab(&self) {
            unsafe {
                xlib::XUngrabKeyboard(self.display, xlib::CurrentTime);
                xlib::XSync(self.display, xlib::False);
            }
        }

        fn begin_active_pointer_grab(&self) -> Result<(), Box<dyn Error>> {
            let status = unsafe {
                xlib::XGrabPointer(
                    self.display,
                    self.window,
                    xlib::False,
                    (xlib::ButtonPressMask | xlib::ButtonReleaseMask | xlib::PointerMotionMask)
                        as u32,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                    0,
                    0,
                    xlib::CurrentTime,
                )
            };
            if status != xlib::GrabSuccess {
                return Err(io::Error::other(format!(
                    "X11 observer pointer grab failed with status {status}"
                ))
                .into());
            }
            Ok(())
        }

        fn end_active_pointer_grab(&self) {
            unsafe {
                xlib::XUngrabPointer(self.display, xlib::CurrentTime);
                xlib::XSync(self.display, xlib::False);
            }
        }

        fn collect_events(&self) -> Vec<(i32, u32)> {
            let deadline = Instant::now() + OBSERVATION_TIMEOUT;
            let mut events = Vec::new();

            while Instant::now() < deadline {
                unsafe {
                    while xlib::XPending(self.display) > 0 {
                        let mut event: xlib::XEvent = std::mem::zeroed();
                        xlib::XNextEvent(self.display, &mut event);
                        let type_ = event.get_type();
                        if type_ == xlib::KeyPress || type_ == xlib::KeyRelease {
                            events.push((type_, event.key.keycode));
                        } else if type_ == xlib::ButtonPress || type_ == xlib::ButtonRelease {
                            events.push((type_, event.button.button));
                        } else if type_ == xlib::MotionNotify {
                            events.push((type_, 0));
                        }
                    }
                }
                sleep(POLL_INTERVAL);
            }

            events
        }
    }

    impl Drop for KeyboardObserver {
        fn drop(&mut self) {
            unsafe {
                xlib::XSetInputFocus(
                    self.display,
                    self.previous_focus,
                    self.previous_revert_to,
                    xlib::CurrentTime,
                );
                xlib::XWarpPointer(
                    self.display,
                    0,
                    self.root,
                    0,
                    0,
                    0,
                    0,
                    self.previous_pointer_x,
                    self.previous_pointer_y,
                );
                xlib::XDestroyWindow(self.display, self.window);
                xlib::XSync(self.display, xlib::False);
                xlib::XCloseDisplay(self.display);
            }
        }
    }

    fn emit_key_pair(key: Key) -> monio::Result<()> {
        key_press(key)?;
        if let Err(error) = key_release(key) {
            let _ = key_release(key);
            return Err(error);
        }
        Ok(())
    }

    fn emit_button_pair(button: Button) -> monio::Result<()> {
        mouse_press(button)?;
        if let Err(error) = mouse_release(button) {
            let _ = mouse_release(button);
            return Err(error);
        }
        Ok(())
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        println!("monio X11 grab diagnostic");
        println!("=========================\n");
        println!("The grab handler will consume Q and pass W.");

        let observer = KeyboardObserver::new()?;
        let q_keycode = observer.q_keycode();
        let w_keycode = observer.w_keycode();

        observer.begin_active_grab()?;
        emit_key_pair(Key::KeyW)?;
        let preflight = observer.collect_events();
        let preflight_w = preflight
            .iter()
            .filter(|(_, keycode)| *keycode == w_keycode)
            .count();
        if preflight_w != 2 {
            return Err(io::Error::other(format!(
                "X11 observer preflight received {preflight_w}/2 W events"
            ))
            .into());
        }

        let keyboard_conflict = Hook::new()
            .grab(|_: &Event| None)
            .expect_err("a second client must not acquire an active keyboard grab");
        if !keyboard_conflict
            .to_string()
            .contains("already owns the grab")
        {
            return Err(io::Error::other(format!(
                "unexpected keyboard-conflict error: {keyboard_conflict}"
            ))
            .into());
        }
        observer.end_active_grab();

        observer.begin_active_pointer_grab()?;
        let pointer_conflict = Hook::new()
            .grab(|_: &Event| None)
            .expect_err("a second client must not acquire an active pointer grab");
        if !pointer_conflict.to_string().contains("X11 pointer") {
            return Err(io::Error::other(format!(
                "unexpected pointer-conflict error: {pointer_conflict}"
            ))
            .into());
        }
        observer.begin_active_grab().map_err(|error| {
            io::Error::other(format!(
                "keyboard grab was not rolled back after pointer failure: {error}"
            ))
        })?;
        observer.end_active_grab();
        observer.end_active_pointer_grab();
        println!("Conflict handling and partial-grab rollback verified.");

        let handled = Arc::new(Mutex::new(Vec::new()));
        let handled_for_hook = handled.clone();
        let handled_buttons = Arc::new(Mutex::new(Vec::new()));
        let handled_buttons_for_hook = handled_buttons.clone();
        let handled_motion = Arc::new(Mutex::new(0usize));
        let handled_motion_for_hook = handled_motion.clone();
        let (enabled_tx, enabled_rx) = mpsc::sync_channel(1);
        let hook = Hook::new();

        hook.grab_async(move |event: &Event| {
            if event.event_type == EventType::HookEnabled {
                let _ = enabled_tx.try_send(());
            }

            if let Some(keyboard) = &event.keyboard
                && matches!(
                    event.event_type,
                    EventType::KeyPressed | EventType::KeyReleased
                )
            {
                handled_for_hook
                    .lock()
                    .expect("handled-event mutex poisoned")
                    .push((event.event_type, keyboard.key));

                if keyboard.key == Key::KeyQ {
                    return None;
                }
            }

            if let Some(mouse) = &event.mouse
                && matches!(
                    event.event_type,
                    EventType::MousePressed | EventType::MouseReleased
                )
                && let Some(button) = mouse.button
            {
                handled_buttons_for_hook
                    .lock()
                    .expect("handled-button mutex poisoned")
                    .push((event.event_type, button));

                if button == Button::Left {
                    return None;
                }
            }

            if matches!(
                event.event_type,
                EventType::MouseMoved | EventType::MouseDragged
            ) {
                *handled_motion_for_hook
                    .lock()
                    .expect("handled-motion mutex poisoned") += 1;
                return None;
            }

            Some(event.clone())
        })?;

        enabled_rx
            .recv_timeout(HOOK_START_TIMEOUT)
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("grab hook did not report HookEnabled: {error}"),
                )
            })?;

        observer.focus();
        let (focus, pointer_child, pointer_x, pointer_y, pointer_mask) = observer.routing_state();
        println!(
            "Observer routing: window={:#x}, focus={focus:#x}, pointer_child={pointer_child:#x}, \
             pointer=({pointer_x}, {pointer_y}), mask={pointer_mask:#x}",
            observer.window
        );
        if focus != observer.window {
            return Err(io::Error::other("observer window did not receive keyboard focus").into());
        }
        if pointer_child != observer.window {
            return Err(io::Error::other(
                "pointer is not routed to the observer window; move it outside desktop shell UI",
            )
            .into());
        }
        emit_key_pair(Key::KeyQ)?;
        emit_key_pair(Key::KeyW)?;
        emit_button_pair(Button::Left)?;
        emit_button_pair(Button::Right)?;
        sleep(Duration::from_millis(20));
        mouse_move(40.0, 40.0)?;

        let observed = observer.collect_events();
        hook.stop()?;
        observer.begin_active_grab().map_err(|error| {
            io::Error::other(format!("keyboard remained grabbed after stop: {error}"))
        })?;
        observer.begin_active_pointer_grab().map_err(|error| {
            io::Error::other(format!("pointer remained grabbed after stop: {error}"))
        })?;
        observer.end_active_pointer_grab();
        observer.end_active_grab();

        let handled = handled
            .lock()
            .map_err(|_| io::Error::other("handled-event mutex poisoned"))?;
        let handled_q = handled.iter().filter(|(_, key)| *key == Key::KeyQ).count();
        let handled_w = handled.iter().filter(|(_, key)| *key == Key::KeyW).count();
        let observed_q = observed
            .iter()
            .filter(|(_, keycode)| *keycode == q_keycode)
            .count();
        let observed_w = observed
            .iter()
            .filter(|(_, keycode)| *keycode == w_keycode)
            .count();
        let handled_buttons = handled_buttons
            .lock()
            .map_err(|_| io::Error::other("handled-button mutex poisoned"))?;
        let handled_left = handled_buttons
            .iter()
            .filter(|(_, button)| *button == Button::Left)
            .count();
        let handled_right = handled_buttons
            .iter()
            .filter(|(_, button)| *button == Button::Right)
            .count();
        let observed_left = observed
            .iter()
            .filter(|(type_, button)| {
                matches!(*type_, xlib::ButtonPress | xlib::ButtonRelease) && *button == 1
            })
            .count();
        let observed_right = observed
            .iter()
            .filter(|(type_, button)| {
                matches!(*type_, xlib::ButtonPress | xlib::ButtonRelease) && *button == 3
            })
            .count();
        let handled_motion = *handled_motion
            .lock()
            .map_err(|_| io::Error::other("handled-motion mutex poisoned"))?;
        let observed_motion = observed
            .iter()
            .filter(|(type_, _)| *type_ == xlib::MotionNotify)
            .count();

        println!("Handler received Q press/release: {handled_q}/2");
        println!("Handler received W press/release: {handled_w}/2");
        println!("Observer received blocked Q events: {observed_q}/0");
        println!("Observer received passed W events: {observed_w}/2");
        println!("Handler received left press/release: {handled_left}/2");
        println!("Observer received blocked left events: {observed_left}/0");
        println!("Handler began passed right gesture: {handled_right}/>=1");
        println!("Observer received passed right gesture: {observed_right}/2");
        println!("Handler received pointer motion: {handled_motion}/>=1");
        println!("Observer received blocked pointer motion: {observed_motion}/0");
        println!("Stop cleanup released keyboard and pointer grabs: YES");

        if handled_q != 2
            || handled_w != 2
            || handled_left != 2
            || handled_right == 0
            || handled_motion == 0
        {
            return Err(io::Error::other("grab handler missed injected input events").into());
        }
        if observed_q != 0 {
            return Err(io::Error::other("consumed Q events reached another X11 client").into());
        }
        if observed_w != 2 {
            return Err(
                io::Error::other("passed W events did not reach another X11 client").into(),
            );
        }
        if observed_left != 0 {
            return Err(
                io::Error::other("consumed left-button events reached another X11 client").into(),
            );
        }
        if observed_right != 2 {
            return Err(
                io::Error::other("passed right-button gesture was not delivered intact").into(),
            );
        }
        if observed_motion != 0 {
            return Err(
                io::Error::other("consumed pointer motion reached another X11 client").into(),
            );
        }

        println!("\nSelective X11 grab verified.");
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    linux::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("x11_grab_detection is only available on Linux/X11");
}
