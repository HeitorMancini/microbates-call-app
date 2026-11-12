//! Screen capture through Windows.Graphics.Capture (`windows-capture`).
//!
//! Source ids: `monitor:<1-based index>` and `window:<HWND>`.

#[cfg(windows)]
pub use imp::WgcScreen;

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use async_trait::async_trait;
    use sheeshcord_core::{
        BoxError, ScreenPreset, ScreenSource, ScreenSourceInfo, ScreenSourceKind,
    };
    use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
    use windows_capture::frame::Frame;
    use windows_capture::graphics_capture_api::InternalCaptureControl;
    use windows_capture::monitor::Monitor;
    use windows_capture::settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    };
    use windows_capture::window::Window;

    use crate::hub::{lock, MediaHub};

    struct Handler {
        hub: MediaHub,
        interval: Duration,
        last: Option<Instant>,
    }

    impl GraphicsCaptureApiHandler for Handler {
        type Flags = (MediaHub, u32);
        type Error = BoxError;

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
            let (hub, fps) = ctx.flags;
            Ok(Self {
                hub,
                interval: Duration::from_secs(1) / fps.max(1),
                last: None,
            })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            _control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            let now = Instant::now();
            // Small tolerance so 60 Hz displays still deliver 60 fps for the 60 fps preset.
            if self.last.is_some_and(|last| {
                now.duration_since(last) + Duration::from_millis(2) < self.interval
            }) {
                return Ok(());
            }
            self.last = Some(now);
            let mut buffer = frame.buffer()?;
            let (width, height, pitch) = (buffer.width(), buffer.height(), buffer.row_pitch());
            self.hub
                .push_screen_bgra(width, height, pitch, buffer.as_raw_buffer());
            Ok(())
        }
    }

    enum Target {
        Monitor(Monitor),
        Window(Window),
    }

    fn resolve(source_id: &str) -> Result<Target, BoxError> {
        if let Some(index) = source_id.strip_prefix("monitor:") {
            return Ok(Target::Monitor(Monitor::from_index(index.parse()?)?));
        }
        if let Some(hwnd) = source_id.strip_prefix("window:") {
            let window = Window::from_raw_hwnd(hwnd.parse::<usize>()? as *mut std::ffi::c_void);
            if !window.is_valid() {
                return Err(format!("janela indisponível: {source_id}").into());
            }
            return Ok(Target::Window(window));
        }
        Err(format!("fonte de tela desconhecida: {source_id}").into())
    }

    fn settings<T>(item: T, flags: (MediaHub, u32)) -> Settings<(MediaHub, u32), T>
    where
        T: TryInto<windows_capture::settings::GraphicsCaptureItemType>,
    {
        Settings::new(
            item,
            CursorCaptureSettings::WithCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            flags,
        )
    }

    /// [`ScreenSource`] backed by Windows.Graphics.Capture (Windows 10 1903+).
    pub struct WgcScreen {
        hub: MediaHub,
        control: Mutex<Option<CaptureControl<Handler, BoxError>>>,
    }

    impl WgcScreen {
        pub fn new(hub: MediaHub) -> Self {
            Self {
                hub,
                control: Mutex::new(None),
            }
        }

        fn stop_capture(&self) {
            let control = lock(&self.control).take();
            if let Some(control) = control {
                if let Err(e) = control.stop() {
                    tracing::warn!(error = %e, "failed to stop screen capture");
                }
            }
        }
    }

    impl Drop for WgcScreen {
        fn drop(&mut self) {
            self.stop_capture();
        }
    }

    fn list_sources() -> Result<Vec<ScreenSourceInfo>, BoxError> {
        let mut sources = Vec::new();
        for (i, monitor) in Monitor::enumerate()?.iter().enumerate() {
            let name = monitor
                .name()
                .or_else(|_| monitor.device_name())
                .unwrap_or_else(|_| format!("Monitor {}", i + 1));
            sources.push(ScreenSourceInfo {
                id: format!("monitor:{}", i + 1),
                name,
                kind: ScreenSourceKind::Monitor,
            });
        }
        for window in Window::enumerate()? {
            if !window.is_valid() {
                continue;
            }
            let Ok(title) = window.title() else { continue };
            if title.trim().is_empty() {
                continue;
            }
            sources.push(ScreenSourceInfo {
                id: format!("window:{}", window.as_raw_hwnd() as usize),
                name: title,
                kind: ScreenSourceKind::Window,
            });
        }
        Ok(sources)
    }

    #[async_trait]
    impl ScreenSource for WgcScreen {
        async fn list_sources(&self) -> Result<Vec<ScreenSourceInfo>, BoxError> {
            tokio::task::spawn_blocking(list_sources).await?
        }

        async fn start(&self, source_id: &str, preset: ScreenPreset) -> Result<(), BoxError> {
            self.stop_capture();
            let flags = (self.hub.clone(), preset.fps());
            let control = match resolve(source_id)? {
                Target::Monitor(m) => Handler::start_free_threaded(settings(m, flags)),
                Target::Window(w) => Handler::start_free_threaded(settings(w, flags)),
            }
            .map_err(|e| format!("falha ao iniciar captura de tela: {e}"))?;
            *lock(&self.control) = Some(control);
            Ok(())
        }

        async fn stop(&self) {
            self.stop_capture();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn rejects_unknown_ids() {
            assert!(resolve("tv:1").is_err());
            assert!(resolve("monitor:abc").is_err());
            assert!(resolve("monitor:0").is_err());
            assert!(resolve("window:0").is_err());
        }

        #[test]
        fn lists_sources_without_panicking() {
            // Headless CI may have no monitors; only the listing contract is checked.
            if let Ok(sources) = list_sources() {
                assert!(sources
                    .iter()
                    .all(|s| s.id.starts_with("monitor:") || s.id.starts_with("window:")));
            }
        }
    }
}
