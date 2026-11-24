//! Windows: a pop-out view in a resizable window of its own (the SDK's `TopWindow`). It is
//! owned by the host's main window, so it stays above it, and outlives the plug-in window.
//! UI thread only.

use std::cell::Cell;
use std::ffi::c_void;
use std::rc::Rc;

use openplugin::dri::win32::{EditorView, Hwnd, TopWindow};

use super::pops::Make;

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetForegroundWindow() -> Hwnd;
    fn GetAncestor(hwnd: Hwnd, flags: u32) -> Hwnd;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn MonitorFromRect(rect: *const Rect, flags: u32) -> *mut c_void;
    fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
}

const GA_ROOTOWNER: u32 = 3;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;

pub struct Window {
    // The window first: dropping it destroys the view's child window with it.
    top: TopWindow,
    _view: EditorView,
    closed: Rc<Cell<bool>>,
}

impl Window {
    /// `saved`: the window's rectangle last time, screen pixels.
    pub fn open(make: Make, title: &str, saved: Option<[i32; 4]>) -> Option<Self> {
        let view = EditorView::new(make(), None)?;
        let closed = Rc::new(Cell::new(false));
        let hwnd = Rc::new(Cell::new(std::ptr::null_mut::<c_void>()));
        let on_close = {
            let (closed, hwnd) = (closed.clone(), hwnd.clone());
            // Hide it; the registry drops it (and saves where it was) when it next looks.
            Box::new(move || {
                closed.set(true);
                unsafe { ShowWindow(hwnd.get(), 0) };
            })
        };
        let top = TopWindow::new(title, &view, Some(on_close))?;
        hwnd.set(top.raw());
        unsafe {
            // Owned by the host's main window, not the plug-in window, which may close first.
            let owner = GetAncestor(GetForegroundWindow(), GA_ROOTOWNER);
            if !owner.is_null() {
                top.set_owner(owner);
            }
            if let Some([x, y, w, h]) = saved {
                let r = Rect { left: x, top: y, right: x + w, bottom: y + h };
                // Only onto a monitor that is still there.
                if !MonitorFromRect(&r, 0).is_null() {
                    SetWindowPos(top.raw(), std::ptr::null_mut(), x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
                }
            }
        }
        top.set_visible(true);
        Some(Self { top, _view: view, closed })
    }

    pub fn is_open(&self) -> bool {
        !self.closed.get()
    }

    pub fn frame(&self) -> Option<[i32; 4]> {
        let mut r = Rect::default();
        (unsafe { GetWindowRect(self.top.raw(), &mut r) } != 0).then(|| [r.left, r.top, r.right - r.left, r.bottom - r.top])
    }
}
