//! The wlroots screencopy protocol: Hyprland copies each monitor into shared memory we hand it. Much quicker
//! than starting `grim` per monitor, and nothing to install.

use std::os::fd::{AsFd, FromRawFd, OwnedFd};

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

use super::Frame;

#[derive(Default)]
struct Output {
    name: String,
}

#[derive(Default)]
struct Shot {
    format: Option<wl_shm::Format>,
    width: u32,
    height: u32,
    stride: u32,
    y_invert: bool,
    done: bool,
    failed: bool,
}

#[derive(Default)]
struct State {
    outputs: Vec<Output>,
    shots: Vec<Shot>,
}

/// Every monitor named in `names`, in that order. Fails as a whole when the protocol is not there, so the
/// caller can try another way.
pub fn grab(names: &[&str]) -> Result<Vec<Result<Frame, String>>, String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
    let manager: ZwlrScreencopyManagerV1 = globals.bind(&qh, 1..=3, ()).map_err(|e| e.to_string())?;

    // wl_output tells its name ("DP-1") from version 4 on
    let mut state = State::default();
    let outputs: Vec<wl_output::WlOutput> = globals.contents().with_list(|list| {
        list.iter()
            .filter(|g| g.interface == "wl_output" && g.version >= 4)
            .map(|g| globals.registry().bind(g.name, 4, &qh, state_index(&mut state)))
            .collect()
    });
    queue.roundtrip(&mut state).map_err(|e| e.to_string())?;

    let mut frames = Vec::new();
    for name in names {
        let Some(i) = state.outputs.iter().position(|o| o.name == *name) else {
            frames.push(None);
            continue;
        };
        state.shots.push(Shot::default());
        let frame = manager.capture_output(0, &outputs[i], &qh, state.shots.len() - 1);
        frames.push(Some((state.shots.len() - 1, frame)));
    }
    // the compositor answers capture_output with the buffer it wants
    queue.roundtrip(&mut state).map_err(|e| e.to_string())?;

    let mut buffers = Vec::new();
    for (slot, frame) in frames.iter().flatten() {
        let shot = &state.shots[*slot];
        match shot.format {
            Some(format) if supported(format) => {
                let buffer = Buffer::new(&shm, &qh, shot.width, shot.height, shot.stride, format)?;
                frame.copy(&buffer.wl);
                buffers.push((*slot, buffer));
            }
            _ => state.shots[*slot].failed = true,
        }
    }
    while state.shots.iter().any(|s| !s.done && !s.failed) {
        queue.blocking_dispatch(&mut state).map_err(|e| e.to_string())?;
    }

    let mut out = Vec::new();
    for entry in &frames {
        let Some((slot, frame)) = entry else {
            out.push(Err("no such monitor".to_owned()));
            continue;
        };
        frame.destroy();
        let shot = &state.shots[*slot];
        let buffer = buffers.iter().find(|(s, _)| s == slot).map(|(_, b)| b);
        out.push(match (shot.failed, buffer, shot.format) {
            (false, Some(buffer), Some(format)) => Ok(buffer.read(shot, format)),
            _ => Err("the compositor would not copy this monitor".to_owned()),
        });
    }
    for (_, buffer) in buffers {
        buffer.wl.destroy();
    }
    for output in outputs {
        output.release();
    }
    manager.destroy();
    Ok(out)
}

fn state_index(state: &mut State) -> usize {
    state.outputs.push(Output::default());
    state.outputs.len() - 1
}

fn supported(format: wl_shm::Format) -> bool {
    use wl_shm::Format::*;
    matches!(format, Argb8888 | Xrgb8888 | Abgr8888 | Xbgr8888)
}

/// A shared-memory buffer the compositor copies a monitor into.
struct Buffer {
    wl: wl_buffer::WlBuffer,
    map: *mut u8,
    len: usize,
}

// SAFETY: the mapping belongs to this Buffer alone and is only read after the compositor said it is done.
unsafe impl Send for Buffer {}

impl Buffer {
    fn new(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<State>,
        width: u32,
        height: u32,
        stride: u32,
        format: wl_shm::Format,
    ) -> Result<Buffer, String> {
        let len = stride as usize * height as usize;
        // SAFETY: memfd_create with a valid C string; the descriptor is checked before use.
        let raw = unsafe { libc::memfd_create(c"luminescent-screencopy".as_ptr(), libc::MFD_CLOEXEC) };
        if raw < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // SAFETY: `raw` is a new descriptor nobody else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let file = std::fs::File::from(fd);
        file.set_len(len as u64).map_err(|e| e.to_string())?;
        // SAFETY: maps the whole file we just sized; checked against MAP_FAILED below.
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                std::os::fd::AsRawFd::as_raw_fd(&file),
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let pool = shm.create_pool(file.as_fd(), len as i32, qh, ());
        let wl = pool.create_buffer(0, width as i32, height as i32, stride as i32, format, qh, ());
        pool.destroy();
        Ok(Buffer { wl, map: map.cast(), len })
    }

    /// The copied monitor as straight RGBA, the right way up.
    fn read(&self, shot: &Shot, format: wl_shm::Format) -> Frame {
        // SAFETY: the mapping is `len` bytes long and the compositor has finished writing it (ready).
        let bytes = unsafe { std::slice::from_raw_parts(self.map, self.len) };
        let (w, h, stride) = (shot.width as usize, shot.height as usize, shot.stride as usize);
        let bgr = matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888);
        let mut rgba = Vec::with_capacity(w * h * 4);
        for row in 0..h {
            let src = if shot.y_invert { h - 1 - row } else { row };
            let line = &bytes[src * stride..src * stride + w * 4];
            for px in line.chunks_exact(4) {
                // little-endian ARGB is B, G, R, A in memory; ABGR is R, G, B, A
                let (r, g, b) = if bgr { (px[2], px[1], px[0]) } else { (px[0], px[1], px[2]) };
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Frame { width: shot.width, height: shot.height, rgba }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: unmaps exactly the region mmap gave us, once.
        unsafe {
            libc::munmap(self.map.cast(), self.len);
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut State,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, usize> for State {
    fn event(
        state: &mut State,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let wl_output::Event::Name { name } = event
            && let Some(output) = state.outputs.get_mut(*index)
        {
            output.name = name;
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, usize> for State {
    fn event(
        state: &mut State,
        _: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        slot: &usize,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let Some(shot) = state.shots.get_mut(*slot) else { return };
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer { format, width, height, stride } => {
                // the first shared-memory format we can read wins; others may follow
                if let WEnum::Value(format) = format
                    && shot.format.is_none_or(|f| !supported(f))
                {
                    shot.format = Some(format);
                    (shot.width, shot.height, shot.stride) = (width, height, stride);
                }
            }
            zwlr_screencopy_frame_v1::Event::Flags { flags: WEnum::Value(flags) } => {
                shot.y_invert = flags.contains(zwlr_screencopy_frame_v1::Flags::YInvert);
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => shot.done = true,
            zwlr_screencopy_frame_v1::Event::Failed => shot.failed = true,
            _ => {}
        }
    }
}

macro_rules! no_events {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(_: &mut State, _: &$t, _: <$t as wayland_client::Proxy>::Event, _: &(), _: &Connection,
                _: &QueueHandle<State>) {}
        }
    )*};
}

no_events!(wl_shm::WlShm, wl_shm_pool::WlShmPool, wl_buffer::WlBuffer, ZwlrScreencopyManagerV1);
