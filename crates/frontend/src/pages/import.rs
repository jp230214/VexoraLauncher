use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use bridge::{
    handle::BackendHandle,
    import::{ImportFromOtherLauncherJob, OtherLauncher},
    message::MessageToBackend,
    modal_action::ModalAction,
};
use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Disableable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    scroll::ScrollableElement,
    spinner::Spinner,
    v_flex,
};
use rustc_hash::FxHashSet;
use strum::IntoEnumIterator;

use crate::{
    component::{path_label::PathLabel, responsive_grid::ResponsiveGrid},
    entity::{DataEntities, instance::InstanceEntries},
    icon::PandoraIcon,
    interface_config::InterfaceConfig,
    pages::page::Page,
};

pub struct ImportPage {
    backend_handle: BackendHandle,
    instances: Entity<InstanceEntries>,
    import_from: Option<OtherLauncher>,
    import_from_path: Option<PathLabel>,
    import_job: Option<ImportFromOtherLauncherJob>,
    disabled_due_to_name_conflict: FxHashSet<Arc<Path>>,
    disabled_manually: FxHashSet<Arc<Path>>,
    import_accounts: bool,
    import_instances: bool,
    _get_import_job_task: Task<()>,
    _open_file_task: Task<()>,
}

/// Well-known alternative data directories per launcher, checked in
/// order when the default location is missing or yields nothing. These
/// are exact paths (no drive-wide scans): each candidate is accepted only
/// if the launcher's marker files are present, so random folders never
/// qualify.
fn candidate_launcher_dirs(launcher: OtherLauncher, home: &Path) -> Vec<Arc<Path>> {
    fn join(base: &Path, parts: &[&str]) -> Arc<Path> {
        let mut path = base.to_path_buf();
        for part in parts {
            path.push(part);
        }
        path.into()
    }

    // NOTE: the default location is intentionally NOT listed here;
    // `resolve_launcher_dir` always checks it first, so listing it again
    // would only create duplicates.
    let mut candidates: Vec<Arc<Path>> = Vec::new();
    match launcher {
        OtherLauncher::Prism => {
            candidates.push(join(home, &["PrismLauncher"]));
            candidates.push(join(home, &[".local", "share", "PrismLauncher"]));
            candidates.push(join(
                home,
                &[
                    ".var",
                    "app",
                    "org.prismlauncher.PrismLauncher",
                    "data",
                    "PrismLauncher",
                ],
            ));
        }
        OtherLauncher::MultiMC => {
            candidates.push(join(home, &["multimc"]));
            candidates.push(join(home, &["MultiMC"]));
            candidates.push(join(home, &[".local", "share", "multimc"]));
        }
        OtherLauncher::CurseForge => {
            // (On Windows the `Documents/CurseForge` casing variant is the
            // same directory as the default; it is covered by the default
            // check, so it is not listed twice.)
            candidates.push(join(home, &["curseforge", "minecraft"]));
        }
        OtherLauncher::Modrinth => {
            candidates.push(join(home, &["ModrinthApp"]));
            candidates.push(join(home, &[".local", "share", "ModrinthApp"]));
            candidates.push(join(
                home,
                &[
                    ".var",
                    "app",
                    "com.modrinth.ModrinthApp",
                    "data",
                    "ModrinthApp",
                ],
            ));
        }
        OtherLauncher::ATLauncher => {
            candidates.push(join(home, &["ATLauncher"]));
            candidates.push(join(home, &["atlauncher"]));
            candidates.push(join(home, &[".local", "share", "atlauncher"]));
        }
    }
    // Portable installs sometimes live at the drive root. `SystemDrive`
    // is like `C:` (no trailing separator), so anchor it explicitly;
    // joining directly would produce a drive-relative path instead.
    if let Some(drive) = std::env::var_os("SystemDrive") {
        let root = PathBuf::from(format!("{}\\", drive.to_string_lossy()));
        let top_level = match launcher {
            OtherLauncher::Prism => "PrismLauncher",
            OtherLauncher::MultiMC => "multimc",
            OtherLauncher::CurseForge => "curseforge",
            OtherLauncher::Modrinth => "ModrinthApp",
            OtherLauncher::ATLauncher => "ATLauncher",
        };
        candidates.push(root.join(top_level).into());
        if matches!(launcher, OtherLauncher::CurseForge) {
            candidates.push(root.join("curseforge").join("minecraft").into());
        }
    }

    // Deduplicate (case-insensitively: Windows paths).
    let mut seen = FxHashSet::default();
    candidates.retain(|path| {
        let key = path.to_string_lossy().to_lowercase();
        seen.insert(key)
    });
    candidates
}

/// Check whether `dir` actually looks like `launcher`'s data directory by
/// requiring the same marker files the backend importer requires. Pure
/// existence checks: fast, non-recursive, never follows links.
fn is_launcher_dir(launcher: OtherLauncher, dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    match launcher {
        OtherLauncher::Prism | OtherLauncher::MultiMC => {
            dir.join("prismlauncher.cfg").is_file()
                || dir.join("multimc.cfg").is_file()
                || dir.join("fjordlauncher.cfg").is_file()
        }
        OtherLauncher::CurseForge => dir.join("Instances").is_dir(),
        OtherLauncher::Modrinth => dir.join("app.db").is_file(),
        OtherLauncher::ATLauncher => dir.join("configs").join("ATLauncher.json").is_file(),
    }
}

/// Resolve the best directory for `launcher`: remembered location first
/// (if still valid), then the default location, then targeted candidate
/// directories. Returns `None` only when nothing plausible exists.
/// Takes plain paths (instead of `BaseDirs`) so the search order is
/// unit-testable with fake directory trees.
fn resolve_launcher_dir(
    launcher: OtherLauncher,
    remembered: Option<Arc<Path>>,
    default_dir: &Path,
    candidates: &[Arc<Path>],
) -> Option<Arc<Path>> {
    if let Some(remembered) = remembered
        && is_launcher_dir(launcher, &remembered)
    {
        return Some(remembered);
    }
    // Default location first (covers the common case with zero search).
    if is_launcher_dir(launcher, default_dir) {
        return Some(default_dir.into());
    }
    candidates
        .iter()
        .find(|candidate| is_launcher_dir(launcher, candidate))
        .cloned()
}

impl ImportPage {
    pub fn new(data: &DataEntities, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            backend_handle: data.backend_handle.clone(),
            instances: data.instances.clone(),
            import_from: None,
            import_from_path: None,
            import_job: None,
            disabled_due_to_name_conflict: FxHashSet::default(),
            disabled_manually: FxHashSet::default(),
            import_accounts: true,
            import_instances: true,
            _get_import_job_task: Task::ready(()),
            _open_file_task: Task::ready(()),
        }
    }

    pub fn get_import_job(
        &mut self,
        launcher: OtherLauncher,
        path: Arc<Path>,
        cx: &mut Context<Self>,
    ) {
        let (send, recv) = tokio::sync::oneshot::channel();
        self._get_import_job_task = cx.spawn(async move |page, cx| {
            let result: Option<ImportFromOtherLauncherJob> = recv.await.unwrap_or_default();
            let _ = page.update(cx, move |page, cx| {
                page.import_job = result;
                page.disabled_due_to_name_conflict.clear();
                page.disabled_manually.clear();

                if let Some(import_job) = &page.import_job {
                    let instances = page.instances.read(cx);
                    let mut instance_file_names = FxHashSet::default();
                    for entry in instances.entries.values() {
                        let entry = entry.read(cx);
                        if let Some(file_name) = entry.root_path.file_name() {
                            instance_file_names.insert(file_name.to_os_string());
                        }
                    }

                    for path in &import_job.paths {
                        let Some(file_name) = path.file_name() else {
                            continue;
                        };
                        if instance_file_names.contains(file_name) {
                            page.disabled_due_to_name_conflict.insert(path.clone());
                        }
                    }
                }

                cx.notify();
            });
        });

        self.backend_handle
            .send(MessageToBackend::GetImportFromOtherLauncherJob {
                channel: send,
                launcher,
                path,
            });
    }
}

impl Page for ImportPage {
    fn controls(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }

    fn scrollable(&self, _cx: &App) -> bool {
        true
    }
}

impl Render for ImportPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = v_flex().size_full().p_3().gap_3().child(
            ResponsiveGrid::new(Size::new(
                AvailableSpace::MinContent,
                AvailableSpace::MinContent,
            ))
            .gap_2()
            .children({
                OtherLauncher::iter().map(|launcher| {
                    Button::new(launcher.name())
                        .label(t::import::from(launcher.name()))
                        .w_full()
                        .on_click(cx.listener(move |page, _, _, cx| {
                            page.import_from = Some(launcher);

                            let Some(base_dirs) = directories::BaseDirs::new() else {
                                page.import_from_path = None;
                                page.import_job = None;
                                page._get_import_job_task = Task::ready(());
                                return;
                            };

                            // Best directory: remembered location, default
                            // location, then targeted candidate search
                            // (marker-validated, no drive-wide scans).
                            let remembered = InterfaceConfig::get(cx)
                                .import_launcher_dirs
                                .get(launcher.name())
                                .map(|cached| {
                                    let path: Arc<Path> = PathBuf::from(cached).into();
                                    path
                                });
                            let default_path = launcher.default_path(&base_dirs);
                            let candidates =
                                candidate_launcher_dirs(launcher, base_dirs.home_dir());
                            let dir = resolve_launcher_dir(
                                launcher,
                                remembered,
                                &default_path,
                                &candidates,
                            )
                            .unwrap_or(default_path.clone());

                            // Remember successful non-default discoveries so
                            // the next visit goes straight there.
                            if dir != default_path {
                                InterfaceConfig::get_mut(cx).import_launcher_dirs.insert(
                                    launcher.name().to_string(),
                                    dir.to_string_lossy().into_owned(),
                                );
                            }

                            page.import_from_path = Some(PathLabel::new(dir.clone(), true));
                            page.import_job = None;
                            page.get_import_job(launcher, dir, cx);
                        }))
                })
            })
            .child(
                Button::new("mrpack")
                    .label(t::import::from::modrinth())
                    .w_full()
                    .on_click(cx.listener(|page, _, window, cx| {
                        let receiver = cx.prompt_for_paths(PathPromptOptions {
                            files: true,
                            directories: false,
                            multiple: false,
                            prompt: Some(t::import::from::modrinth::select().into()),
                        });
                        let page_entity = cx.entity();
                        page._open_file_task = window.spawn(cx, async move |cx| {
                            let Ok(Ok(Some(result))) = receiver.await else {
                                return;
                            };
                            let Some(path) = result.first() else {
                                return;
                            };
                            _ = page_entity.update_in(cx, |page, window, cx| {
                                let modal_action = ModalAction::default();

                                page.backend_handle.send(
                                    MessageToBackend::CreateInstanceFromFile {
                                        file: path.clone(),
                                        modal_action: modal_action.clone(),
                                    },
                                );

                                crate::modals::generic::show_notification(
                                    window,
                                    cx,
                                    t::instance::content::install::error().into(),
                                    modal_action,
                                );
                            });
                        })
                    })),
            ),
        );

        if let Some(import_from) = self.import_from {
            let label = t::import::from::label(import_from.name());

            let mut import_box = v_flex()
                .w_full()
                .border_1()
                .gap_2()
                .p_2()
                .rounded(cx.theme().radius_lg)
                .border_color(cx.theme().border);

            let pick_folder = cx.listener(move |_, _: &ClickEvent, _, cx| {
                let receiver = cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some(t::import::pick_folder().into()),
                });
                cx.spawn(async move |page, cx| {
                    let Ok(Ok(Some(mut paths))) = receiver.await else {
                        return;
                    };
                    if paths.is_empty() {
                        return;
                    }
                    let path: Arc<Path> = paths.remove(0).into();
                    _ = page.update(cx, |page, cx| {
                        page.import_from_path = Some(PathLabel::new(path.clone(), true));
                        page.import_job = None;
                        page.get_import_job(import_from, path, cx);
                    });
                })
                .detach();
            });

            if let Some(path) = &self.import_from_path {
                import_box = import_box.child(path.button("select-folder").on_click(pick_folder));
            } else {
                import_box = import_box.child(
                    Button::new("select-folder")
                        .success()
                        .label(t::import::select_folder::label())
                        .on_click(pick_folder),
                );
            }

            if let Some(import_job) = &self.import_job {
                import_box = import_box.child(
                    h_flex()
                        .gap_2()
                        .text_color(cx.theme().button_success_foreground)
                        .child(PandoraIcon::Check)
                        .child(t::import::detected_files()),
                );
                if import_job.import_accounts {
                    import_box = import_box.child(
                        Checkbox::new("accounts")
                            .label(t::import::import_accounts())
                            .checked(self.import_accounts)
                            .on_click(cx.listener(|page, checked, _, _| {
                                page.import_accounts = *checked;
                            })),
                    );
                }
                import_box = import_box.child(
                    Checkbox::new("instances")
                        .label(t::import::import_instances())
                        .checked(self.import_instances)
                        .on_click(cx.listener(|page, checked, _, _| {
                            page.import_instances = *checked;
                        })),
                );
                if self.import_instances {
                    import_box = import_box.child(
                        div()
                            .w_full()
                            .border_1()
                            .p_2()
                            .rounded(cx.theme().radius)
                            .border_color(cx.theme().border)
                            .max_h_64()
                            .child(v_flex().overflow_y_scrollbar().gap_2().children(
                                import_job.paths.iter().enumerate().map(|(index, path)| {
                                    if self.disabled_due_to_name_conflict.contains(&*path) {
                                        h_flex()
                                            .gap_4()
                                            .child(
                                                Checkbox::new(index)
                                                    .checked(false)
                                                    .disabled(true)
                                                    .label(&*path.to_string_lossy()),
                                            )
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .line_height(rems(1.0))
                                                    .text_color(
                                                        cx.theme().button_warning_foreground,
                                                    )
                                                    .child(PandoraIcon::TriangleAlert)
                                                    .child(t::import::already_exists()),
                                            )
                                            .into_any_element()
                                    } else {
                                        Checkbox::new(index)
                                            .checked(!self.disabled_manually.contains(&*path))
                                            .label(&*path.to_string_lossy())
                                            .on_click({
                                                let path = path.clone();
                                                cx.listener(move |page, value, _, _| {
                                                    if *value {
                                                        page.disabled_manually.remove(&*path);
                                                    } else {
                                                        page.disabled_manually.insert(path.clone());
                                                    }
                                                })
                                            })
                                            .into_any_element()
                                    }
                                }),
                            )),
                    )
                }
                let import_accounts = import_job.import_accounts && self.import_accounts;
                let can_import = import_accounts
                    || (self.import_instances
                        && self.disabled_due_to_name_conflict.len() + self.disabled_manually.len()
                            != import_job.paths.len());
                import_box = import_box.child(
                    Button::new("doimport")
                        .tooltip(match can_import {
                            true => t::import::enabled(import_from.name()),
                            false => t::import::disabled(import_from.name()),
                        })
                        .disabled(!can_import)
                        .success()
                        .label(label.clone())
                        .on_click(cx.listener(move |page, _, window, cx| {
                            let Some(import_job) = &page.import_job else {
                                return;
                            };

                            let modal_action = ModalAction::default();

                            page.backend_handle
                                .send(MessageToBackend::ImportFromOtherLauncher {
                                    launcher: import_from,
                                    import_job: ImportFromOtherLauncherJob {
                                        import_accounts,
                                        root: import_job.root.clone(),
                                        paths: import_job
                                            .paths
                                            .iter()
                                            .cloned()
                                            .filter(|path| {
                                                !page.disabled_due_to_name_conflict.contains(&*path)
                                                    && !page.disabled_manually.contains(&*path)
                                            })
                                            .collect(),
                                    },
                                    modal_action: modal_action.clone(),
                                });

                            let title = SharedString::new(label.clone());
                            crate::modals::generic::show_modal(
                                window,
                                cx,
                                title,
                                t::import::error_importing().into(),
                                modal_action,
                            );
                        })),
                );
            } else if self._get_import_job_task.is_ready() {
                import_box = import_box.child(
                    h_flex()
                        .gap_2()
                        .text_color(cx.theme().button_danger_foreground)
                        .child(PandoraIcon::TriangleAlert)
                        .child(t::import::no_detected_files()),
                );
            } else {
                import_box = import_box.child(
                    h_flex()
                        .gap_2()
                        .child(Spinner::new())
                        .child(t::import::loading_launcher_data()),
                );
            }

            content = content.child(import_box);
        }

        content
    }
}

#[cfg(test)]
mod tests {
    // NOTE: intentionally *not* `use super::*`: this file's scope pulls in
    // `gpui::prelude::*`, whose `test` macro would shadow the builtin
    // `#[test]` attribute and fail to expand.
    use super::{candidate_launcher_dirs, is_launcher_dir, resolve_launcher_dir};
    use bridge::import::OtherLauncher;
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
    };
    use strum::IntoEnumIterator;

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vexora-import-test-{}-{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn marker_validation_accepts_real_layouts_and_rejects_random_folders() {
        let root = unique_temp_dir("markers");

        // Prism layout with its marker file.
        let prism = root.join("PrismLauncher");
        std::fs::create_dir_all(prism.join("instances")).unwrap();
        std::fs::write(prism.join("prismlauncher.cfg"), "x").unwrap();
        assert!(is_launcher_dir(OtherLauncher::Prism, &prism));

        // Same shape without markers is rejected (no false positives).
        let random = root.join("SomeRandomFolder");
        std::fs::create_dir_all(random.join("instances")).unwrap();
        assert!(!is_launcher_dir(OtherLauncher::Prism, &random));

        // CurseForge wants an Instances directory...
        let cf = root.join("curseforge").join("minecraft");
        std::fs::create_dir_all(cf.join("Instances")).unwrap();
        assert!(is_launcher_dir(OtherLauncher::CurseForge, &cf));
        assert!(!is_launcher_dir(
            OtherLauncher::CurseForge,
            &root.join("curseforge")
        ));

        // ...Modrinth wants app.db, ATLauncher its config.
        let mr = root.join("ModrinthApp");
        std::fs::create_dir_all(&mr).unwrap();
        assert!(!is_launcher_dir(OtherLauncher::Modrinth, &mr));
        std::fs::write(mr.join("app.db"), "x").unwrap();
        assert!(is_launcher_dir(OtherLauncher::Modrinth, &mr));

        let atl = root.join("ATLauncher");
        std::fs::create_dir_all(atl.join("configs")).unwrap();
        assert!(!is_launcher_dir(OtherLauncher::ATLauncher, &atl));
        std::fs::write(atl.join("configs").join("ATLauncher.json"), "{}").unwrap();
        assert!(is_launcher_dir(OtherLauncher::ATLauncher, &atl));

        // Missing and file paths never qualify.
        assert!(!is_launcher_dir(
            OtherLauncher::Prism,
            &root.join("does-not-exist")
        ));
        assert!(!is_launcher_dir(
            OtherLauncher::Prism,
            &prism.join("prismlauncher.cfg")
        ));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn remembered_valid_location_wins_without_search() {
        let root = unique_temp_dir("remembered");
        let dir = root.join("MyPrism");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("multimc.cfg"), "x").unwrap();
        let remembered: Arc<Path> = dir.clone().into();

        let empty: Vec<Arc<Path>> = Vec::new();
        let missing = root.join("Nowhere");
        let resolved = resolve_launcher_dir(
            OtherLauncher::Prism,
            Some(remembered.clone()),
            &missing,
            &empty,
        );
        assert_eq!(resolved, Some(remembered.clone()));

        // A stale remembered path (markers gone) is ignored, never trusted.
        std::fs::remove_file(dir.join("multimc.cfg")).unwrap();
        let resolved =
            resolve_launcher_dir(OtherLauncher::Prism, Some(remembered), &missing, &empty);
        assert!(resolved.map(|p| p.to_path_buf()) != Some(dir.clone()));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn search_order_default_first_then_candidates() {
        // Fake profile: an empty default location plus a valid copy under
        // a candidate directory. Resolution must skip the default and find
        // the candidate through targeted search.
        let root = unique_temp_dir("searchorder");
        let fake_default = root.join("DefaultData").join("PrismLauncher");
        std::fs::create_dir_all(&fake_default).unwrap();

        let fake_home = root.join("Home");
        let home_prism = fake_home.join("PrismLauncher");
        std::fs::create_dir_all(home_prism.join("instances")).unwrap();
        std::fs::write(home_prism.join("prismlauncher.cfg"), "x").unwrap();

        let candidates = candidate_launcher_dirs(OtherLauncher::Prism, &fake_home);
        let resolved = resolve_launcher_dir(OtherLauncher::Prism, None, &fake_default, &candidates);
        assert_eq!(resolved.map(|p| p.to_path_buf()), Some(home_prism.clone()));

        // Nothing valid anywhere -> None (caller falls back to the default
        // path so the user can pick a folder manually).
        std::fs::remove_file(home_prism.join("prismlauncher.cfg")).unwrap();
        let candidates = candidate_launcher_dirs(OtherLauncher::Prism, &fake_home);
        let resolved = resolve_launcher_dir(OtherLauncher::Prism, None, &fake_default, &candidates);
        assert_eq!(resolved, None);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn candidates_are_deduplicated() {
        let home = std::env::temp_dir();
        let base_dirs = directories::BaseDirs::new().unwrap();
        for launcher in OtherLauncher::iter() {
            let candidates = candidate_launcher_dirs(launcher, &home);
            // Targeted alternates only; the default location is checked
            // separately first and must not be duplicated here.
            assert!(!candidates.is_empty());
            let mut seen = std::collections::HashSet::new();
            for candidate in &candidates {
                let key = candidate.to_string_lossy().to_lowercase();
                assert_ne!(
                    key,
                    launcher
                        .default_path(&base_dirs)
                        .to_string_lossy()
                        .to_lowercase(),
                    "default location must not be a search candidate for {}",
                    launcher.name()
                );
                assert!(
                    seen.insert(key),
                    "duplicate candidate for {}",
                    launcher.name()
                );
            }
        }
    }
}
