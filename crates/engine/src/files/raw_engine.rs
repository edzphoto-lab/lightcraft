use serde::{Deserialize, Serialize};

/// The RAW developer selected for this process. Restarting prevents in-flight work from mixing engines.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RawEngine {
    #[default]
    LightCraft,
    Apple,
    RawTherapee,
    LibRaw,
}

impl RawEngine {
    pub fn label(self) -> &'static str {
        match self {
            Self::LightCraft => "LightCraft",
            Self::Apple => "Apple Core Image",
            Self::RawTherapee => "RawTherapee",
            Self::LibRaw => "LibRaw",
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::RawEngine;
    use crate::files::{Filter, Orientation, Rgb32f, fit, preview::fit_box};
    use std::hash::{Hash, Hasher};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    struct Backend {
        engine: RawEngine,
        executable: Option<PathBuf>,
        cache_tag: u32,
    }

    static BACKEND: OnceLock<Backend> = OnceLock::new();
    // Both converters can use multiple threads and allocate several full-size images at once.
    static EXTERNAL_RENDER: Mutex<()> = Mutex::new(());

    impl RawEngine {
        pub fn executable_name(self) -> Option<&'static str> {
            match self {
                Self::RawTherapee => Some(if cfg!(windows) { "rawtherapee-cli.exe" } else { "rawtherapee-cli" }),
                Self::LibRaw => Some(if cfg!(windows) { "dcraw_emu.exe" } else { "dcraw_emu" }),
                _ => None,
            }
        }
    }

    fn executable(engine: RawEngine, explicit: Option<&Path>) -> Option<PathBuf> {
        if let Some(path) = explicit {
            return std::path::absolute(path).ok().filter(|path| path.is_file());
        }
        let name = engine.executable_name()?;
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .map(|directory| directory.join(name))
            .chain(
                ["/Applications/RawTherapee.app/Contents/MacOS", "/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
                    .into_iter()
                    .filter(|_| cfg!(target_os = "macos"))
                    .map(|directory| Path::new(directory).join(name)),
            )
            .filter(|candidate| candidate.is_absolute())
            .find(|candidate| candidate.is_file())
    }

    /// Configure a native process before opening any photos.
    pub fn configure_raw_engine(engine: RawEngine, explicit: Option<&Path>) {
        BACKEND.get_or_init(|| {
            let executable = executable(engine, explicit);
            let engine = match engine {
                RawEngine::Apple if !apple_available() => RawEngine::LightCraft,
                engine if engine.executable_name().is_some() && executable.is_none() => {
                    log::warn!("{engine:?} executable not found; using LightCraft");
                    RawEngine::LightCraft
                }
                engine => engine,
            };
            let cache_tag = match engine {
                RawEngine::LightCraft => 0,
                RawEngine::Apple => 1,
                RawEngine::RawTherapee | RawEngine::LibRaw => {
                    // Include the installation identity to invalidate a render after changing the executable.
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    engine.executable_name().hash(&mut hash);
                    executable.hash(&mut hash);
                    if let Some(metadata) = executable.as_ref().and_then(|path| path.metadata().ok()) {
                        metadata.len().hash(&mut hash);
                        metadata.modified().ok().hash(&mut hash);
                    }
                    (hash.finish() as u32) | (1 << 31)
                }
            };
            Backend { engine, executable, cache_tag }
        });
    }

    fn apple_available() -> bool {
        #[cfg(target_os = "macos")]
        return lightcraft_apple_raw::available();
        #[cfg(not(target_os = "macos"))]
        false
    }

    pub fn active_raw_engine() -> RawEngine {
        BACKEND.get().map_or(RawEngine::LightCraft, |backend| backend.engine)
    }

    pub fn raw_engine_cache_tag() -> u32 {
        BACKEND.get().map_or(0, |backend| backend.cache_tag)
    }

    pub(in crate::files) fn develop(path: &str, max_edge: usize) -> Result<(Rgb32f, usize), String> {
        let backend = BACKEND.get().ok_or("RAW engine has not been configured")?;
        #[cfg(target_os = "macos")]
        if backend.engine == RawEngine::Apple {
            return lightcraft_apple_raw::decode(path, max_edge);
        }
        let executable = backend.executable.as_ref().ok_or("RAW converter executable is unavailable")?;
        let source = std::path::absolute(path).map_err(|error| error.to_string())?;
        let workspace = tempfile::Builder::new().prefix("lightcraft-raw-").tempdir().map_err(|error| error.to_string())?;
        let output = workspace.path().join("developed.tif");
        let mut command = Command::new(executable);
        command.current_dir(workspace.path()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        match backend.engine {
            RawEngine::RawTherapee => {
                command.args(["-q", "-t", "-b16", "-o"]).arg(&output).arg("-c").arg(&source);
            }
            RawEngine::LibRaw => {
                // As-shot WB and an sRGB 16-bit TIFF. An explicit basename confines output to the temporary folder.
                command.args(["-w", "-o", "1", "-g", "2.4", "12.92", "-6", "-T", "-Z", "developed.tif"]).arg(&source);
            }
            _ => return Err("selected RAW engine is unavailable".into()),
        }
        let status = {
            let _permit = EXTERNAL_RENDER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut child = command.spawn().map_err(|error| error.to_string())?;
            wait(&mut child, Duration::from_secs(120)).map_err(|error| error.to_string())?
        };
        if !status.success() {
            return Err(format!("converter exited with {status}"));
        }
        let bytes = std::fs::read(output).map_err(|error| format!("converter did not produce a readable TIFF: {error}"))?;
        let decoded = lightcraft_codecs::decode(&bytes, fit_box(max_edge)).map_err(|error| error.to_string())?;
        let orientation = Orientation::from_exif(decoded.orientation);
        let image = decoded.to_working();
        let native_long = decoded.source_width.max(decoded.source_height) as usize;
        let image = if image.width.max(image.height) > max_edge { fit(&image, max_edge, max_edge, Filter::Mitchell) } else { image };
        Ok((image.into_oriented(orientation), native_long))
    }

    fn wait(child: &mut Child, timeout: Duration) -> std::io::Result<ExitStatus> {
        let start = Instant::now();
        let result = loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(25)),
                Ok(None) => break std::io::Error::new(std::io::ErrorKind::TimedOut, "RAW converter timed out"),
                Err(error) => break error,
            }
        };
        let _ = child.kill();
        let _ = child.wait();
        Err(result)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) use native::develop;
#[cfg(not(target_arch = "wasm32"))]
pub use native::{active_raw_engine, configure_raw_engine, raw_engine_cache_tag};
#[cfg(target_arch = "wasm32")]
pub const fn raw_engine_cache_tag() -> u32 {
    0
}
