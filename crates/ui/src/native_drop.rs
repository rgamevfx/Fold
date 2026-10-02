//! Winit 0.30 supplies file drops only on X11 on Linux, and omits XDND pointer
//! positions. Query the actual window-relative pointer during external drags;
//! never use the stale last ordinary mouse event to choose an import bin.
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
use x11rb::{protocol::xproto::ConnectionExt, rust_connection::RustConnection};
pub struct DropPointer {
    connection: RustConnection,
    window: u32,
}
impl DropPointer {
    pub fn new(window: &Window) -> Option<Self> {
        let id = match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok()?,
            RawWindowHandle::Xcb(handle) => handle.window.get(),
            _ => return None,
        };
        let (connection, _) = x11rb::connect(None).ok()?;
        Some(Self {
            connection,
            window: id,
        })
    }
    pub fn position(&self, scale: f64) -> Option<[f32; 2]> {
        let reply = self
            .connection
            .query_pointer(self.window)
            .ok()?
            .reply()
            .ok()?;
        reply.same_screen.then_some([
            f32::from(reply.win_x) / scale as f32,
            f32::from(reply.win_y) / scale as f32,
        ])
    }
}
