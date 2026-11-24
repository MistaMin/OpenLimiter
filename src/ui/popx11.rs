//! Linux: a pop-out view in a top-level X11 window, run on a thread of its own with its own
//! X connection. Nothing else pumps it, so it keeps running after the plug-in window closes;
//! dropping it stops the thread and waits for it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use openplugin::dri::x11::EditorView;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, WindowClass};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

use super::pops::Make;

pub struct Window {
    quit: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    frame: Arc<Mutex<Option<[i32; 4]>>>,
    thread: Option<JoinHandle<()>>,
}

impl Window {
    /// `saved`: the window's place and size last time, root-window pixels.
    pub fn open(make: Make, title: &str, saved: Option<[i32; 4]>) -> Option<Self> {
        let quit = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let frame = Arc::new(Mutex::new(saved));
        let title = title.to_string();
        let thread = std::thread::Builder::new()
            .name("openlimiter-pop".into())
            .spawn({
                let (quit, done, frame) = (quit.clone(), done.clone(), frame.clone());
                move || {
                    let _ = run(make, &title, saved, &quit, &frame);
                    done.store(true, Ordering::Release);
                }
            })
            .ok()?;
        Some(Self { quit, done, frame, thread: Some(thread) })
    }

    pub fn is_open(&self) -> bool {
        !self.done.load(Ordering::Acquire)
    }

    pub fn frame(&self) -> Option<[i32; 4]> {
        *self.frame.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(make: Make, title: &str, saved: Option<[i32; 4]>, quit: &AtomicBool, frame: &Mutex<Option<[i32; 4]>>) -> Option<()> {
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    let mut view = EditorView::new(make(), None)?;
    let scale = view.scale().max(1.0);
    let px = |p: u32| (p as f32 * scale).round() as u32;
    let points = |p: u32| (p as f32 / scale).round() as u32;
    let (vw, vh) = view.size();
    let [x, y, w, h] = saved.unwrap_or([0, 0, px(vw) as i32, px(vh) as i32]);
    let mut size = (w.max(1) as u32, h.max(1) as u32);
    let top = conn.generate_id().ok()?;
    let aux = CreateWindowAux::new().event_mask(EventMask::STRUCTURE_NOTIFY);
    conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, top, root, x as i16, y as i16, size.0 as u16, size.1 as u16, 0, WindowClass::INPUT_OUTPUT, x11rb::COPY_FROM_PARENT, &aux).ok()?;
    let atom = |name: &[u8]| Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom);
    let (protocols, delete, net_name, utf8) = (atom(b"WM_PROTOCOLS")?, atom(b"WM_DELETE_WINDOW")?, atom(b"_NET_WM_NAME")?, atom(b"UTF8_STRING")?);
    let latin: Vec<u8> = title.chars().map(|c| if (c as u32) < 256 { c as u32 as u8 } else { b'?' }).collect();
    conn.change_property8(PropMode::REPLACE, top, AtomEnum::WM_NAME, AtomEnum::STRING, &latin).ok()?;
    conn.change_property8(PropMode::REPLACE, top, net_name, utf8, title.as_bytes()).ok()?;
    conn.change_property32(PropMode::REPLACE, top, protocols, AtomEnum::ATOM, &[delete]).ok()?;
    // WM_NORMAL_HINTS: the minimum size, and USPosition | USSize when it goes back where it was.
    let min = view.size_range().min;
    let mut hints = [0u32; 18];
    hints[0] = 16 | if saved.is_some() { 1 | 2 } else { 0 };
    (hints[1], hints[2], hints[3], hints[4]) = (x as u32, y as u32, size.0, size.1);
    (hints[5], hints[6]) = (px(min.0), px(min.1));
    conn.change_property32(PropMode::REPLACE, top, AtomEnum::WM_NORMAL_HINTS, AtomEnum::WM_SIZE_HINTS, &hints).ok()?;
    conn.map_window(top).ok()?;
    conn.flush().ok()?;
    if view.attach(top) {
        view.resize(points(size.0), points(size.1));
        let mut closed = false;
        while !closed && !quit.load(Ordering::Acquire) {
            view.pump();
            loop {
                // A lost connection to the X server ends the window.
                let event = match conn.poll_for_event() {
                    Ok(Some(event)) => event,
                    Ok(None) => break,
                    Err(_) => {
                        closed = true;
                        break;
                    }
                };
                match event {
                    Event::ConfigureNotify(e) if e.window == top => {
                        if (u32::from(e.width), u32::from(e.height)) != size {
                            size = (u32::from(e.width), u32::from(e.height));
                            view.resize(points(size.0), points(size.1));
                        }
                        // Where it is on the screen, not inside the window manager's frame.
                        if let Some(r) = conn.translate_coordinates(top, root, 0, 0).ok().and_then(|c| c.reply().ok()) {
                            *frame.lock().unwrap_or_else(|e| e.into_inner()) = Some([i32::from(r.dst_x), i32::from(r.dst_y), size.0 as i32, size.1 as i32]);
                        }
                    }
                    Event::ClientMessage(e) if e.format == 32 && e.data.as_data32()[0] == delete => closed = true,
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(8));
        }
    }
    drop(view);
    let _ = conn.destroy_window(top);
    let _ = conn.flush();
    Some(())
}
