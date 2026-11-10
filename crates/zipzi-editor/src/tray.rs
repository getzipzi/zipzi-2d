//! Windows notification-area controls for the windowless Editor host.
use crate::{actions::RunCommand, host, host::BackgroundStatus, t};
use gpui_kit::{App, Global};
use std::{cell::RefCell, rc::Rc};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{Error, HSTRING, PCWSTR, Result, w},
};
use zipzi_editor_core::CommandId;

const CLASS: PCWSTR = w!("ZipziEditorTray");
const CALLBACK: u32 = WM_APP + 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;

enum Event {
    Open,
    Menu(POINT),
    TaskbarCreated,
}

struct WindowState {
    events: async_channel::Sender<Event>,
    taskbar_created: u32,
}

struct Tray {
    hwnd: HWND,
    icon: HICON,
    tooltip: RefCell<Option<String>>,
    _window_state: Box<WindowState>,
}

struct TrayGlobal(Rc<Tray>);
impl Global for TrayGlobal {}

pub(super) fn start(cx: &mut App) {
    let (send, receive) = async_channel::unbounded();
    let tray = match Tray::new(send) {
        Ok(tray) => Rc::new(tray),
        Err(error) => {
            eprintln!("Could not initialize the Editor notification-area icon: {error}");
            return;
        }
    };
    cx.set_global(TrayGlobal(tray.clone()));
    cx.on_window_closed(|cx, _| sync(cx)).detach();
    cx.observe_global::<BackgroundStatus>(|cx| sync(cx))
        .detach();
    sync(cx);

    let tray = Rc::downgrade(&tray);
    cx.spawn(async move |cx| {
        while let Ok(event) = receive.recv().await {
            let Some(tray) = tray.upgrade() else { break };
            // Native popup menus and confirmation dialogs run nested message loops.
            // Keep them outside the App borrow so Runtime and window events can run.
            let action = match event {
                Event::TaskbarCreated => {
                    tray.tooltip.take();
                    cx.update(|cx| sync(cx));
                    continue;
                }
                _ if tray.tooltip.borrow().is_none() => continue,
                Event::Open => Some(MenuAction::Open),
                Event::Menu(point) => tray.menu(point),
            };
            match action {
                Some(MenuAction::Open) => cx.update(|cx| {
                    host::open_window(cx, true);
                }),
                Some(MenuAction::Quit) => cx.update(|cx| {
                    let action = RunCommand::new(CommandId::Quit);
                    if let Some(handle) = cx.windows().first().copied() {
                        let _ = handle.update(cx, |_, window, cx| {
                            window.dispatch_action(Box::new(action), cx);
                        });
                    } else {
                        cx.dispatch_action(&action);
                    }
                }),
                Some(MenuAction::ForceQuit) if tray.confirm_force_quit() => {
                    tray.hide();
                    // A graceful shutdown can join Runtime workers. Force Quit must
                    // also stop a host whose job cannot finish.
                    std::process::exit(1);
                }
                _ => {}
            }
        }
    })
    .detach();
}

pub(super) fn sync(cx: &App) {
    let Some(TrayGlobal(tray)) = cx.try_global::<TrayGlobal>() else {
        return;
    };
    if !cx.windows().is_empty() {
        tray.hide();
        return;
    }
    let status = *cx.global::<BackgroundStatus>();
    let reason = match status {
        BackgroundStatus::Resident => BackgroundStatus::Idle,
        status => status,
    };
    tray.show(&t!(
        "background.tray_hint",
        app = t!("app.title"),
        reason = t!(reason.label())
    ));
}

pub(super) fn hide(cx: &App) {
    if let Some(TrayGlobal(tray)) = cx.try_global::<TrayGlobal>() {
        tray.hide();
    }
}

#[repr(usize)]
enum MenuAction {
    Open = 1,
    Quit,
    ForceQuit,
}

impl Tray {
    fn new(events: async_channel::Sender<Event>) -> Result<Self> {
        // SAFETY: All operations on the tray HWND run on the GPUI event thread.
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(None)?.0);
            // Resource 1 is the application icon embedded by editor.rc.
            let icon = LoadIconW(Some(instance), PCWSTR(1 as _))?;
            let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
            if taskbar_created == 0 {
                return Err(Error::from_thread());
            }
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: CLASS,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(Error::from_thread());
            }
            // A hidden top-level window receives TaskbarCreated broadcasts.
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                CLASS,
                CLASS,
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )?;
            let state = Box::new(WindowState {
                events,
                taskbar_created,
            });
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                std::ptr::from_ref(state.as_ref()) as isize,
            );
            Ok(Self {
                hwnd,
                icon,
                tooltip: RefCell::default(),
                _window_state: state,
            })
        }
    }

    fn icon_data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            ..Default::default()
        }
    }

    fn show(&self, tooltip: &str) {
        let mut previous = self.tooltip.borrow_mut();
        if previous.as_deref() == Some(tooltip) {
            return;
        }
        let mut data = self.icon_data();
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = CALLBACK;
        data.hIcon = self.icon;
        for (slot, unit) in data.szTip[..127].iter_mut().zip(tooltip.encode_utf16()) {
            *slot = unit;
        }
        // SAFETY: The HWND and icon remain valid while this instance is alive.
        unsafe {
            if previous.is_some() && Shell_NotifyIconW(NIM_MODIFY, &data).as_bool() {
                *previous = Some(tooltip.into());
            } else if Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
                data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                if Shell_NotifyIconW(NIM_SETVERSION, &data).as_bool() {
                    *previous = Some(tooltip.into());
                } else {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                    *previous = None;
                    eprintln!("Could not configure the Editor notification-area icon");
                }
            } else {
                *previous = None;
                eprintln!("Could not show the Editor notification-area icon");
            }
        }
    }

    fn hide(&self) {
        if self.tooltip.take().is_some() {
            // SAFETY: The HWND remains valid while this instance is alive.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &self.icon_data());
            }
        }
    }

    fn menu(&self, mut point: POINT) -> Option<MenuAction> {
        // SAFETY: The menu is used synchronously and destroyed before returning.
        unsafe {
            if point.x == -1 && point.y == -1 {
                GetCursorPos(&mut point).ok()?;
            }
            let menu = CreatePopupMenu().ok()?;
            let result = (|| {
                for (action, key) in [
                    (MenuAction::Open, "background.open"),
                    (MenuAction::Quit, "app.quit"),
                    (MenuAction::ForceQuit, "background.force_quit"),
                ] {
                    AppendMenuW(
                        menu,
                        MF_STRING,
                        action as usize,
                        &HSTRING::from(t!(key).as_ref()),
                    )
                    .ok()?;
                }
                let _ = SetForegroundWindow(self.hwnd);
                let selected = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    None,
                    self.hwnd,
                    None,
                );
                let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                if selected.0 == 0 {
                    let _ = Shell_NotifyIconW(NIM_SETFOCUS, &self.icon_data());
                }
                match selected.0 {
                    1 => Some(MenuAction::Open),
                    2 => Some(MenuAction::Quit),
                    3 => Some(MenuAction::ForceQuit),
                    _ => None,
                }
            })();
            let _ = DestroyMenu(menu);
            result
        }
    }

    fn confirm_force_quit(&self) -> bool {
        // SAFETY: The string buffers live through the synchronous call.
        unsafe {
            MessageBoxW(
                Some(self.hwnd),
                &HSTRING::from(t!("background.force_quit_confirm").as_ref()),
                &HSTRING::from(t!("background.force_quit").as_ref()),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_SETFOREGROUND,
            ) == IDYES
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.hide();
        // SAFETY: Clear the pointer before destroying its window and freeing the state.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: The owning Tray keeps the pointed-to state alive until it clears
    // GWLP_USERDATA. The window procedure and Tray run on the same thread.
    unsafe {
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
        if let Some(state) = state.as_ref() {
            let event = match message {
                CALLBACK => match u32::from(lparam.0 as u16) {
                    NIN_SELECT | NIN_KEYSELECT => Some(Event::Open),
                    WM_CONTEXTMENU => Some(Event::Menu(POINT {
                        x: i32::from(wparam.0 as i16),
                        y: i32::from((wparam.0 >> 16) as i16),
                    })),
                    _ => None,
                },
                message if message == state.taskbar_created => Some(Event::TaskbarCreated),
                _ => return DefWindowProcW(hwnd, message, wparam, lparam),
            };
            if let Some(event) = event {
                let _ = state.events.try_send(event);
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}
