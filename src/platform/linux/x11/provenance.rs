use crate::{InjectorIdentity, InputOrigin};
use std::collections::VecDeque;
use std::os::raw::{c_int, c_ulong};
use std::slice;
use x11::{xlib, xrecord};

const XTEST_FAKE_INPUT_MINOR_OPCODE: u8 = 2;
const XTEST_FAKE_INPUT_REQUEST_BYTES: usize = 36;
const MAX_PENDING_EVENTS: usize = 64;

#[derive(Clone, Copy, Debug)]
struct ExpectedDeviceEvent {
    type_: u8,
    detail: Option<u8>,
}

impl ExpectedDeviceEvent {
    fn from_xtest_request(request: &[u8]) -> Option<Self> {
        if request.len() < XTEST_FAKE_INPUT_REQUEST_BYTES {
            return None;
        }

        let type_ = request[4];
        match type_ as c_int {
            xlib::KeyPress | xlib::KeyRelease | xlib::ButtonPress | xlib::ButtonRelease => {
                Some(Self {
                    type_,
                    detail: Some(request[5]),
                })
            }
            xlib::MotionNotify => Some(Self {
                type_,
                detail: None,
            }),
            _ => None,
        }
    }

    fn matches(self, type_: u8, detail: u8) -> bool {
        self.type_ == type_ && self.detail.is_none_or(|expected| expected == detail)
    }
}

/// Correlates requests from Monio's persistent XTest client with device events.
///
/// XRecord identifies client requests by their X11 resource-ID base, but
/// device events intentionally have no client identity. The RECORD protocol
/// reports requests immediately before execution, and XTestFakeInput produces
/// its device event synchronously in that server order. Any mismatch clears
/// the queue so ambiguous input remains `Unknown`.
pub(super) struct RequestCorrelation {
    injector_client_id_base: c_ulong,
    xtest_major_opcode: u8,
    pending: VecDeque<ExpectedDeviceEvent>,
}

impl RequestCorrelation {
    pub(super) fn new(injector_client_id_base: c_ulong, xtest_major_opcode: u8) -> Self {
        Self {
            injector_client_id_base,
            xtest_major_opcode,
            pending: VecDeque::new(),
        }
    }

    pub(super) fn observe_request(&mut self, data: &xrecord::XRecordInterceptData) {
        if data.id_base != self.injector_client_id_base
            || data.client_swapped != 0
            || data.data.is_null()
        {
            return;
        }

        let byte_len = match usize::try_from(data.data_len)
            .ok()
            .and_then(|units| units.checked_mul(4))
        {
            Some(byte_len) => byte_len,
            None => {
                self.pending.clear();
                return;
            }
        };

        // SAFETY: XRecord owns `data.data` for `data_len` four-byte units
        // during this callback. The slice is not retained.
        let request = unsafe { slice::from_raw_parts(data.data, byte_len) };
        if request.first() != Some(&self.xtest_major_opcode)
            || request.get(1) != Some(&XTEST_FAKE_INPUT_MINOR_OPCODE)
        {
            return;
        }

        let Some(expected) = ExpectedDeviceEvent::from_xtest_request(request) else {
            self.pending.clear();
            return;
        };

        if self.pending.len() == MAX_PENDING_EVENTS {
            self.pending.clear();
        }
        self.pending.push_back(expected);
    }

    pub(super) fn classify_device_event(&mut self, type_: u8, detail: u8) -> InputOrigin {
        let Some(expected) = self.pending.pop_front() else {
            return InputOrigin::Unknown;
        };

        if expected.matches(type_, detail) {
            InputOrigin::Injected {
                injector: InjectorIdentity::ThisMonioSession,
            }
        } else {
            self.pending.clear();
            InputOrigin::Unknown
        }
    }
}
