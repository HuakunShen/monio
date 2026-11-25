use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use monio::{
    Button, Event, EventType, Hook, Key, MediaKey, RelativePointerCapture, ScrollPhase,
    display_at_point, displays, key_press, key_release, key_tap, media_key, mouse_click,
    mouse_move, mouse_move_relative, mouse_position, primary_display, scroll, system_settings,
    type_text,
};

use crate::model::{DisplaySnapshot, StatusBanner};
use crate::platform_info::select_all_modifier_key;

#[derive(Debug)]
pub enum WorkerMessage {
    HookEvent {
        event: Event,
        blocked: bool,
    },
    HookEnded {
        result: Result<(), monio::Error>,
    },
    ActionFinished {
        action: String,
        result: Result<(), monio::Error>,
    },
    DisplayRefreshed(DisplaySnapshot),
}

#[derive(Debug, Clone)]
pub enum SimulateCommand {
    TypeText(String),
    TapKey(Key),
    SelectAll,
    SelectAllAndDelete,
    MoveAbsolute {
        x: f64,
        y: f64,
    },
    MoveRelative {
        dx: f64,
        dy: f64,
    },
    Click(Button),
    MoveToAndClick {
        x: f64,
        y: f64,
        button: Button,
    },
    MoveRelativeAndRightClick {
        dx: f64,
        dy: f64,
    },
    PhasedScroll {
        target: Option<(f64, f64)>,
        delta_x: f64,
        delta_y: f64,
    },
    ScrollBackAndForth {
        target: Option<(f64, f64)>,
    },
    Media(MediaKey),
}

impl SimulateCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::TypeText(_) => "Type Text",
            Self::TapKey(_) => "Tap Key",
            Self::SelectAll => "Select All",
            Self::SelectAllAndDelete => "Select All & Delete",
            Self::MoveAbsolute { .. } => "Move Mouse Absolute",
            Self::MoveRelative { .. } => "Move Mouse Relative",
            Self::Click(_) => "Mouse Click",
            Self::MoveToAndClick { .. } => "Move to Target & Click",
            Self::MoveRelativeAndRightClick { .. } => "Move Rel & Right Click",
            Self::PhasedScroll { .. } => "Phased Scroll",
            Self::ScrollBackAndForth { .. } => "Scroll Back & Forth (10x)",
            Self::Media(_) => "Media Key",
        }
    }
}

pub struct ActiveHookSession {
    hook: Arc<Hook>,
    join_handle: Option<JoinHandle<()>>,
}

impl ActiveHookSession {
    pub fn start_listen(tx: SyncSender<WorkerMessage>) -> Self {
        let hook = Arc::new(Hook::new());
        let hook_for_thread = Arc::clone(&hook);
        let join_handle = thread::spawn(move || {
            let tx_events = tx.clone();
            let result = hook_for_thread.run(move |event: &Event| {
                let _ = tx_events.try_send(WorkerMessage::HookEvent {
                    event: event.clone(),
                    blocked: false,
                });
            });
            let _ = tx.send(WorkerMessage::HookEnded { result });
        });
        Self {
            hook,
            join_handle: Some(join_handle),
        }
    }

    pub fn start_grab(block_escape: Arc<AtomicBool>, tx: SyncSender<WorkerMessage>) -> Self {
        let hook = Arc::new(Hook::new());
        let hook_for_thread = Arc::clone(&hook);
        let join_handle = thread::spawn(move || {
            let tx_events = tx.clone();
            let result = hook_for_thread.grab(move |event: &Event| {
                let should_block = block_escape.load(Ordering::Relaxed)
                    && matches!(
                        event.event_type,
                        EventType::KeyPressed | EventType::KeyReleased
                    )
                    && event
                        .keyboard
                        .as_ref()
                        .is_some_and(|kb| kb.key == Key::Escape);

                let _ = tx_events.try_send(WorkerMessage::HookEvent {
                    event: event.clone(),
                    blocked: should_block,
                });

                if should_block {
                    None
                } else {
                    Some(event.clone())
                }
            });
            let _ = tx.send(WorkerMessage::HookEnded { result });
        });
        Self {
            hook,
            join_handle: Some(join_handle),
        }
    }

    pub fn stop(&mut self) -> Result<(), monio::Error> {
        let stop_res = self.hook.stop();
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
        stop_res
    }
}

impl Drop for ActiveHookSession {
    fn drop(&mut self) {
        if self.hook.is_running() {
            let _ = self.hook.stop();
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn create_worker_channel() -> (SyncSender<WorkerMessage>, Receiver<WorkerMessage>) {
    mpsc::sync_channel(512)
}

pub fn spawn_simulate(cmd: SimulateCommand, tx: SyncSender<WorkerMessage>) {
    thread::spawn(move || {
        let action = cmd.label().to_string();
        let result = execute_simulate(cmd);
        let _ = tx.send(WorkerMessage::ActionFinished { action, result });
    });
}

fn execute_simulate(cmd: SimulateCommand) -> Result<(), monio::Error> {
    match cmd {
        SimulateCommand::TypeText(text) => type_text(&text),
        SimulateCommand::TapKey(key) => key_tap(key),
        SimulateCommand::SelectAll => {
            let modifier = select_all_modifier_key();
            key_press(modifier)?;
            thread::sleep(Duration::from_millis(25));
            let tap_res = key_tap(Key::KeyA);
            let rel_res = key_release(modifier);
            tap_res.and(rel_res)
        }
        SimulateCommand::SelectAllAndDelete => {
            let modifier = select_all_modifier_key();
            key_press(modifier)?;
            thread::sleep(Duration::from_millis(25));
            let tap_res = key_tap(Key::KeyA);
            let rel_res = key_release(modifier);
            tap_res.and(rel_res)?;
            thread::sleep(Duration::from_millis(35));
            key_tap(Key::Backspace)
        }
        SimulateCommand::MoveAbsolute { x, y } => mouse_move(x, y),
        SimulateCommand::MoveRelative { dx, dy } => mouse_move_relative(dx, dy),
        SimulateCommand::Click(button) => mouse_click(button),
        SimulateCommand::MoveToAndClick { x, y, button } => {
            mouse_move(x, y)?;
            thread::sleep(Duration::from_millis(30));
            mouse_click(button)
        }
        SimulateCommand::MoveRelativeAndRightClick { dx, dy } => {
            mouse_move_relative(dx, dy)?;
            thread::sleep(Duration::from_millis(30));
            mouse_click(Button::Right)
        }
        SimulateCommand::PhasedScroll {
            target,
            delta_x,
            delta_y,
        } => {
            if let Some((tx, ty)) = target {
                mouse_move(tx, ty)?;
                thread::sleep(Duration::from_millis(30));
            }
            scroll(delta_x, delta_y, ScrollPhase::Began)?;
            thread::sleep(Duration::from_millis(20));
            scroll(delta_x * 2.0, delta_y * 2.0, ScrollPhase::Changed)?;
            thread::sleep(Duration::from_millis(20));
            scroll(0.0, 0.0, ScrollPhase::Ended)
        }
        SimulateCommand::ScrollBackAndForth { target } => {
            if let Some((tx, ty)) = target {
                mouse_move(tx, ty)?;
                thread::sleep(Duration::from_millis(30));
            }
            for i in 1..=10 {
                let step = f64::from(i) * 4.0;
                scroll(0.0, -step, ScrollPhase::Began)?;
                scroll(0.0, 0.0, ScrollPhase::Ended)?;
                thread::sleep(Duration::from_millis(60));
                scroll(0.0, step, ScrollPhase::Began)?;
                scroll(0.0, 0.0, ScrollPhase::Ended)?;
                thread::sleep(Duration::from_millis(60));
            }
            Ok(())
        }
        SimulateCommand::Media(key) => media_key(key),
    }
}

pub fn spawn_display_refresh(tx: SyncSender<WorkerMessage>) {
    thread::spawn(move || {
        let snapshot = collect_display_snapshot();
        let _ = tx.send(WorkerMessage::DisplayRefreshed(snapshot));
    });
}

pub fn collect_display_snapshot() -> DisplaySnapshot {
    let displays_res = displays().map_err(|e| e.to_string());
    let primary_res = primary_display().map_err(|e| e.to_string());
    let mouse_res = mouse_position().map_err(|e| e.to_string());
    let display_at_cursor = mouse_res
        .as_ref()
        .ok()
        .map(|&(x, y)| display_at_point(x, y).map_err(|e| e.to_string()));
    let settings_res = system_settings().map_err(|e| e.to_string());

    DisplaySnapshot {
        displays: displays_res,
        primary: primary_res,
        mouse_position: mouse_res,
        display_at_cursor,
        settings: settings_res,
    }
}

pub fn spawn_probe_relative_capture(tx: SyncSender<WorkerMessage>) {
    thread::spawn(move || {
        let result = match RelativePointerCapture::acquire() {
            Ok(lease) => lease.release(),
            Err(err) => Err(err),
        };
        let _ = tx.send(WorkerMessage::ActionFinished {
            action: String::from("RelativePointerCapture Probe"),
            result,
        });
    });
}

pub fn banner_from_action_result(action: &str, result: &Result<(), monio::Error>) -> StatusBanner {
    match result {
        Ok(()) => StatusBanner::success(
            format!("{action}: Completed"),
            "Operation dispatched through monio on background thread.",
        ),
        Err(err) => StatusBanner::from_monio_error(action, err),
    }
}
