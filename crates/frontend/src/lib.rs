#![deny(unused_must_use)]

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use bridge::{
    handle::{BackendHandle, FrontendReceiver},
    quit::QuitCoordinator,
};
use gpui::*;
use gpui_component::{
    Root, StyledExt, WindowExt,
    notification::{Notification, NotificationType},
};
use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::{
    entity::{
        DataEntities, PanicMessages, account::AccountEntries, instance::InstanceEntries,
        metadata::FrontendMetadata,
    },
    interface_config::InterfaceConfig,
    processor::Processor,
    root::{LauncherRoot, LauncherRootGlobal},
};

pub mod component;
pub mod data_asset_loader;
pub mod entity;
pub mod game_output;
pub mod icon;
pub mod interface_config;
pub mod modals;
pub mod pages;
pub mod png_render_cache;
pub mod processor;
pub mod root;
pub mod settings;
pub mod skin_renderer;
pub mod skin_thumbnail_cache;
pub mod ui;

#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
#[include = "icons/**/*.svg"]
#[include = "icons/**/*.ico"]
#[include = "icons/**/*.icns"]
#[include = "icons/**/*.png"]
#[include = "branding/**/*.svg"]
#[include = "branding/**/*.png"]
#[include = "images/**/*.png"]
#[include = "fonts/**/*.ttf"]
#[include = "themes/**/*.json"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        Self::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow::anyhow!("could not find asset at path \"{path}\""))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

actions!([
    Quit,
    CloseWindow,
    OpenSettings,
    Forwards,
    Backwards,
    Confirm
]);

pub fn start(
    launcher_dir: PathBuf,
    panic_message: Arc<RwLock<Option<String>>>,
    deadlock_message: Arc<RwLock<Option<String>>>,
    backend_handle: BackendHandle,
    mut recv: FrontendReceiver,
    quit_coordinator: QuitCoordinator,
) {
    let http_client =
        Arc::new(reqwest_client::ReqwestClient::user_agent(&*schema::USER_AGENT).unwrap());

    gpui_platform::application()
        .with_http_client(http_client)
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            let _ = cx.text_system().add_fonts(vec![
                Assets
                    .load("fonts/inter/Inter-Regular.ttf")
                    .unwrap()
                    .unwrap(),
                Assets
                    .load("fonts/roboto-mono/RobotoMono-Regular.ttf")
                    .unwrap()
                    .unwrap(),
                Assets.load("fonts/minecraft.ttf").unwrap().unwrap(),
            ]);

            gpui_component::init(cx);
            InterfaceConfig::init(cx, launcher_dir.join("interface.json").into());

            t::set_lang(&InterfaceConfig::get(cx).language);

            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);

            let theme_folder = launcher_dir.join("themes");
            _ = std::fs::create_dir_all(&theme_folder);

            let vexora_theme_path = theme_folder.join("vexora.json");
            // The theme watcher rebuilds the registry exclusively from this
            // folder, so the folder copy must always contain the full
            // bundled set: missing/corrupt copies are (re)written from the
            // embedded asset, and valid-but-stale copies are topped up with
            // any bundled themes absent from them. User edits to existing
            // entries are preserved (matching is by theme name); other user
            // files in the folder are never touched.
            if let Some(theme_data) = Assets::get("themes/vexora.json") {
                refresh_bundled_themes(&vexora_theme_path, theme_data.data.as_ref());
            }

            if let Some(theme_data) = Assets::get("themes/vexora.json") {
                let theme_json = String::from_utf8_lossy(theme_data.data.as_ref());
                match gpui_component::ThemeRegistry::global_mut(cx)
                    .load_themes_from_str(&theme_json)
                {
                    Ok(()) => log::info!(
                        "Loaded embedded Vexora themes ({} total in registry)",
                        gpui_component::ThemeRegistry::global(cx).themes().len()
                    ),
                    Err(err) => log::error!("Failed to load embedded Vexora themes: {err:?}"),
                }
            } else {
                log::error!("Embedded asset themes/vexora.json is missing from the binary");
            }

            _ = gpui_component::ThemeRegistry::watch_dir(theme_folder.clone(), cx, move |cx| {
                InterfaceConfig::apply_theme(cx, true);
            });
            InterfaceConfig::apply_theme(cx, false);

            // First-run default: a fresh install (no interface.json yet, so
            // `first_run` is true) starts on Vexora Dark. Afterwards the
            // flag is cleared and the saved selection is respected as-is.
            // NOTE: an explicit "Default Dark" choice is stored as
            // `active_theme: None`, which must keep meaning Default Dark and
            // must never be rewritten to Vexora Dark.
            if InterfaceConfig::get(cx).first_run {
                {
                    let config = InterfaceConfig::get_mut(cx);
                    config.first_run = false;
                    if config.active_theme.is_none() {
                        config.set_active_theme("Vexora Dark".into());
                    }
                }
                InterfaceConfig::apply_theme(cx, true);
            }

            cx.set_quit_mode(QuitMode::Explicit);

            cx.on_app_quit(|cx| {
                InterfaceConfig::force_save(cx);
                async {}
            })
            .detach();

            let main_window_hidden = Arc::new(AtomicBool::new(false));

            cx.on_window_closed({
                let main_window_hidden = main_window_hidden.clone();
                let quit_coordinator = quit_coordinator.clone();
                move |cx, _window| {
                    if main_window_hidden.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }

                    let windows = cx.windows();

                    let config = InterfaceConfig::get(cx);
                    if config.quit_on_main_closed {
                        for window in &windows {
                            let is_main = window
                                .read(cx, |window: Entity<Root>, cx| {
                                    window
                                        .read(cx)
                                        .view()
                                        .clone()
                                        .downcast::<LauncherRoot>()
                                        .is_ok()
                                })
                                .unwrap_or(false);
                            if is_main {
                                return;
                            }
                        }

                        for window in &windows {
                            _ = window.update(cx, |_, window, _| {
                                window.remove_window();
                            });
                        }
                    }

                    quit_coordinator.set_can_quit(windows.is_empty());
                }
            })
            .detach();

            cx.bind_keys([
                KeyBinding::new("secondary-q", Quit, None),
                KeyBinding::new("secondary-w", CloseWindow, None),
                KeyBinding::new("secondary-,", OpenSettings, None),
                KeyBinding::new("secondary-[", Backwards, None),
                KeyBinding::new("secondary-]", Forwards, None),
                KeyBinding::new("enter", Confirm, None),
            ]);

            cx.on_action(|_: &Quit, cx| {
                for window in cx.windows() {
                    _ = window.update(cx, |_, window, _| {
                        window.remove_window();
                    });
                }
            });

            let instances = cx.new(|_| InstanceEntries {
                entries: IndexMap::new(),
            });
            let metadata = cx.new(|_| FrontendMetadata::new(backend_handle.clone()));
            let accounts = cx.new(|_| AccountEntries::default());
            let data = DataEntities {
                instances,
                metadata,
                backend_handle,
                accounts,
                theme_folder: theme_folder.into(),
                panic_messages: Arc::new(PanicMessages {
                    panic_message,
                    deadlock_message,
                }),
            };

            let mut processor = Processor::new(data.clone(), main_window_hidden, quit_coordinator);

            while let Some(message) = recv.try_recv() {
                processor.process(message, cx);
            }

            cx.spawn(async move |cx| {
                while let Some(message) = recv.recv().await {
                    _ = cx.update(|cx| {
                        processor.process(message, cx);
                    });
                }
            })
            .detach();
        });
}

pub fn open_main_window(data: &DataEntities, cx: &mut App) -> AnyWindowHandle {
    let config = InterfaceConfig::get(cx);

    let window_bounds = match config.main_window_bounds {
        interface_config::WindowBounds::Inherit => None,
        interface_config::WindowBounds::Windowed { x, y, w, h } => Some(WindowBounds::Windowed(
            Bounds::new(Point::new(px(x), px(y)), Size::new(px(w), px(h))),
        )),
        interface_config::WindowBounds::Maximized { x, y, w, h } => Some(WindowBounds::Maximized(
            Bounds::new(Point::new(px(x), px(y)), Size::new(px(w), px(h))),
        )),
        interface_config::WindowBounds::Fullscreen { x, y, w, h } => {
            Some(WindowBounds::Fullscreen(Bounds::new(
                Point::new(px(x), px(y)),
                Size::new(px(w), px(h)),
            )))
        }
    };

    let use_custom_titlebar = !config.use_os_titlebar;
    crate::root::set_should_render_custom_titlebar(use_custom_titlebar);
    let handle = cx
        .open_window(
            WindowOptions {
                app_id: Some("Vexora".into()),
                window_min_size: Some(size(px(480.0), px(270.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Vexora Launcher".into()),
                    appears_transparent: use_custom_titlebar,
                    ..Default::default()
                }),
                app_owns_titlebar_drag: use_custom_titlebar,
                window_bounds,
                window_decorations: Some(if use_custom_titlebar {
                    WindowDecorations::Client
                } else {
                    WindowDecorations::Server
                }),
                ..Default::default()
            },
            |window, cx| {
                let launcher_root = cx.new(|cx| {
                    cx.observe_window_bounds(window, move |_, window, cx| {
                        let origin = window.bounds().origin;
                        let size = window.viewport_size();
                        let new_bounds = (
                            origin.x.to_f64() as f32,
                            origin.y.to_f64() as f32,
                            size.width.to_f64() as f32,
                            size.height.to_f64() as f32,
                        );

                        let old_window_bounds = InterfaceConfig::get(cx).main_window_bounds.clone();
                        let old_bounds = match old_window_bounds {
                            interface_config::WindowBounds::Inherit => new_bounds,
                            interface_config::WindowBounds::Windowed { x, y, w, h } => (x, y, w, h),
                            interface_config::WindowBounds::Maximized { x, y, w, h } => {
                                (x, y, w, h)
                            }
                            interface_config::WindowBounds::Fullscreen { x, y, w, h } => {
                                (x, y, w, h)
                            }
                        };

                        let new_window_bounds = if window.is_fullscreen() {
                            interface_config::WindowBounds::Fullscreen {
                                x: old_bounds.0,
                                y: old_bounds.1,
                                w: old_bounds.2,
                                h: old_bounds.3,
                            }
                        } else if window.is_maximized() {
                            interface_config::WindowBounds::Maximized {
                                x: old_bounds.0,
                                y: old_bounds.1,
                                w: old_bounds.2,
                                h: old_bounds.3,
                            }
                        } else {
                            interface_config::WindowBounds::Windowed {
                                x: new_bounds.0,
                                y: new_bounds.1,
                                w: new_bounds.2,
                                h: new_bounds.3,
                            }
                        };

                        if new_window_bounds != old_window_bounds {
                            InterfaceConfig::get_mut(cx).main_window_bounds = new_window_bounds;
                        }
                    })
                    .detach();

                    LauncherRoot::new(&data, window, cx)
                });

                DataEntities::init_globals(cx);
                cx.set_global(LauncherRootGlobal {
                    root: launcher_root.clone(),
                });
                cx.new(|cx| Root::new(launcher_root, window, cx))
            },
        )
        .unwrap();

    cx.activate(true);

    handle.into()
}

pub(crate) fn is_valid_instance_name(name: &str) -> bool {
    is_single_component_path(name)
        && sanitize_filename::is_sanitized_with_options(
            name,
            sanitize_filename::OptionsForCheck {
                windows: true,
                ..Default::default()
            },
        )
}

pub(crate) fn is_single_component_path(path: &str) -> bool {
    let path = std::path::Path::new(path);
    let mut components = path.components().peekable();

    if let Some(first) = components.peek()
        && !matches!(first, std::path::Component::Normal(_))
    {
        return false;
    }

    components.count() == 1
}

pub(crate) fn get_unique_instance_name(original_name: &str, existing_names: &[&str]) -> String {
    if !existing_names.iter().any(|n| *n == original_name) {
        return original_name.to_string();
    }
    for i in 1..100 {
        let numbered = format!("{original_name} ({i})");
        if !existing_names.iter().any(|n| *n == &numbered) {
            return numbered;
        }
    }
    String::new()
}

#[inline]
pub(crate) fn labelled(label: impl Into<SharedString>, element: impl IntoElement) -> Div {
    gpui_component::v_flex()
        .gap_0p5()
        .child(div().text_sm().font_medium().child(label.into()))
        .child(element)
}

/// Bring the app-owned `vexora.json` in the user theme folder up to date
/// with the embedded bundled set.
///
/// - Missing or unparseable file -> written verbatim from the asset.
/// - Valid file missing some bundled themes (e.g. an older copy from a
///   previous install) -> the absent bundled themes are appended, existing
///   entries (including user edits) are left untouched.
/// - Fully up to date -> file is not rewritten.
fn refresh_bundled_themes(path: &Path, embedded: &[u8]) {
    let Ok(bundled) = serde_json::from_slice::<gpui_component::ThemeSet>(embedded) else {
        log::error!("Embedded Vexora themes are invalid, cannot refresh theme folder");
        return;
    };

    let existing = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<gpui_component::ThemeSet>(&bytes).ok());

    let Some(mut existing) = existing else {
        if let Some(parent) = path.parent() {
            _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(path, embedded).is_err() {
            log::warn!("Unable to write bundled themes to {}", path.display());
        }
        return;
    };

    let missing: Vec<_> = bundled
        .themes
        .iter()
        .filter(|bundled_theme| {
            !existing
                .themes
                .iter()
                .any(|existing_theme| existing_theme.name == bundled_theme.name)
        })
        .cloned()
        .collect();

    if missing.is_empty() {
        return;
    }

    let names: Vec<_> = missing.iter().map(|theme| theme.name.clone()).collect();
    existing.themes.extend(missing);
    match serde_json::to_string_pretty(&existing) {
        Ok(updated) => {
            if std::fs::write(path, updated.as_bytes()).is_err() {
                log::warn!("Unable to update bundled themes at {}", path.display());
            } else {
                log::info!(
                    "Added {} bundled theme(s) to {}: {:?}",
                    names.len(),
                    path.display(),
                    names
                );
            }
        }
        Err(err) => {
            log::warn!("Unable to serialize updated themes: {err:?}");
        }
    }
}

pub(crate) fn open_folder(path: &Path, window: &mut Window, cx: &mut App) {
    let mut is_dir = path.is_dir();
    if !is_dir && !path.exists() {
        _ = std::fs::create_dir_all(path);
        is_dir = true;
    }
    if is_dir {
        if let Err(err) = open::that_detached(path) {
            let notification: Notification = (
                NotificationType::Error,
                SharedString::new(t::file_system::open_folder::error(err)),
            )
                .into();
            window.push_notification(notification.autohide(false), cx);
        }
    } else {
        let notification: Notification = (
            NotificationType::Error,
            t::file_system::open_folder::not_a_directory(),
        )
            .into();
        window.push_notification(notification.autohide(false), cx);
    }
}

pub fn format_downloads(downloads: u64) -> SharedString {
    if downloads >= 1_000_000_000 {
        t::instance::content::downloads::b((downloads / 10_000_000) as f64 / 100.0)
    } else if downloads >= 1_000_000 {
        t::instance::content::downloads::m((downloads / 10_000) as f64 / 100.0)
    } else if downloads >= 10_000 {
        t::instance::content::downloads::k((downloads / 10) as f64 / 100.0)
    } else {
        t::instance::content::downloads::n(downloads)
    }
    .into()
}
