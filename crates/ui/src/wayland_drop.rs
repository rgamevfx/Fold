//! Native Wayland file offers on Winit's existing display. This owns only its
//! own registry/data devices; Winit remains the event-loop and surface owner.
use std::{
    collections::HashMap,
    io::Read,
    os::fd::AsFd,
    os::unix::net::UnixStream,
    sync::Arc,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    protocol::{wl_data_device, wl_data_device_manager, wl_data_offer, wl_registry, wl_seat},
};
use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
    window::Window,
};
const MIME: &str = "text/uri-list";
pub struct WaylandDrop {
    queue: EventQueue<State>,
    connection: Connection,
    state: State,
    // Keep the display/surface alive until all guest proxies have been dropped.
    _window: Arc<Window>,
    _display: winit::event_loop::OwnedDisplayHandle,
}
struct Offer {
    proxy: wl_data_offer::WlDataOffer,
    uri_list: bool,
    copy: bool,
}
struct Transfer {
    offer: wl_data_offer::WlDataOffer,
    reader: UnixStream,
    bytes: Vec<u8>,
    position: [f32; 2],
    started: Instant,
}
#[derive(Default)]
struct State {
    surface: usize,
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    seats: Vec<(u32, wl_seat::WlSeat, Option<wl_data_device::WlDataDevice>)>,
    offers: HashMap<wayland_client::backend::ObjectId, Offer>,
    active: Option<wl_data_offer::WlDataOffer>,
    position: Option<[f32; 2]>,
    transfer: Option<Transfer>,
    error: Option<String>,
}
pub struct Update {
    pub position: Option<[f32; 2]>,
    pub dropped: Option<([f32; 2], Vec<std::path::PathBuf>)>,
    pub error: Option<String>,
}
impl WaylandDrop {
    #[allow(unsafe_code)] // The sole display-pointer FFI boundary.
    pub fn new(
        window: Arc<Window>,
        owned_display: winit::event_loop::OwnedDisplayHandle,
    ) -> Option<Self> {
        let RawDisplayHandle::Wayland(display) = owned_display.display_handle().ok()?.as_raw()
        else {
            return None;
        };
        let RawWindowHandle::Wayland(surface) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: guest backend never closes this display. The retained Window
        // and OwnedDisplayHandle retain the connection beyond the guest backend;
        // Drop destroys our proxies before releasing the owning connection.
        let backend = unsafe {
            wayland_client::backend::Backend::from_foreign_display(display.display.as_ptr().cast())
        };
        let connection = Connection::from_backend(backend);
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let _ = connection.flush();
        Some(Self {
            queue,
            connection,
            state: State {
                surface: surface.surface.as_ptr() as usize,
                ..Default::default()
            },
            _window: window,
            _display: owned_display,
        })
    }
    pub fn poll(&mut self, scale: f32) -> Update {
        if let Err(e) = self.queue.dispatch_pending(&mut self.state) {
            self.state.error = Some(format!("Wayland drop: {e}"));
        }
        let mut dropped = None;
        let mut complete = false;
        if let Some(t) = &mut self.state.transfer {
            let mut buffer = [0; 8192];
            loop {
                match t.reader.read(&mut buffer) {
                    Ok(0) => {
                        complete = true;
                        break;
                    }
                    Ok(n) => {
                        t.bytes.extend_from_slice(&buffer[..n]);
                        if t.bytes.len() > 1024 * 1024 {
                            self.state.error = Some("File drop exceeds 1 MiB".into());
                            complete = true;
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        self.state.error = Some(e.to_string());
                        complete = true;
                        break;
                    }
                }
            }
            if t.started.elapsed() > Duration::from_secs(15) {
                self.state.error = Some("File drop transfer timed out".into());
                complete = true;
            }
        }
        if complete {
            let t = self.state.transfer.take().unwrap();
            if self.state.error.is_none() {
                match parse_uris(&t.bytes) {
                    Ok(paths) => {
                        dropped = Some((t.position.map(|v| v * scale), paths));
                        if t.offer.version() >= 3 {
                            t.offer.finish();
                        }
                    }
                    Err(e) => self.state.error = Some(e),
                }
            }
            self.state.offers.remove(&t.offer.id());
            t.offer.destroy();
        }
        let _ = self.connection.flush();
        Update {
            position: self.state.position.map(|p| p.map(|v| v * scale)),
            dropped,
            error: self.state.error.take(),
        }
    }
}
impl State {
    fn devices(&mut self, qh: &QueueHandle<Self>) {
        if let Some(manager) = &self.manager {
            for (_, seat, device) in &mut self.seats {
                if device.is_none() {
                    *device = Some(manager.get_data_device(seat, qh, ()));
                }
            }
        }
    }
    fn leave(&mut self) {
        self.position = None;
        if let Some(offer) = self.active.take() {
            self.offers.remove(&offer.id());
            offer.destroy();
        }
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                if interface == "wl_data_device_manager" {
                    s.manager = Some(
                        registry.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
                            name,
                            version.min(3),
                            qh,
                            (),
                        ),
                    );
                }
                if interface == "wl_seat" {
                    s.seats.push((
                        name,
                        registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(5), qh, ()),
                        None,
                    ));
                }
                s.devices(qh);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(i) = s.seats.iter().position(|(n, _, _)| *n == name) {
                    let (_, seat, device) = s.seats.remove(i);
                    if let Some(d) = device {
                        if d.version() >= 2 {
                            d.release();
                        }
                    }
                    if seat.version() >= 5 {
                        seat.release();
                    }
                    s.leave();
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_data_device_manager::WlDataDeviceManager,
        _: wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        s: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(state) = s.offers.get_mut(&offer.id()) {
            match event {
                wl_data_offer::Event::Offer { mime_type } if mime_type == MIME => {
                    state.uri_list = true
                }
                wl_data_offer::Event::Action { dnd_action } => {
                    state.copy = dnd_action
                        == wayland_client::WEnum::Value(wl_data_device_manager::DndAction::Copy)
                }
                _ => {}
            }
        }
    }
}
impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    wayland_client::event_created_child!(State, wl_data_device::WlDataDevice, [0 => (wl_data_offer::WlDataOffer, ())]);
    fn event(
        s: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                s.offers.insert(
                    id.id(),
                    Offer {
                        copy: id.version() < 3,
                        proxy: id,
                        uri_list: false,
                    },
                );
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                s.leave();
                if let Some(offer) = id {
                    let valid = surface.id().as_ptr() as usize == s.surface
                        && s.offers.get(&offer.id()).is_some_and(|o| o.uri_list);
                    offer.accept(serial, valid.then(|| MIME.into()));
                    if valid {
                        if offer.version() >= 3 {
                            offer.set_actions(
                                wl_data_device_manager::DndAction::Copy,
                                wl_data_device_manager::DndAction::Copy,
                            );
                        }
                        s.position = Some([x as f32, y as f32]);
                        s.active = Some(offer);
                    } else {
                        s.offers.remove(&offer.id());
                        offer.destroy();
                    }
                }
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                if s.active.is_some() {
                    s.position = Some([x as f32, y as f32]);
                }
            }
            wl_data_device::Event::Leave => s.leave(),
            wl_data_device::Event::Drop => {
                if let (Some(offer), Some(position)) = (s.active.take(), s.position.take()) {
                    if !s.offers.get(&offer.id()).is_some_and(|o| o.copy) {
                        s.offers.remove(&offer.id());
                        offer.destroy();
                        s.error = Some("The file browser did not offer a copy action".into());
                        return;
                    }
                    if s.transfer.is_some() {
                        s.offers.remove(&offer.id());
                        offer.destroy();
                        s.error = Some("A file drop is already being received".into());
                        return;
                    }
                    match UnixStream::pair().and_then(|(reader, writer)| {
                        reader.set_nonblocking(true)?;
                        Ok((reader, writer))
                    }) {
                        Ok((reader, writer)) => {
                            offer.receive(MIME.into(), writer.as_fd());
                            s.transfer = Some(Transfer {
                                offer,
                                reader,
                                position,
                                bytes: vec![],
                                started: Instant::now(),
                            });
                        }
                        Err(e) => {
                            s.offers.remove(&offer.id());
                            offer.destroy();
                            s.error = Some(e.to_string());
                        }
                    }
                }
            }
            wl_data_device::Event::Selection { id: Some(offer) } => {
                s.offers.remove(&offer.id());
                offer.destroy();
            }
            _ => {}
        }
    }
}
impl Drop for WaylandDrop {
    fn drop(&mut self) {
        for (_, offer) in self.state.offers.drain() {
            offer.proxy.destroy();
        }
        for (_, seat, device) in self.state.seats.drain(..) {
            if let Some(d) = device {
                if d.version() >= 2 {
                    d.release();
                }
            }
            if seat.version() >= 5 {
                seat.release();
            }
        }
        let _ = self.connection.flush();
    }
}
fn parse_uris(bytes: &[u8]) -> Result<Vec<std::path::PathBuf>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Invalid file URI list")?;
    let mut paths = Vec::new();
    for line in text
        .lines()
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let uri = url::Url::parse(line).map_err(|_| "Invalid file URI")?;
        let path = uri
            .to_file_path()
            .map_err(|_| "Only local file drops are supported")?;
        if paths.len() == 4096 {
            return Err("Drop exceeds 4096 entries".into());
        }
        paths.push(path);
    }
    if paths.is_empty() {
        return Err("No files in drop".into());
    }
    Ok(paths)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_uri_lists_decode_spaces_comments_and_unicode() {
        assert_eq!(
            parse_uris(
                b"# files\r\nfile:///tmp/hello%20world.mp4\r\nfile:///tmp/%E2%98%83.exr\r\n"
            )
            .unwrap(),
            vec![
                std::path::PathBuf::from("/tmp/hello world.mp4"),
                "/tmp/☃.exr".into()
            ]
        );
        assert!(parse_uris(b"https://example.com/movie.mp4").is_err());
        assert!(parse_uris(b"file://remote/share/movie.mp4").is_err());
    }
}
