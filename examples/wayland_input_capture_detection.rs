//! Native Wayland InputCapture portal and EIS diagnostic.
//!
//! Run with:
//! `cargo run --no-default-features --features wayland-portal,x11 \
//!     --example wayland_input_capture_detection`
//!
//! Add `-- --inject-self-test` to verify whether RemoteDesktop/EIS target
//! injection can activate an armed InputCapture barrier and whether injected
//! events echo into the active InputCapture session.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This diagnostic is only available on Linux.");
}

#[cfg(target_os = "linux")]
mod linux {
    use ashpd::WindowIdentifier;
    use ashpd::desktop::input_capture::{
        Barrier, BarrierID, Capabilities, CreateSessionOptions, InputCapture, ReleaseOptions,
        StartOptions,
    };
    use ashpd::desktop::remote_desktop::{
        ConnectToEISOptions as RemoteConnectToEISOptions, DeviceType, RemoteDesktop,
        SelectDevicesOptions, StartOptions as RemoteStartOptions,
    };
    use ashpd::desktop::{PersistMode, Session};
    use futures_util::StreamExt;
    use reis::event::{Connection, Device, DeviceCapability, EiEvent};
    use reis::{ei, tokio::EiConvertEventStream};
    use std::error::Error;
    use std::ffi::CString;
    use std::os::unix::net::UnixStream;
    use std::ptr;
    use std::time::Duration;
    use x11::xlib;

    struct PortalParentWindow {
        display: *mut xlib::Display,
        window: xlib::Window,
    }

    impl PortalParentWindow {
        fn new() -> Result<Self, Box<dyn Error>> {
            let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
            if display.is_null() {
                return Err("cannot open XWayland display for the portal parent window".into());
            }

            let screen = unsafe { xlib::XDefaultScreen(display) };
            let root = unsafe { xlib::XRootWindow(display, screen) };
            let black = unsafe { xlib::XBlackPixel(display, screen) };
            let white = unsafe { xlib::XWhitePixel(display, screen) };
            let window = unsafe {
                xlib::XCreateSimpleWindow(display, root, 64, 64, 520, 120, 1, black, white)
            };
            if window == 0 {
                unsafe { xlib::XCloseDisplay(display) };
                return Err("cannot create XWayland portal parent window".into());
            }

            let title = CString::new("Monio Wayland InputCapture diagnostic")?;
            unsafe {
                xlib::XStoreName(display, window, title.as_ptr());
                xlib::XMapRaised(display, window);
                xlib::XSync(display, xlib::False);
            }

            Ok(Self { display, window })
        }

        fn identifier(&self) -> WindowIdentifier {
            WindowIdentifier::from_xid(self.window)
        }

        fn pointer_position(&self) -> Result<(i32, i32), Box<dyn Error>> {
            let screen = unsafe { xlib::XDefaultScreen(self.display) };
            let root = unsafe { xlib::XRootWindow(self.display, screen) };
            let mut root_return = 0;
            let mut child_return = 0;
            let mut root_x = 0;
            let mut root_y = 0;
            let mut window_x = 0;
            let mut window_y = 0;
            let mut mask = 0;
            let result = unsafe {
                xlib::XQueryPointer(
                    self.display,
                    root,
                    &mut root_return,
                    &mut child_return,
                    &mut root_x,
                    &mut root_y,
                    &mut window_x,
                    &mut window_y,
                    &mut mask,
                )
            };
            if result == xlib::False {
                return Err("XWayland could not query the compositor pointer position".into());
            }
            Ok((root_x, root_y))
        }

        fn root_dimensions(&self) -> (i32, i32) {
            let screen = unsafe { xlib::XDefaultScreen(self.display) };
            (
                unsafe { xlib::XDisplayWidth(self.display, screen) },
                unsafe { xlib::XDisplayHeight(self.display, screen) },
            )
        }

        fn cover_pointer(&self) -> Result<(), Box<dyn Error>> {
            let pointer = self.pointer_position()?;
            let dimensions = self.root_dimensions();
            let width = 960_i32.min(dimensions.0);
            let height = 720_i32.min(dimensions.1);
            let x = (pointer.0 - width / 2).clamp(0, dimensions.0 - width);
            let y = (pointer.1 - height / 2).clamp(0, dimensions.1 - height);
            unsafe {
                xlib::XMoveResizeWindow(
                    self.display,
                    self.window,
                    x,
                    y,
                    width as u32,
                    height as u32,
                );
                xlib::XMapRaised(self.display, self.window);
                xlib::XSync(self.display, xlib::False);
            }
            Ok(())
        }
    }

    impl Drop for PortalParentWindow {
        fn drop(&mut self) {
            unsafe {
                xlib::XDestroyWindow(self.display, self.window);
                xlib::XCloseDisplay(self.display);
            }
        }
    }

    #[derive(Default)]
    struct EchoEvidence {
        motion_out: bool,
        motion_back: bool,
        key_press: bool,
        key_release: bool,
        button_press: bool,
        button_release: bool,
        scroll: bool,
    }

    impl EchoEvidence {
        fn any(&self) -> bool {
            self.motion_out
                || self.motion_back
                || self.key_press
                || self.key_release
                || self.button_press
                || self.button_release
                || self.scroll
        }

        fn print(&self) {
            println!(
                "Injected relative motion echoed: {}",
                self.motion_out || self.motion_back
            );
            println!(
                "Injected keyboard echoed:        {}",
                self.key_press || self.key_release
            );
            println!(
                "Injected button echoed:          {}",
                self.button_press || self.button_release
            );
            println!("Injected scroll echoed:          {}", self.scroll);
        }
    }

    #[derive(Default)]
    struct CapturedCounts {
        relative_motion: u64,
        printed_motion: u8,
        keys: u64,
        buttons: u64,
        scroll: u64,
        echo: EchoEvidence,
    }

    impl CapturedCounts {
        fn total(&self) -> u64 {
            self.relative_motion + self.keys + self.buttons + self.scroll
        }
    }

    fn handle_eis_event(
        context: &ei::Context,
        event: reis::event::EiEvent,
        counts: &mut CapturedCounts,
    ) -> Result<(), Box<dyn Error>> {
        match event {
            reis::event::EiEvent::SeatAdded(event) => {
                println!("EIS seat added: {:?}", event.seat.name());
                event.seat.bind_capabilities(
                    DeviceCapability::Pointer
                        | DeviceCapability::Keyboard
                        | DeviceCapability::Scroll
                        | DeviceCapability::Button,
                );
                context.flush()?;
            }
            reis::event::EiEvent::DeviceAdded(event) => {
                println!(
                    "EIS device added: name={:?}, type={:?}",
                    event.device.name(),
                    event.device.device_type()
                );
            }
            reis::event::EiEvent::PointerMotion(event) => {
                counts.relative_motion += 1;
                counts.echo.motion_out |=
                    approximately(event.dx, ECHO_DX) && approximately(event.dy, ECHO_DY);
                counts.echo.motion_back |=
                    approximately(event.dx, -ECHO_DX) && approximately(event.dy, -ECHO_DY);
                if counts.printed_motion < 12 {
                    counts.printed_motion += 1;
                    println!(
                        "PointerMotion: relative=({:.3}, {:.3}) device_type={:?}",
                        event.dx,
                        event.dy,
                        event.device.device_type()
                    );
                    if counts.printed_motion == 12 {
                        println!("Further pointer motion is counted without per-event output.");
                    }
                }
            }
            reis::event::EiEvent::KeyboardKey(event) => {
                counts.keys += 1;
                if event.key == ECHO_KEY {
                    counts.echo.key_press |= event.state == ei::keyboard::KeyState::Press;
                    counts.echo.key_release |= event.state == ei::keyboard::KeyState::Released;
                }
                println!(
                    "KeyboardKey: key={} state={:?} device_type={:?}",
                    event.key,
                    event.state,
                    event.device.device_type()
                );
            }
            reis::event::EiEvent::Button(event) => {
                counts.buttons += 1;
                if event.button == ECHO_BUTTON {
                    counts.echo.button_press |= event.state == ei::button::ButtonState::Press;
                    counts.echo.button_release |= event.state == ei::button::ButtonState::Released;
                }
                println!(
                    "Button: button={} state={:?} device_type={:?}",
                    event.button,
                    event.state,
                    event.device.device_type()
                );
            }
            reis::event::EiEvent::ScrollDelta(event) => {
                counts.scroll += 1;
                println!(
                    "Scroll: delta=({:.3}, {:.3}) device_type={:?}",
                    event.dx,
                    event.dy,
                    event.device.device_type()
                );
            }
            reis::event::EiEvent::ScrollDiscrete(event) => {
                counts.scroll += 1;
                counts.echo.scroll |= event.discrete_dx == 0 && event.discrete_dy == ECHO_SCROLL;
                println!(
                    "Scroll: discrete=({}, {}) device_type={:?}",
                    event.discrete_dx,
                    event.discrete_dy,
                    event.device.device_type()
                );
            }
            reis::event::EiEvent::Disconnected(event) => {
                return Err(format!(
                    "EIS disconnected: reason={:?}, explanation={:?}",
                    event.reason, event.explanation
                )
                .into());
            }
            _ => {}
        }

        Ok(())
    }

    const ECHO_DX: f32 = 37.0;
    const ECHO_DY: f32 = -23.0;
    const ECHO_KEY: u32 = 30;
    const ECHO_BUTTON: u32 = 272;
    const ECHO_SCROLL: i32 = 120;

    fn approximately(value: f32, expected: f32) -> bool {
        (value - expected).abs() < 0.01
    }

    struct SenderDevice {
        device: Device,
        resumed: bool,
        emulating: bool,
    }

    struct EisSender {
        _portal: RemoteDesktop,
        _session: Session<RemoteDesktop>,
        connection: Connection,
        events: EiConvertEventStream,
        devices: Vec<SenderDevice>,
        next_sequence: u32,
    }

    impl EisSender {
        async fn connect(parent: &WindowIdentifier) -> Result<Self, Box<dyn Error>> {
            let portal = RemoteDesktop::new().await?;
            if portal.version() < 2 {
                return Err(format!(
                    "RemoteDesktop portal version {} does not support ConnectToEIS",
                    portal.version()
                )
                .into());
            }

            let session = portal.create_session(Default::default()).await?;
            portal
                .select_devices(
                    &session,
                    SelectDevicesOptions::default()
                        .set_devices(DeviceType::Keyboard | DeviceType::Pointer)
                        .set_persist_mode(PersistMode::DoNot),
                )
                .await?
                .response()?;
            let granted = portal
                .start(&session, Some(parent), RemoteStartOptions::default())
                .await?
                .response()?;
            let required = DeviceType::Keyboard | DeviceType::Pointer;
            if !granted.devices().contains(required) {
                return Err(format!(
                    "RemoteDesktop did not grant keyboard and pointer: {:?}",
                    granted.devices()
                )
                .into());
            }
            println!(
                "RemoteDesktop v{} session started; granted={:?}.",
                portal.version(),
                granted.devices()
            );

            let fd = portal
                .connect_to_eis(&session, RemoteConnectToEISOptions::default())
                .await?;
            let context = ei::Context::new(UnixStream::from(fd))?;
            let (connection, events) = context
                .handshake_tokio(
                    "monio-wayland-remote-desktop-diagnostic",
                    ei::handshake::ContextType::Sender,
                )
                .await?;
            let mut sender = Self {
                _portal: portal,
                _session: session,
                connection,
                events,
                devices: Vec::new(),
                next_sequence: 1,
            };
            sender.wait_until_ready().await?;
            sender.start_emulating()?;
            sender.drain_events().await?;
            if !sender.all_capabilities_ready() {
                return Err("an EIS sender device paused after start_emulating".into());
            }
            Ok(sender)
        }

        fn has_ready_capability(&self, capability: DeviceCapability) -> bool {
            self.devices
                .iter()
                .any(|entry| entry.resumed && entry.device.has_capability(capability))
        }

        fn all_capabilities_ready(&self) -> bool {
            [
                DeviceCapability::Pointer,
                DeviceCapability::Keyboard,
                DeviceCapability::Button,
                DeviceCapability::Scroll,
            ]
            .into_iter()
            .all(|capability| self.has_ready_capability(capability))
        }

        async fn wait_until_ready(&mut self) -> Result<(), Box<dyn Error>> {
            let deadline = tokio::time::sleep(Duration::from_secs(8));
            tokio::pin!(deadline);
            while !self.all_capabilities_ready() {
                tokio::select! {
                    _ = &mut deadline => {
                        return Err("timed out waiting for EIS sender devices to resume".into());
                    }
                    Some(event) = self.events.next() => self.handle_event(event?)?,
                    else => return Err("EIS sender stream ended during device setup".into()),
                }
            }
            println!("EIS sender devices are ready for pointer, keyboard, button, and scroll.");
            Ok(())
        }

        fn handle_event(&mut self, event: EiEvent) -> Result<(), Box<dyn Error>> {
            match event {
                EiEvent::SeatAdded(event) => {
                    println!("Sender EIS seat added: {:?}", event.seat.name());
                    event.seat.bind_capabilities(
                        DeviceCapability::Pointer
                            | DeviceCapability::PointerAbsolute
                            | DeviceCapability::Keyboard
                            | DeviceCapability::Button
                            | DeviceCapability::Scroll,
                    );
                    self.connection.flush()?;
                }
                EiEvent::DeviceAdded(event) => {
                    println!(
                        "Sender EIS device added: name={:?}, type={:?}, relative_pointer={}, absolute_pointer={}, keyboard={}, button={}, scroll={}",
                        event.device.name(),
                        event.device.device_type(),
                        event.device.has_capability(DeviceCapability::Pointer),
                        event
                            .device
                            .has_capability(DeviceCapability::PointerAbsolute),
                        event.device.has_capability(DeviceCapability::Keyboard),
                        event.device.has_capability(DeviceCapability::Button),
                        event.device.has_capability(DeviceCapability::Scroll),
                    );
                    if event.device.device().version() >= 3 {
                        event.device.device().ready();
                        self.connection.flush()?;
                    }
                    self.devices.push(SenderDevice {
                        device: event.device,
                        resumed: false,
                        emulating: false,
                    });
                }
                EiEvent::DeviceResumed(event) => {
                    println!(
                        "Sender EIS device resumed: name={:?}, serial={}",
                        event.device.name(),
                        event.serial
                    );
                    if let Some(entry) = self
                        .devices
                        .iter_mut()
                        .find(|entry| entry.device == event.device)
                    {
                        entry.resumed = true;
                    }
                }
                EiEvent::DevicePaused(event) => {
                    println!(
                        "Sender EIS device paused: name={:?}, serial={}",
                        event.device.name(),
                        event.serial
                    );
                    if let Some(entry) = self
                        .devices
                        .iter_mut()
                        .find(|entry| entry.device == event.device)
                    {
                        entry.resumed = false;
                        entry.emulating = false;
                    }
                }
                EiEvent::DeviceRemoved(event) => {
                    self.devices.retain(|entry| entry.device != event.device);
                }
                EiEvent::Disconnected(event) => {
                    return Err(format!(
                        "sender EIS disconnected: reason={:?}, explanation={:?}",
                        event.reason, event.explanation
                    )
                    .into());
                }
                _ => {}
            }
            Ok(())
        }

        fn start_emulating(&mut self) -> Result<(), Box<dyn Error>> {
            for entry in &mut self.devices {
                if entry.resumed && !entry.emulating {
                    entry
                        .device
                        .device()
                        .start_emulating(self.connection.serial(), self.next_sequence);
                    self.next_sequence = self.next_sequence.wrapping_add(1);
                    entry.emulating = true;
                }
            }
            self.connection.flush()?;
            Ok(())
        }

        fn device_for(&self, capability: DeviceCapability) -> Result<&Device, Box<dyn Error>> {
            self.devices
                .iter()
                .find(|entry| {
                    entry.resumed && entry.emulating && entry.device.has_capability(capability)
                })
                .map(|entry| &entry.device)
                .ok_or_else(|| format!("no active EIS device for {capability:?}").into())
        }

        fn frame(&self, device: &Device) -> Result<(), Box<dyn Error>> {
            device
                .device()
                .frame(self.connection.serial(), monotonic_micros()?);
            self.connection.flush()?;
            Ok(())
        }

        fn pointer_motion(&self, dx: f32, dy: f32) -> Result<(), Box<dyn Error>> {
            let device = self.device_for(DeviceCapability::Pointer)?;
            device
                .interface::<ei::Pointer>()
                .ok_or("EIS pointer interface disappeared")?
                .motion_relative(dx, dy);
            self.frame(device)
        }

        fn pointer_motion_absolute(&self, x: f32, y: f32) -> Result<(), Box<dyn Error>> {
            let device = self.device_for(DeviceCapability::PointerAbsolute)?;
            device
                .interface::<ei::PointerAbsolute>()
                .ok_or("EIS absolute-pointer interface disappeared")?
                .motion_absolute(x, y);
            self.frame(device)
        }

        async fn inject_barrier_crossing(
            &self,
            right_x: i32,
            top_y: i32,
            bottom_y: i32,
        ) -> Result<(), Box<dyn Error>> {
            let x = (right_x - 40) as f32;
            let y = (top_y + (bottom_y - top_y) / 2) as f32;
            println!(
                "Testing the armed barrier with target injection: absolute=({x}, {y}), then relative=(120, 0)."
            );
            self.pointer_motion_absolute(x, y)?;
            tokio::time::sleep(Duration::from_millis(300)).await;
            self.pointer_motion(120.0, 0.0)?;
            Ok(())
        }

        async fn pointer_preflight(
            &self,
            parent: &PortalParentWindow,
        ) -> Result<(), Box<dyn Error>> {
            parent.cover_pointer()?;
            tokio::time::sleep(Duration::from_millis(250)).await;
            let before = parent.pointer_position()?;
            let dimensions = parent.root_dimensions();
            let dx = if before.0 < dimensions.0 / 2 {
                180.0
            } else {
                -180.0
            };
            let dy = if before.1 < dimensions.1 / 2 {
                120.0
            } else {
                -120.0
            };
            println!("EIS preflight: moving ({dx}, {dy}) for 700 ms.");
            self.pointer_motion(dx, dy)?;
            tokio::time::sleep(Duration::from_millis(700)).await;
            let eis_moved = parent.pointer_position()?;
            self.pointer_motion(-dx, -dy)?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            let eis_restored = parent.pointer_position()?;
            println!(
                "RemoteDesktop pointer preflight:\n  before={before:?}\n  EIS moved={eis_moved:?}, restored={eis_restored:?}"
            );
            if eis_moved == before {
                println!(
                    "XQueryPointer did not observe EIS injection; visual output and the capture echo test will provide the remaining evidence."
                );
            }
            Ok(())
        }

        async fn inject_echo_sequence(&self) -> Result<(), Box<dyn Error>> {
            println!("Injecting the distinctive EIS target sequence now.");
            self.pointer_motion(ECHO_DX, ECHO_DY)?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            self.pointer_motion(-ECHO_DX, -ECHO_DY)?;

            let keyboard_device = self.device_for(DeviceCapability::Keyboard)?;
            let keyboard = keyboard_device
                .interface::<ei::Keyboard>()
                .ok_or("EIS keyboard interface disappeared")?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            keyboard.key(ECHO_KEY, ei::keyboard::KeyState::Press);
            self.frame(keyboard_device)?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            keyboard.key(ECHO_KEY, ei::keyboard::KeyState::Released);
            self.frame(keyboard_device)?;

            let button_device = self.device_for(DeviceCapability::Button)?;
            let button = button_device
                .interface::<ei::Button>()
                .ok_or("EIS button interface disappeared")?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            button.button(ECHO_BUTTON, ei::button::ButtonState::Press);
            self.frame(button_device)?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            button.button(ECHO_BUTTON, ei::button::ButtonState::Released);
            self.frame(button_device)?;

            let scroll_device = self.device_for(DeviceCapability::Scroll)?;
            let scroll = scroll_device
                .interface::<ei::Scroll>()
                .ok_or("EIS scroll interface disappeared")?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            scroll.scroll_discrete(0, ECHO_SCROLL);
            self.frame(scroll_device)?;
            Ok(())
        }

        async fn drain_events(&mut self) -> Result<(), Box<dyn Error>> {
            let deadline = tokio::time::sleep(Duration::from_millis(300));
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    _ = &mut deadline => return Ok(()),
                    Some(event) = self.events.next() => self.handle_event(event?)?,
                    else => return Err("EIS sender stream ended after injection".into()),
                }
            }
        }

        fn stop_emulating(&mut self) -> Result<(), Box<dyn Error>> {
            for entry in &mut self.devices {
                if entry.emulating {
                    entry
                        .device
                        .device()
                        .stop_emulating(self.connection.serial());
                    entry.emulating = false;
                }
            }
            self.connection.flush()?;
            Ok(())
        }
    }

    fn monotonic_micros() -> Result<u64, Box<dyn Error>> {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(time.tv_sec as u64 * 1_000_000 + time.tv_nsec as u64 / 1_000)
    }

    pub async fn run() -> Result<(), Box<dyn Error>> {
        if std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("wayland") {
            return Err("this diagnostic requires a native Wayland session".into());
        }

        let inject_self_test = std::env::args().any(|argument| argument == "--inject-self-test");
        println!("monio Wayland InputCapture diagnostic");
        println!("======================================");
        println!("The portal may ask for permission.");
        println!("After it is armed, push the pointer through the rightmost screen edge.");
        if inject_self_test {
            println!("RemoteDesktop/EIS will first try to activate the armed barrier.");
            println!("Keep your hands off the keyboard and mouse during that automatic test.");
            println!("It will inject a distinctive sequence after activation.");
        } else {
            println!("Move, click, scroll, and type during the eight-second capture window.");
        }
        println!("The diagnostic will then release capture and exit.\n");

        let parent_window = PortalParentWindow::new()?;
        let parent_identifier = parent_window.identifier();
        println!("Portal parent window: {parent_identifier}");

        let input_capture = InputCapture::new().await?;
        let required = Capabilities::Keyboard | Capabilities::Pointer;
        println!("Requesting portal capabilities: {required:?}");

        let session = match input_capture.create_session2(Default::default()).await {
            Ok(session) => {
                let request = input_capture
                    .start(
                        &session,
                        Some(&parent_identifier),
                        StartOptions::default().set_capabilities(required),
                    )
                    .await?;
                request.response()?;
                println!("InputCapture v2 session started.");
                session
            }
            Err(ashpd::Error::RequiresVersion(_, _)) => {
                let (session, granted) = input_capture
                    .create_session(
                        Some(&parent_identifier),
                        CreateSessionOptions::default().set_capabilities(required),
                    )
                    .await?;
                println!("InputCapture v1 session started; granted={granted:?}.");
                session
            }
            Err(error) => return Err(error.into()),
        };
        let zones = input_capture
            .zones(&session, Default::default())
            .await?
            .response()?;
        println!("Portal zones: {:?}", zones.regions());

        let rightmost = zones
            .regions()
            .iter()
            .max_by_key(|zone| zone.x_offset() + zone.width() as i32)
            .ok_or("portal returned no capture zones")?;
        let right_x = rightmost.x_offset() + rightmost.width() as i32;
        let top_y = rightmost.y_offset();
        let bottom_y = top_y + rightmost.height() as i32 - 1;
        let barrier_id = BarrierID::new(1).expect("one is non-zero");
        let barrier = Barrier::new(barrier_id, (right_x, top_y, right_x, bottom_y));

        let response = input_capture
            .set_pointer_barriers(&session, &[barrier], zones.zone_set(), Default::default())
            .await?
            .response()?;
        if !response.failed_barriers().is_empty() {
            return Err(format!(
                "portal rejected the right-edge barrier: {:?}",
                response.failed_barriers()
            )
            .into());
        }
        println!("Right-edge barrier armed at x={right_x}, y={top_y}..={bottom_y}.");

        let eis_fd = input_capture
            .connect_to_eis(&session, Default::default())
            .await?;
        let context = ei::Context::new(UnixStream::from(eis_fd))?;
        let (_connection, mut eis_events) = context
            .handshake_tokio(
                "monio-wayland-diagnostic",
                ei::handshake::ContextType::Receiver,
            )
            .await?;
        let mut activated = input_capture.receive_activated().await?.fuse();
        let mut deactivated = input_capture.receive_deactivated().await?.fuse();

        let mut sender = if inject_self_test {
            println!("\nStarting the RemoteDesktop/EIS sender self-test.");
            let sender = EisSender::connect(&parent_identifier).await?;
            sender.pointer_preflight(&parent_window).await?;
            println!("RemoteDesktop/EIS target pointer injection is working.\n");
            Some(sender)
        } else {
            None
        };

        let mut ignored_counts = CapturedCounts::default();
        if let Some(sender) = sender.as_ref() {
            println!("Testing target injection while InputCapture is disabled.");
            sender
                .inject_barrier_crossing(right_x, top_y, bottom_y)
                .await?;
            let deadline = tokio::time::sleep(Duration::from_secs(2));
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    _ = &mut deadline => break,
                    Some(signal) = activated.next() => {
                        return Err(format!(
                            "target injection activated disabled InputCapture: {:?}",
                            signal.activation_id()
                        ).into());
                    }
                    Some(event) = eis_events.next() => {
                        handle_eis_event(&context, event?, &mut ignored_counts)?;
                    }
                    else => return Err("portal or EIS stream ended during disabled-capture test".into()),
                }
            }
            println!("Disabled InputCapture resisted the injected barrier crossing.\n");
        }

        input_capture.enable(&session, Default::default()).await?;
        println!("Capture enabled.\n");

        let injected_activation = if let Some(sender) = sender.as_ref() {
            sender
                .inject_barrier_crossing(right_x, top_y, bottom_y)
                .await?;
            let deadline = tokio::time::sleep(Duration::from_secs(2));
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    _ = &mut deadline => break None,
                    Some(signal) = activated.next() => break Some(signal),
                    Some(signal) = deactivated.next() => {
                        return Err(format!(
                            "capture deactivated during injected barrier test: {:?}",
                            signal.activation_id()
                        ).into());
                    }
                    Some(event) = eis_events.next() => {
                        handle_eis_event(&context, event?, &mut ignored_counts)?;
                    }
                    else => return Err("portal or EIS stream ended during injected barrier test".into()),
                }
            }
        } else {
            None
        };

        let activation_was_injected = injected_activation.is_some();
        if activation_was_injected {
            println!("The RemoteDesktop/EIS target injection ACTIVATED the armed barrier.\n");
        } else {
            if inject_self_test {
                println!("Target injection did not activate the barrier within two seconds.");
            }
            println!("Push through the rightmost edge now.\n");
        }

        let activation = if let Some(activation) = injected_activation {
            activation
        } else {
            loop {
                tokio::select! {
                    Some(signal) = activated.next() => break signal,
                    Some(signal) = deactivated.next() => {
                        return Err(format!(
                            "capture deactivated before activation: {:?}",
                            signal.activation_id()
                        ).into());
                    }
                    Some(event) = eis_events.next() => {
                        handle_eis_event(&context, event?, &mut ignored_counts)?;
                    }
                    else => return Err("portal or EIS stream ended before activation".into()),
                }
            }
        };

        /*
         * Keep this output adjacent to the activation details: the distinction
         * decides whether a CrossFlow target may leave its source barrier armed.
         */
        println!("Capture ACTIVATED");
        println!("  triggered_by_target_injection={activation_was_injected}");
        println!("  activation_id={:?}", activation.activation_id());
        println!("  cursor_position={:?}", activation.cursor_position());
        println!("  barrier_id={:?}", activation.barrier_id());

        let capture_result: Result<CapturedCounts, Box<dyn Error>> = async {
            let mut counts = CapturedCounts::default();
            if let Some(sender) = sender.as_ref() {
                println!("Discarding the barrier-crossing tail for 500 ms.");
                let settle = tokio::time::sleep(Duration::from_millis(500));
                tokio::pin!(settle);
                loop {
                    tokio::select! {
                        _ = &mut settle => break,
                        Some(event) = eis_events.next() => {
                            handle_eis_event(&context, event?, &mut ignored_counts)?;
                        }
                        else => return Err("EIS receiver stream ended before injection".into()),
                    }
                }

                sender.inject_echo_sequence().await?;
                println!("Watching InputCapture for injected-event echo for two seconds.\n");
                let deadline = tokio::time::sleep(Duration::from_secs(2));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = &mut deadline => break,
                        Some(event) = eis_events.next() => {
                            handle_eis_event(&context, event?, &mut counts)?;
                        }
                        else => return Err("EIS receiver stream ended during echo test".into()),
                    }
                }
            } else {
                println!("Capturing input for eight seconds...\n");
                let deadline = tokio::time::sleep(Duration::from_secs(8));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = &mut deadline => break,
                        Some(event) = eis_events.next() => {
                            handle_eis_event(&context, event?, &mut counts)?;
                        }
                        else => return Err("EIS stream ended during capture".into()),
                    }
                }
            }
            Ok(counts)
        }
        .await;

        let sender_drain_result = if let Some(sender) = sender.as_mut() {
            sender.drain_events().await
        } else {
            Ok(())
        };
        let sender_stop_result = if let Some(sender) = sender.as_mut() {
            sender.stop_emulating()
        } else {
            Ok(())
        };
        let release_result = input_capture
            .release(
                &session,
                ReleaseOptions::default().set_activation_id(activation.activation_id()),
            )
            .await;
        println!("\nCapture released.");

        let counts = capture_result?;
        sender_drain_result?;
        sender_stop_result?;
        release_result?;
        println!("Results");
        println!("=======");
        println!("Barrier activated by target injection: {activation_was_injected}");
        println!("Relative motion events: {}", counts.relative_motion);
        println!("Keyboard events:        {}", counts.keys);
        println!("Button events:          {}", counts.buttons);
        println!("Scroll events:          {}", counts.scroll);

        if inject_self_test {
            counts.echo.print();
            if counts.echo.any() {
                let reason = if activation_was_injected {
                    "RemoteDesktop/EIS target injection activated the armed InputCapture barrier and echoed into capture; disable source capture before accepting an inbound CrossFlow lease"
                } else {
                    "RemoteDesktop/EIS injection echoed into InputCapture; CrossFlow must not retransmit it"
                };
                return Err(reason.into());
            }
            println!(
                "No injected target event echoed after this target-injected activation. A prior physical activation did echo target events, so this is not a general provenance guarantee."
            );
            if activation_was_injected {
                return Err(
                    "RemoteDesktop/EIS target injection activated InputCapture; disable source capture and barriers while an inbound CrossFlow lease is active"
                        .into(),
                );
            }
            println!("Wayland RemoteDesktop/EIS target injection is working.");
        } else if counts.total() == 0 {
            return Err("capture activated but no EIS input events were received".into());
        } else {
            println!("Wayland InputCapture and passive EIS receive are working.");
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match tokio::time::timeout(std::time::Duration::from_secs(60), linux::run()).await {
        Ok(result) => result,
        Err(_) => Err("timed out waiting for the Wayland portal or edge activation".into()),
    }
}
