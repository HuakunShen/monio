#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Registrations {
    pub(crate) key_hook: bool,
    pub(crate) key_monitor: bool,
    pub(crate) mouse_monitor: bool,
    pub(crate) axis_monitor: bool,
}

pub(crate) trait RegistrationApi {
    fn add_key_hook(&mut self) -> Result<(), u32>;
    fn add_key_monitor(&mut self) -> Result<(), u32>;
    fn add_mouse_monitor(&mut self) -> Result<(), u32>;
    fn add_axis_monitor(&mut self) -> Result<(), u32>;
    fn remove_key_hook(&mut self) -> Result<(), u32>;
    fn remove_key_monitor(&mut self) -> Result<(), u32>;
    fn remove_mouse_monitor(&mut self) -> Result<(), u32>;
    fn remove_axis_monitor(&mut self) -> Result<(), u32>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistrationMode {
    Listen,
    Grab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandlerMode {
    Listen,
    Grab,
}

pub(crate) fn should_dispatch_original(
    mode: HandlerMode,
    handler_returned_some: bool,
    handler_panicked: bool,
    handler_available: bool,
) -> bool {
    mode == HandlerMode::Grab && (handler_returned_some || handler_panicked || !handler_available)
}

pub(crate) fn register<A: RegistrationApi>(
    api: &mut A,
    mode: RegistrationMode,
) -> Result<Registrations, u32> {
    let mut registrations = Registrations::default();

    let first_result = match mode {
        RegistrationMode::Listen => api.add_key_monitor().map(|()| {
            registrations.key_monitor = true;
        }),
        RegistrationMode::Grab => api.add_key_hook().map(|()| {
            registrations.key_hook = true;
        }),
    };
    first_result?;

    if let Err(code) = api.add_mouse_monitor() {
        let _ = unregister(api, &mut registrations);
        return Err(code);
    }
    registrations.mouse_monitor = true;

    if let Err(code) = api.add_axis_monitor() {
        let _ = unregister(api, &mut registrations);
        return Err(code);
    }
    registrations.axis_monitor = true;

    Ok(registrations)
}

pub(crate) fn unregister<A: RegistrationApi>(
    api: &mut A,
    registrations: &mut Registrations,
) -> Result<(), u32> {
    let mut first_error = None;

    if registrations.axis_monitor {
        registrations.axis_monitor = false;
        remember_first_error(&mut first_error, api.remove_axis_monitor());
    }
    if registrations.mouse_monitor {
        registrations.mouse_monitor = false;
        remember_first_error(&mut first_error, api.remove_mouse_monitor());
    }
    if registrations.key_monitor {
        registrations.key_monitor = false;
        remember_first_error(&mut first_error, api.remove_key_monitor());
    }
    if registrations.key_hook {
        registrations.key_hook = false;
        remember_first_error(&mut first_error, api.remove_key_hook());
    }

    match first_error {
        Some(code) => Err(code),
        None => Ok(()),
    }
}

fn remember_first_error(first_error: &mut Option<u32>, result: Result<(), u32>) {
    if let Err(code) = result
        && first_error.is_none()
    {
        *first_error = Some(code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeApi {
        calls: Vec<&'static str>,
        fail_add: Option<&'static str>,
        fail_remove: Option<&'static str>,
    }

    impl FakeApi {
        fn call(&mut self, operation: &'static str, is_remove: bool) -> Result<(), u32> {
            self.calls.push(operation);
            let configured_failure = if is_remove {
                self.fail_remove
            } else {
                self.fail_add
            };

            if configured_failure == Some(operation) {
                Err(if is_remove { 902 } else { 901 })
            } else {
                Ok(())
            }
        }
    }

    impl RegistrationApi for FakeApi {
        fn add_key_hook(&mut self) -> Result<(), u32> {
            self.call("add-key-hook", false)
        }

        fn add_key_monitor(&mut self) -> Result<(), u32> {
            self.call("add-key-monitor", false)
        }

        fn add_mouse_monitor(&mut self) -> Result<(), u32> {
            self.call("add-mouse-monitor", false)
        }

        fn add_axis_monitor(&mut self) -> Result<(), u32> {
            self.call("add-axis-monitor", false)
        }

        fn remove_key_hook(&mut self) -> Result<(), u32> {
            self.call("remove-key-hook", true)
        }

        fn remove_key_monitor(&mut self) -> Result<(), u32> {
            self.call("remove-key-monitor", true)
        }

        fn remove_mouse_monitor(&mut self) -> Result<(), u32> {
            self.call("remove-mouse-monitor", true)
        }

        fn remove_axis_monitor(&mut self) -> Result<(), u32> {
            self.call("remove-axis-monitor", true)
        }
    }

    #[test]
    fn listen_and_grab_register_their_exact_sources() {
        let mut listen = FakeApi::default();
        let listen_registrations =
            register(&mut listen, RegistrationMode::Listen).expect("listen should register");
        assert_eq!(
            listen.calls,
            ["add-key-monitor", "add-mouse-monitor", "add-axis-monitor"]
        );
        assert_eq!(
            listen_registrations,
            Registrations {
                key_monitor: true,
                mouse_monitor: true,
                axis_monitor: true,
                ..Registrations::default()
            }
        );

        let mut grab = FakeApi::default();
        let grab_registrations =
            register(&mut grab, RegistrationMode::Grab).expect("grab should register");
        assert_eq!(
            grab.calls,
            ["add-key-hook", "add-mouse-monitor", "add-axis-monitor"]
        );
        assert_eq!(
            grab_registrations,
            Registrations {
                key_hook: true,
                mouse_monitor: true,
                axis_monitor: true,
                ..Registrations::default()
            }
        );
    }

    #[test]
    fn registration_failure_rolls_back_completed_steps_in_reverse_order() {
        let mut listen = FakeApi {
            fail_add: Some("add-mouse-monitor"),
            ..FakeApi::default()
        };
        assert_eq!(register(&mut listen, RegistrationMode::Listen), Err(901));
        assert_eq!(
            listen.calls,
            ["add-key-monitor", "add-mouse-monitor", "remove-key-monitor"]
        );

        let mut grab = FakeApi {
            fail_add: Some("add-axis-monitor"),
            ..FakeApi::default()
        };
        assert_eq!(register(&mut grab, RegistrationMode::Grab), Err(901));
        assert_eq!(
            grab.calls,
            [
                "add-key-hook",
                "add-mouse-monitor",
                "add-axis-monitor",
                "remove-mouse-monitor",
                "remove-key-hook"
            ]
        );
    }

    #[test]
    fn cleanup_is_reverse_order_idempotent_and_keeps_going_after_error() {
        let mut api = FakeApi {
            fail_remove: Some("remove-mouse-monitor"),
            ..FakeApi::default()
        };
        let mut registrations = Registrations {
            key_hook: true,
            key_monitor: false,
            mouse_monitor: true,
            axis_monitor: true,
        };

        assert_eq!(unregister(&mut api, &mut registrations), Err(902));
        assert_eq!(
            api.calls,
            [
                "remove-axis-monitor",
                "remove-mouse-monitor",
                "remove-key-hook"
            ]
        );
        assert_eq!(registrations, Registrations::default());

        assert_eq!(unregister(&mut api, &mut registrations), Ok(()));
        assert_eq!(api.calls.len(), 3);
    }

    #[test]
    fn dispatch_policy_consumes_only_an_explicit_grab_none() {
        assert!(should_dispatch_original(
            HandlerMode::Grab,
            true,
            false,
            true
        ));
        assert!(!should_dispatch_original(
            HandlerMode::Grab,
            false,
            false,
            true
        ));
        assert!(should_dispatch_original(
            HandlerMode::Grab,
            false,
            true,
            true
        ));
        assert!(should_dispatch_original(
            HandlerMode::Grab,
            false,
            false,
            false
        ));
        assert!(!should_dispatch_original(
            HandlerMode::Listen,
            true,
            true,
            false
        ));
    }
}
