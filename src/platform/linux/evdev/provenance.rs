use crate::{InjectorIdentity, InputOrigin};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::RawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::PathBuf;

/// Exact kernel device identities owned by this process's uinput injector.
///
/// Device names and advertised input IDs are caller-controlled metadata. The
/// character-device number identifies the live `/dev/input/event*` node that
/// belongs to the `VirtualDevice` handle Monio owns.
#[derive(Clone, Debug)]
pub(super) struct InjectorDeviceIdentity {
    device_numbers: HashSet<u64>,
}

impl InjectorDeviceIdentity {
    pub(super) fn from_event_nodes(paths: &[PathBuf]) -> io::Result<Self> {
        let mut device_numbers = HashSet::new();

        for path in paths {
            let metadata = fs::metadata(path)?;
            if !metadata.file_type().is_char_device() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} is not an input character device", path.display()),
                ));
            }
            device_numbers.insert(metadata.rdev());
        }

        if device_numbers.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "uinput device has no /dev/input/event* node",
            ));
        }

        Ok(Self { device_numbers })
    }

    pub(super) fn event_origin(&self, fd: RawFd) -> io::Result<InputOrigin> {
        let mut metadata = MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `metadata` points to writable storage for one `libc::stat`,
        // and it is read only after `fstat` reports success.
        if unsafe { libc::fstat(fd, metadata.as_mut_ptr()) } == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a successful `fstat` initialized the complete structure.
        let device_number = unsafe { metadata.assume_init() }.st_rdev;

        Ok(if self.device_numbers.contains(&device_number) {
            InputOrigin::Injected {
                injector: InjectorIdentity::ThisMonioSession,
            }
        } else {
            InputOrigin::Unknown
        })
    }
}
