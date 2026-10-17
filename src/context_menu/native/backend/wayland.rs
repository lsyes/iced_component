//! A native Wayland popup (`xdg_popup`) drawing a context menu outside of the
//! application window.
//!
//! The popup reuses the Wayland connection owned by `winit`: the display, the
//! parent `wl_surface` and the parent `xdg_surface` come from the window's raw
//! handles plus the accessor added by our `winit` patch.

#![cfg(target_os = "linux")]

use std::{
    ffi::c_void,
    io,
    os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
    sync::{LazyLock, Mutex},
};

use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
    backend::{Backend, ObjectId, WaylandError},
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_wm_base,
};

use super::{
    Direction, PopupEvent,
    render::{self, MenuStyle},
};

/// State tracked while dispatching Wayland events.
#[derive(Default)]
pub struct State {
    configured: bool,
    done: bool,
    pointer: Option<(f64, f64)>,
    press: Option<(f64, f64)>,
    /// The surface of our own popup, if it has been created yet.
    own_surface: Option<ObjectId>,
    /// Whether the pointer is currently focused on our popup.
    pointer_focus: bool,
}

wayland_client::delegate_noop!(State: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(State: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(State: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(State: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: ignore xdg_wm_base::XdgWmBase);
wayland_client::delegate_noop!(State: ignore xdg_positioner::XdgPositioner);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            proxy.ack_configure(serial);
            state.configured = true;
        }
    }
}

impl Dispatch<xdg_popup::XdgPopup, ()> for State {
    fn event(
        state: &mut Self,
        _: &xdg_popup::XdgPopup,
        event: xdg_popup::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_popup::Event::PopupDone = event {
            state.done = true;
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // The seat is shared with the application window, so only react to
            // events that belong to the popup surface itself.
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_focus =
                    state.own_surface.as_ref() == Some(&surface.id());
                state.pointer =
                    state.pointer_focus.then_some((surface_x, surface_y));
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                if state.pointer_focus {
                    state.pointer = Some((surface_x, surface_y));
                }
            }
            wl_pointer::Event::Leave { .. } => {
                state.pointer_focus = false;
                state.pointer = None;
            }
            wl_pointer::Event::Button {
                button: 0x110,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } if state.pointer_focus => {
                state.press = state.pointer;
            }
            _ => {}
        }
    }
}

/// A live `xdg_popup` drawing a menu.
pub struct Popup {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    popup: xdg_popup::XdgPopup,
    shm: wl_shm::WlShm,
    qh: QueueHandle<State>,
    pool: Option<wl_shm_pool::WlShmPool>,
    buffer: Option<wl_buffer::WlBuffer>,
    fd: Option<OwnedFd>,
    width: i32,
    height: i32,
    stride: i32,
    labels: Vec<String>,
    enabled: Vec<bool>,
    style: MenuStyle,
    hovered: Option<usize>,
}

impl Popup {
    /// Creates and maps a popup anchored at `(x, y)` inside the parent surface.
    ///
    /// # Safety
    ///
    /// `display` and `parent_surface` must be the raw `wl_display` and
    /// `wl_surface` pointers of a live window created by the patched `winit`.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn create(
        display: *mut c_void,
        parent_surface: *mut c_void,
        x: i32,
        y: i32,
        direction: Direction,
        labels: Vec<String>,
        enabled: Vec<bool>,
        style: MenuStyle,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let width = super::width().round() as i32;
        let height = (style.padding * 2.0
            + style.row_height * labels.len() as f32)
            .round() as i32;

        let connection = unsafe {
            Connection::from_backend(Backend::from_foreign_display(
                display.cast(),
            ))
        };

        let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
        let qh = queue.handle();

        let compositor: wl_compositor::WlCompositor =
            globals.bind(&qh, 4..=6, ())?;
        let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ())?;
        let wm_base: xdg_wm_base::XdgWmBase = globals.bind(&qh, 1..=6, ())?;

        // Bind a pointer so that we receive input for the popup surface.
        if let Ok(seat) = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=7, ())
        {
            let _pointer = seat.get_pointer(&qh, ());
        }

        // `raw-window-handle` only exposes the `wl_surface`, so ask the patched
        // winit for the `xdg_surface` that was created for it.
        let parent_xdg_ptr =
            winit::platform::wayland::xdg_surface_ptr(parent_surface);

        if parent_xdg_ptr.is_null() {
            return Err("the window has no xdg_surface".into());
        }

        let parent_xdg_id = unsafe {
            ObjectId::from_ptr(
                xdg_surface::XdgSurface::interface(),
                parent_xdg_ptr.cast(),
            )?
        };
        let parent_xdg = std::mem::ManuallyDrop::new(
            xdg_surface::XdgSurface::from_id(&connection, parent_xdg_id)?,
        );

        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());

        let positioner = wm_base.create_positioner(&qh, ());
        positioner.set_size(width, height);
        positioner.set_anchor_rect(x, y, 1, 1);
        // The menu hangs off the click: downwards normally, upwards when it
        // would not fit below. The compositor still flips or slides it when it
        // would run past the edge of the screen.
        let (anchor, gravity) = match direction {
            Direction::Down => (
                xdg_positioner::Anchor::TopLeft,
                xdg_positioner::Gravity::BottomRight,
            ),
            Direction::Up => (
                xdg_positioner::Anchor::BottomLeft,
                xdg_positioner::Gravity::TopRight,
            ),
        };
        positioner.set_anchor(anchor);
        positioner.set_gravity(gravity);
        positioner.set_constraint_adjustment(
            xdg_positioner::ConstraintAdjustment::SlideX
                | xdg_positioner::ConstraintAdjustment::SlideY
                | xdg_positioner::ConstraintAdjustment::FlipX
                | xdg_positioner::ConstraintAdjustment::FlipY,
        );

        if positioner.version() >= 3 {
            // Re-position the menu whenever the parent surface (or the screen
            // layout) changes instead of leaving it stranded.
            positioner.set_reactive();
        }

        let popup =
            xdg_surface.get_popup(Some(&parent_xdg), &positioner, &qh, ());
        positioner.destroy();

        surface.commit();

        let mut state = State {
            own_surface: Some(surface.id()),
            ..State::default()
        };

        while !state.configured {
            queue.blocking_dispatch(&mut state)?;
        }

        let mut popup = Self {
            connection,
            queue,
            state,
            surface,
            xdg_surface,
            popup,
            shm,
            qh,
            pool: None,
            buffer: None,
            fd: None,
            width,
            height,
            stride: width * 4,
            labels,
            enabled,
            style,
            hovered: None,
        };

        popup.refresh();

        Ok(popup)
    }

    /// Re-renders the menu into a fresh shared-memory buffer.
    ///
    /// A new buffer is allocated on every frame because the compositor may
    /// still be reading the previous one; mutating it in place would tear.
    fn refresh(&mut self) {
        let pixels = render::render(
            self.width as u32,
            self.height as u32,
            &self.labels,
            &self.enabled,
            self.hovered,
            &self.style,
        );

        let size = (self.stride * self.height) as usize;

        let Ok(fd) = shm_fd(size) else {
            return;
        };

        unsafe {
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_fd().as_raw_fd(),
                0,
            );

            if ptr == libc::MAP_FAILED {
                return;
            }

            let target = ptr as *mut u32;

            for (index, pixel) in pixels.iter().enumerate() {
                target.add(index).write(*pixel);
            }

            libc::munmap(ptr, size);
        }

        let pool = self.shm.create_pool(fd.as_fd(), size as i32, &self.qh, ());
        let buffer = pool.create_buffer(
            0,
            self.width,
            self.height,
            self.stride,
            wl_shm::Format::Argb8888,
            &self.qh,
            (),
        );

        self.surface.attach(Some(&buffer), 0, 0);
        self.surface.damage_buffer(0, 0, self.width, self.height);
        self.surface.commit();
        let _ = self.connection.flush();

        // Release the previous buffer; the compositor keeps its own reference
        // until it is done with it.
        if let Some(previous) = self.buffer.replace(buffer) {
            previous.destroy();
        }

        if let Some(previous) = self.pool.replace(pool) {
            previous.destroy();
        }

        self.fd = Some(fd);
    }

    fn index_at(&self, x: f64, y: f64) -> Option<usize> {
        let padding = f64::from(self.style.padding);

        if x < padding || y < padding {
            return None;
        }

        let index = ((y - padding) / f64::from(self.style.row_height)) as usize;

        (index < self.labels.len()).then_some(index)
    }

    /// Whether the entry at the given index can be chosen.
    fn is_enabled(&self, index: usize) -> bool {
        self.enabled.get(index).copied().unwrap_or(true)
    }

    /// Dispatches pending events and reports what happened.
    ///
    /// The compositor reports a dismissal (`popup_done`, for instance when the
    /// user clicks outside of the menu) through this connection, so the socket
    /// has to be drained here for the menu to notice.
    pub fn pump(&mut self) -> PopupEvent {
        let _ = self.connection.flush();

        if let Some(guard) = self.queue.prepare_read() {
            // The read never blocks; a lack of events is reported as
            // `WouldBlock` and is not an error.
            if let Err(err) = guard.read()
                && !matches!(
                    &err,
                    WaylandError::Io(err) if err.kind() == io::ErrorKind::WouldBlock
                )
            {
                log::warn!("failed to read popup events: {err}");
            }
        }

        if let Err(err) = self.queue.dispatch_pending(&mut self.state) {
            log::warn!("failed to dispatch popup events: {err}");
            return PopupEvent::Dismissed;
        }

        if self.state.done {
            return PopupEvent::Dismissed;
        }

        let hovered = self
            .state
            .pointer
            .and_then(|(x, y)| self.index_at(x, y))
            .filter(|&index| self.is_enabled(index));

        if hovered != self.hovered {
            self.hovered = hovered;
            self.refresh();
            self.surface.damage_buffer(0, 0, self.width, self.height);
            self.surface.commit();
            let _ = self.connection.flush();
        }

        if let Some((x, y)) = self.state.press.take() {
            return match self.index_at(x, y) {
                Some(index) if self.is_enabled(index) => {
                    PopupEvent::Selected(index)
                }
                // A disabled entry swallows the click and keeps the menu open.
                Some(_) => PopupEvent::None,
                None => PopupEvent::Dismissed,
            };
        }

        PopupEvent::None
    }

    /// Destroys the popup and its surface.
    pub fn destroy(mut self) {
        self.popup.destroy();
        self.xdg_surface.destroy();

        if let Some(buffer) = self.buffer.take() {
            buffer.destroy();
        }

        if let Some(pool) = self.pool.take() {
            pool.destroy();
        }

        self.surface.destroy();
        let _ = self.connection.flush();
    }
}

fn shm_fd(size: usize) -> Result<OwnedFd, Box<dyn std::error::Error>> {
    let name = std::ffi::CString::new("iced-component-menu")?;

    let fd = unsafe {
        libc::memfd_create(
            name.as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };

    if fd < 0 {
        return Err("memfd_create failed".into());
    }

    let fd = unsafe { OwnedFd::from_raw_fd(fd) };

    if unsafe { libc::ftruncate(fd.as_raw_fd(), size as libc::off_t) } < 0 {
        return Err("ftruncate failed".into());
    }

    Ok(fd)
}

/// The process-wide popup, if one has been created.
static POPUP: LazyLock<Mutex<Option<Popup>>> =
    LazyLock::new(|| Mutex::new(None));

/// Shows the context menu, replacing any existing popup.
///
/// # Safety
///
/// See [`Popup::create`].
#[allow(clippy::too_many_arguments)]
pub unsafe fn show(
    display: *mut c_void,
    parent_surface: *mut c_void,
    x: i32,
    y: i32,
    direction: Direction,
    labels: Vec<String>,
    enabled: Vec<bool>,
    style: MenuStyle,
) -> Result<(), Box<dyn std::error::Error>> {
    hide();

    let popup = unsafe {
        Popup::create(
            display,
            parent_surface,
            x,
            y,
            direction,
            labels,
            enabled,
            style,
        )
    }?;

    *POPUP.lock().unwrap() = Some(popup);

    Ok(())
}

/// Pumps the popup, destroying it when the compositor dismisses it.
pub fn pump() -> PopupEvent {
    let mut guard = POPUP.lock().unwrap();

    let Some(popup) = guard.as_mut() else {
        return PopupEvent::None;
    };

    let event = popup.pump();

    if matches!(event, PopupEvent::Dismissed) {
        let popup = guard.take().unwrap();
        popup.destroy();
    }

    event
}

/// Hides the popup, if any.
pub fn hide() {
    if let Some(popup) = POPUP.lock().unwrap().take() {
        popup.destroy();
    }
}
