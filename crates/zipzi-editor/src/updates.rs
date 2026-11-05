//! One process-wide updater. Background work never owns a window or a Runtime session.
mod source;
pub(crate) mod ui;
#[cfg(windows)]
mod windows;

use crate::{EditorView, host, settings, t};
use gpui_kit::component::WindowExt;
use gpui_kit::{prelude::*, *};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager};
use zipzi_editor_core::ConfigStore;

pub(crate) const DOWNLOADS: &str = "https://github.com/getzipzi/zipzi-2d/releases";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Unavailable,
    Idle,
    Checking,
    Current,
    Available,
    Downloading(i16),
    Ready,
    Applying,
    Failed,
}
impl Phase {
    pub(crate) fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Downloading(_) | Self::Applying)
    }
    pub(crate) fn indicator_visible(&self) -> bool {
        *self == Self::Ready
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct Preference {
    automatic: bool,
}
impl Default for Preference {
    fn default() -> Self {
        Self { automatic: true }
    }
}

pub(crate) struct Updater {
    pub(crate) phase: Phase,
    pub(crate) automatic: bool,
    pub(crate) detail: Option<String>,
    pub(crate) release: Option<UpdateInfo>,
    manager: Option<UpdateManager>,
    store: ConfigStore,
    events: async_channel::Sender<Event>,
}

struct Service(Entity<Updater>);
impl Global for Service {}

enum Event {
    Checked(Result<Option<UpdateInfo>, String>),
    Progress(i16),
    Downloaded(Result<(), String>),
}

/// Run before launch parsing and before claiming the resident Runtime endpoint.
pub(crate) fn bootstrap() {
    #[cfg(windows)]
    match std::env::args().nth(1).as_deref() {
        Some("--veloapp-uninstall") => {
            if let Err(error) = windows::register(true) {
                eprintln!("Shell unregistration: {error}");
            }
        }
        Some("--veloapp-install" | "--veloapp-updated") => {
            if let Err(error) = windows::register(false) {
                eprintln!("Shell registration: {error}");
            }
        }
        _ => {}
    }
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();
}

pub(crate) fn init(cx: &mut App) -> Entity<Updater> {
    if let Some(service) = cx.try_global::<Service>() {
        return service.0.clone();
    }
    #[cfg(windows)]
    if let Err(error) = windows::register(false) {
        eprintln!("Shell registration: {error}");
    }
    let store = cx.global::<settings::Settings>().store.clone();
    let preference = store
        .read::<Preference>("updates.json")
        .ok()
        .flatten()
        .unwrap_or_default();
    let (events, incoming) = async_channel::unbounded();
    // These are release-build inputs, never inherited from a project or a runtime env var.
    let manager = match (
        option_env!("ZIPZI_UPDATE_URL"),
        option_env!("ZIPZI_UPDATE_PUBLIC_KEY"),
    ) {
        (Some(url), Some(key)) => source::SignedSource::new(url, key)
            .and_then(|source| UpdateManager::new(source, None, None))
            .ok(),
        _ => None,
    };
    let available = manager.is_some();
    let entity = cx.new(|_| Updater {
        phase: if available {
            Phase::Idle
        } else {
            Phase::Unavailable
        },
        automatic: preference.automatic,
        detail: None,
        release: None,
        manager,
        store,
        events,
    });
    cx.set_global(Service(entity.clone()));
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(event) = incoming.recv().await {
            if weak
                .update(cx, |updater, cx| updater.event(event, cx))
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
    if available {
        let weak = entity.downgrade();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs(30))
                .await;
            loop {
                if weak
                    .update(cx, |updater, cx| {
                        if updater.automatic {
                            updater.check(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_secs(4 * 60 * 60))
                    .await;
            }
        })
        .detach();
    }
    entity
}

pub(crate) fn ready(cx: &App) -> bool {
    cx.try_global::<Service>()
        .is_some_and(|service| service.0.read(cx).phase.indicator_visible())
}

pub(crate) fn applying(cx: &App) -> bool {
    cx.try_global::<Service>()
        .is_some_and(|service| service.0.read(cx).phase == Phase::Applying)
}

impl Updater {
    pub(crate) fn check(&mut self, cx: &mut Context<Self>) {
        if self.phase.busy() || self.phase == Phase::Ready {
            return;
        }
        let Some(manager) = self.manager.clone() else {
            return;
        };
        self.phase = Phase::Checking;
        self.detail = None;
        cx.notify();
        let events = self.events.clone();
        std::thread::spawn(move || {
            let result = manager
                .check_for_updates()
                .map(|result| match result {
                    UpdateCheck::UpdateAvailable(info) => Some(*info),
                    _ => None,
                })
                .map_err(|error| error.to_string());
            let _ = events.send_blocking(Event::Checked(result));
        });
    }

    pub(crate) fn download(&mut self, cx: &mut Context<Self>) {
        if self.phase.busy() || self.phase == Phase::Ready {
            return;
        }
        let (Some(manager), Some(release)) = (self.manager.clone(), self.release.clone()) else {
            return;
        };
        self.phase = Phase::Downloading(0);
        self.detail = None;
        cx.notify();
        let events = self.events.clone();
        let (progress, incoming) = std::sync::mpsc::channel();
        let progress_events = events.clone();
        std::thread::spawn(move || {
            while let Ok(progress) = incoming.recv() {
                if progress_events
                    .send_blocking(Event::Progress(progress))
                    .is_err()
                {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let result = velopack::locator::auto_locate_app_manifest(
                velopack::locator::LocationContext::FromCurrentExe,
            )
            .and_then(|locator| {
                source::download_verified(&manager, &release, &locator, Some(progress))
            })
            .map_err(|error| error.to_string());
            let _ = events.send_blocking(Event::Downloaded(result));
        });
    }

    fn event(&mut self, event: Event, cx: &mut Context<Self>) {
        let was_ready = self.phase.indicator_visible();
        match event {
            Event::Checked(Ok(release)) => {
                self.phase = if release.is_some() {
                    Phase::Available
                } else {
                    Phase::Current
                };
                self.release = release;
                if self.automatic && self.phase == Phase::Available {
                    self.download(cx);
                }
            }
            Event::Downloaded(Ok(())) => self.phase = Phase::Ready,
            Event::Checked(Err(error)) | Event::Downloaded(Err(error)) => {
                self.detail = Some(error);
                self.phase = Phase::Failed;
            }
            Event::Progress(progress) => {
                if matches!(self.phase, Phase::Downloading(_)) {
                    self.phase = Phase::Downloading(progress.clamp(0, 100));
                }
            }
        }
        cx.notify();
        // Download progress is confined to the panel; only readiness changes window chrome.
        if was_ready != self.phase.indicator_visible() {
            cx.refresh_windows();
        }
    }

    pub(crate) fn set_automatic(&mut self, automatic: bool, cx: &mut Context<Self>) {
        if let Err(error) = self.store.write("updates.json", &Preference { automatic }) {
            self.detail = Some(error.to_string());
        } else {
            self.automatic = automatic;
            if automatic {
                if self.phase == Phase::Available {
                    self.download(cx);
                } else if matches!(self.phase, Phase::Idle | Phase::Current | Phase::Failed) {
                    self.check(cx);
                }
            }
        }
        cx.notify();
    }
}

/// Flush fields and inspect every visible/parked session before arming the updater.
/// The updater's 60-second process deadline must never include a save prompt or SDK job.
pub(crate) fn prepare_restart(cx: &mut App) -> Result<Vec<std::ffi::OsString>, String> {
    if let Some(host) = cx.try_global::<host::DesktopHost>() {
        if host.protocol.borrow().has_connected_clients() {
            return Err(t!("updates.blocked_clients").into_owned());
        }
        if !host.runtime.read().pending_jobs().is_empty() {
            return Err(t!("updates.blocked_jobs").into_owned());
        }
    }
    let mut paths = Vec::new();
    for handle in cx.windows() {
        handle
            .update(cx, |root, window, cx| -> Result<(), String> {
                let view = root
                    .downcast::<gpui_kit::base::Root>()
                    .map_err(|_| t!("updates.blocked_operation").into_owned())?
                    .read(cx)
                    .view()
                    .clone()
                    .downcast::<EditorView>()
                    .map_err(|_| t!("updates.blocked_operation").into_owned())?;
                view.update(cx, |view, cx| {
                    if view.modal
                        || window.has_active_dialog(cx)
                        || view.editor.gesture_active()
                        || view.editor.is_busy()
                        || view.editor.pending_io_count() != 0
                    {
                        return Err(t!("updates.blocked_operation").into_owned());
                    }
                    if let Some(id) = view.editor.active_id() {
                        if !view.prepare_property_inputs(id, window, cx) {
                            return Err(t!("updates.blocked_operation").into_owned());
                        }
                    }
                    paths.extend(
                        view.editor
                            .documents()
                            .into_iter()
                            .filter_map(|doc| doc.path),
                    );
                    if view.editor.documents().iter().any(|doc| doc.modified) {
                        return Err(t!("updates.blocked_documents").into_owned());
                    }
                    Ok(())
                })
            })
            .map_err(|error| error.to_string())??;
    }
    if let Some(host) = cx.try_global::<host::DesktopHost>() {
        let runtime = host.runtime.read();
        let sessions = runtime.list_sessions();
        if sessions
            .iter()
            .any(|session| session.modified || session.state != zipzi_runtime::SessionState::Ready)
        {
            return Err(t!("updates.blocked_documents").into_owned());
        }
        paths.extend(sessions.into_iter().filter_map(|session| session.path));
    }
    paths.sort();
    paths.dedup();
    let mut args = vec![std::ffi::OsString::from("--")];
    args.extend(paths.into_iter().map(|path| path.into_os_string()));
    Ok(args)
}

pub(crate) fn restart(window: &mut Window, cx: &mut App) {
    window.close_dialog(cx);
    let handle = window.window_handle();
    // Release the clicked window's update lease before examining every window.
    cx.defer(move |cx| {
        let entity = init(cx);
        let result = prepare_restart(cx).and_then(|args| {
            entity.update(cx, |updater, cx| {
                if updater.phase != Phase::Ready {
                    return Err(t!("updates.not_ready").into_owned());
                }
                let manager = updater
                    .manager
                    .as_ref()
                    .ok_or_else(|| t!("updates.unavailable").into_owned())?;
                let release = updater
                    .release
                    .as_ref()
                    .ok_or_else(|| t!("updates.not_ready").into_owned())?;
                if let Err(error) = verify_pending(release) {
                    updater.phase = Phase::Failed;
                    return Err(error);
                }
                manager
                    .wait_exit_then_apply_updates(release, false, true, args)
                    .map_err(|error| error.to_string())?;
                updater.phase = Phase::Applying;
                cx.notify();
                Ok(())
            })
        });
        match result {
            Ok(()) => cx.quit(),
            Err(error) => {
                entity.update(cx, |updater, cx| {
                    updater.detail = Some(error);
                    cx.notify();
                });
                cx.refresh_windows();
                let _ = handle.update(cx, |_, window, cx| ui::show(window, cx, false));
            }
        }
    });
}

fn verify_pending(release: &UpdateInfo) -> Result<(), String> {
    let locator = velopack::locator::auto_locate_app_manifest(
        velopack::locator::LocationContext::FromCurrentExe,
    )
    .map_err(|error| error.to_string())?;
    let asset = &release.TargetFullRelease;
    source::verify_package(&locator.get_packages_dir().join(&asset.FileName), asset)
        .map_err(|error| error.to_string())
}

pub(crate) fn on_quit(cx: &mut App) {
    let Some(service) = cx.try_global::<Service>() else {
        return;
    };
    let entity = service.0.clone();
    let updater = entity.read(cx);
    if !updater.automatic || updater.phase != Phase::Ready {
        return;
    }
    if prepare_restart(cx).is_err() {
        return;
    }
    entity.update(cx, |updater, cx| {
        if let (Some(manager), Some(release)) = (&updater.manager, &updater.release) {
            if let Err(error) = verify_pending(release) {
                eprintln!("Update deferred: {error}");
                return;
            }
            // The process is already quitting. No saving or other work follows this call.
            match manager.wait_exit_then_apply_updates(release, false, false, Vec::<String>::new())
            {
                Ok(()) => updater.phase = Phase::Applying,
                Err(error) => eprintln!("Update deferred: {error}"),
            }
            cx.notify();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::Phase;
    #[test]
    fn titlebar_only_indicates_a_verified_download_ready_to_install() {
        for phase in [
            Phase::Unavailable,
            Phase::Idle,
            Phase::Checking,
            Phase::Current,
            Phase::Available,
            Phase::Downloading(42),
            Phase::Failed,
            Phase::Applying,
        ] {
            assert!(!phase.indicator_visible(), "{phase:?}");
        }
        assert!(Phase::Ready.indicator_visible());
    }
}
