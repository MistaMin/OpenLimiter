//! macOS: a pop-out view in a resizable floating window of its own. Main thread only.

use std::ffi::{c_char, c_void, CString};

use openplugin::dri::cocoa::EditorView;

use super::pops::Make;

type Id = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Size {
    w: f64,
    h: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    size: Size,
}

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> *const c_void;
    fn objc_msgSend();
    #[cfg(target_arch = "x86_64")]
    fn objc_msgSend_stret();
}

/// `send!(Ret, receiver, b"selector:\0", ArgType = value, ...)`
macro_rules! send {
    ($ret:ty, $obj:expr, $sel:expr $(, $t:ty = $a:expr)*) => {{
        let f: unsafe extern "C" fn(Id, *const c_void $(, $t)*) -> $ret = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f($obj, sel_registerName($sel.as_ptr() as *const c_char) $(, $a)*)
    }};
}

/// `[window frame]`: a struct this large comes back through memory on x86_64.
unsafe fn frame_of(window: Id) -> Rect {
    unsafe {
        let sel = sel_registerName(c"frame".as_ptr());
        #[cfg(target_arch = "x86_64")]
        {
            let f: unsafe extern "C" fn(*mut Rect, Id, *const c_void) = std::mem::transmute(objc_msgSend_stret as unsafe extern "C" fn());
            let mut r = Rect { x: 0.0, y: 0.0, size: Size { w: 0.0, h: 0.0 } };
            f(&mut r, window, sel);
            r
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let f: unsafe extern "C" fn(Id, *const c_void) -> Rect = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            f(window, sel)
        }
    }
}

pub struct Window {
    window: Id,
    view: Option<EditorView>,
}

impl Window {
    /// `saved`: the window's frame last time (screen points, origin bottom left).
    pub fn open(make: Make, title: &str, saved: Option<[i32; 4]>) -> Option<Self> {
        let view = EditorView::new(make(), None)?;
        let (w, h) = view.size();
        let min = view.size_range().min;
        unsafe {
            let frame = Rect { x: 0.0, y: 0.0, size: Size { w: f64::from(w), h: f64::from(h) } };
            let alloc = send!(Id, objc_getClass(c"NSWindow".as_ptr()), b"alloc\0");
            // Titled | closable | miniaturizable | resizable; buffered backing.
            let window = send!(Id, alloc, b"initWithContentRect:styleMask:backing:defer:\0", Rect = frame, usize = 15, usize = 2, i8 = 0);
            if window.is_null() {
                return None;
            }
            // Closing only hides it; it is released on drop, after the view is gone.
            send!((), window, b"setReleasedWhenClosed:\0", i8 = 0);
            send!((), window, b"setContentMinSize:\0", Size = Size { w: f64::from(min.0), h: f64::from(min.1) });
            let title = CString::new(title).unwrap_or_default();
            let ns = send!(Id, objc_getClass(c"NSString".as_ptr()), b"stringWithUTF8String:\0", *const c_char = title.as_ptr());
            send!((), window, b"setTitle:\0", Id = ns);
            send!((), window, b"setContentView:\0", Id = view.raw() as Id);
            send!((), window, b"setAcceptsMouseMovedEvents:\0", i8 = 1);
            // Hosts float their plug-in windows; a normal window would open behind them.
            send!((), window, b"setLevel:\0", isize = 3);
            match saved {
                Some([x, y, w, h]) => {
                    let r = Rect { x: f64::from(x), y: f64::from(y), size: Size { w: f64::from(w), h: f64::from(h) } };
                    send!((), window, b"setFrame:display:\0", Rect = r, i8 = 0);
                    // The screen it was on is gone: start in the middle of this one.
                    if send!(Id, window, b"screen\0").is_null() {
                        send!((), window, b"center\0");
                    }
                }
                None => send!((), window, b"center\0"),
            }
            send!((), window, b"makeKeyAndOrderFront:\0", Id = std::ptr::null_mut());
            Some(Self { window, view: Some(view) })
        }
    }

    /// False once the user closed it.
    pub fn is_open(&self) -> bool {
        unsafe { send!(i8, self.window, b"isVisible\0") != 0 || send!(i8, self.window, b"isMiniaturized\0") != 0 }
    }

    pub fn frame(&self) -> Option<[i32; 4]> {
        let r = unsafe { frame_of(self.window) };
        Some([r.x.round() as i32, r.y.round() as i32, r.size.w.round() as i32, r.size.h.round() as i32])
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // The view first: it stops its timer and leaves the window.
        self.view = None;
        unsafe {
            send!((), self.window, b"close\0");
            send!((), self.window, b"release\0");
        }
    }
}
