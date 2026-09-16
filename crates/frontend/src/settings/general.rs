use std::rc::Rc;

use gpui::*;
use gpui_component::{
    WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    select::{Select, SelectEvent},
    v_flex,
};

use crate::{
    component::named_dropdown::{
        DropdownName, NamedDropdown, NamedDropdownItem, SearchableNamedDropdown,
    },
    interface_config::{InterfaceConfig, LiveGameOutputDisplay, PreferredAddContentSource},
    settings::{SettingGroup, SettingItem, SettingItemWidget, SettingPage},
};

pub(super) fn create_page(window: &mut Window, cx: &mut App) -> SettingPage {
    SettingPage {
        title: t::settings::general,
        groups: vec![
            SettingGroup {
                title: None,
                items: vec![
                    SettingItem {
                        title: t::settings::general::general::language,
                        description: t::settings::general::general::language,
                        widget: create_language_dropdown(window, cx),
                        reset: Some(|cx| {
                            t::set_lang(&t::Language::System);
                            InterfaceConfig::get_mut(cx).language = t::Language::System;
                            cx.refresh_windows();
                        }),
                        ..Default::default()
                    },
                    SettingItem {
                        title: t::settings::general::general::quick_delete_mods,
                        description: t::settings::general::general::quick_delete_mods_desc,
                        widget: SettingItemWidget::Switch(
                            |cfg| cfg.quick_delete_mods,
                            |cfg, val| cfg.quick_delete_mods = val,
                        ),
                        ..Default::default()
                    },
                    SettingItem {
                        title: t::settings::general::general::quick_delete_instance,
                        description: t::settings::general::general::quick_delete_instance_desc,
                        widget: SettingItemWidget::Switch(
                            |cfg| cfg.quick_delete_instance,
                            |cfg, val| cfg.quick_delete_instance = val,
                        ),
                        ..Default::default()
                    },
                    SettingItem {
                        title: t::settings::general::general::live_game_output_display,
                        description: t::settings::general::general::live_game_output_display_desc,
                        widget: create_live_game_output_dropdown(window, cx),
                        ..Default::default()
                    },
                ]
                .into(),
                searched_items: None,
            },
            SettingGroup {
                title: Some(t::settings::general::privacy),
                items: vec![
                    SettingItem {
                        title: t::settings::general::privacy::hide_usernames,
                        description: t::settings::general::privacy::hide_usernames_desc,
                        widget: SettingItemWidget::Switch(
                            |cfg| cfg.hide_usernames,
                            |cfg, val| cfg.hide_usernames = val,
                        ),
                        ..Default::default()
                    },
                    SettingItem {
                        title: t::settings::general::privacy::hide_skins,
                        description: t::settings::general::privacy::hide_skins_desc,
                        widget: SettingItemWidget::Switch(
                            |cfg| cfg.hide_skins,
                            |cfg, val| cfg.hide_skins = val,
                        ),
                        ..Default::default()
                    },
                    SettingItem {
                        title: t::settings::general::privacy::hide_server_addresses,
                        description: t::settings::general::privacy::hide_server_addresses_desc,
                        widget: SettingItemWidget::Switch(
                            |cfg| cfg.hide_server_addresses,
                            |cfg, val| cfg.hide_server_addresses = val,
                        ),
                        ..Default::default()
                    },
                    // todo: Only hide when OBS is open
                ]
                .into(),
                searched_items: None,
            },
            SettingGroup {
                title: Some(t::settings::general::content),
                items: vec![
                    SettingItem {
                        title: t::settings::general::content::content_install_latest,
                        description: t::settings::general::content::content_install_latest_desc,
                        widget: SettingItemWidget::Switch(
                            |cfg| cfg.content_install_latest,
                            |cfg, val| cfg.content_install_latest = val,
                        ),
                        ..Default::default()
                    },
                    SettingItem {
                        title: t::settings::general::content::content_filter_version,
                        description: t::settings::general::content::content_filter_version_desc,
                        widget: SettingItemWidget::Switch(
                            |cfg| cfg.content_filter_version,
                            |cfg, val| cfg.content_filter_version = val,
                        ),
                        ..Default::default()
                    },
                    SettingItem {
                        title: t::settings::general::content::show_snapshots,
                        description: t::settings::general::content::show_snapshots_desc,
                        widget: SettingItemWidget::Switch(
                            |cfg| cfg.show_snapshots_in_create_instance,
                            |cfg, val| cfg.show_snapshots_in_create_instance = val,
                        ),
                        ..Default::default()
                    },
                    SettingItem {
                        title: t::settings::general::content::preferred_add_content_source,
                        description:
                            t::settings::general::content::preferred_add_content_source_desc,
                        widget: create_content_source_dropdown(window, cx),
                        ..Default::default()
                    },
                ]
                .into(),
                searched_items: None,
            },
            SettingGroup {
                title: Some(t::settings::reset_all),
                items: vec![SettingItem {
                    title: t::settings::reset_all,
                    description: t::settings::reset_all_desc,
                    widget: SettingItemWidget::Any(Rc::new(move |_window, cx| {
                        let root_entity = cx.entity();
                        Button::new("reset-all-settings")
                            .label(t::settings::reset_all())
                            .danger()
                            .on_click(move |_, window, cx| {
                                open_reset_all_confirm(root_entity.clone(), window, cx);
                            })
                            .into_any_element()
                    })),
                    ..Default::default()
                }]
                .into(),
                searched_items: None,
            },
        ]
        .into(),
        searched_groups: None,
    }
}

fn create_content_source_dropdown(window: &mut Window, cx: &mut App) -> SettingItemWidget {
    let options = vec![
        NamedDropdownItem {
            name: DropdownName::translated(
                t::settings::general::content::preferred_add_content_source::modrinth,
            ),
            item: PreferredAddContentSource::Modrinth,
        },
        NamedDropdownItem {
            name: DropdownName::translated(
                t::settings::general::content::preferred_add_content_source::curseforge,
            ),
            item: PreferredAddContentSource::CurseForge,
        },
        NamedDropdownItem {
            name: DropdownName::translated(
                t::settings::general::content::preferred_add_content_source::file,
            ),
            item: PreferredAddContentSource::File,
        },
    ];

    let initial = InterfaceConfig::get(cx).preferred_add_content_source;
    let dropdown = NamedDropdown::create_and_select(options, initial, window, cx);

    cx.subscribe(&dropdown, |_, event: &SelectEvent<_>, cx| {
        let SelectEvent::Confirm(Some(value)) = event else {
            return;
        };
        InterfaceConfig::get_mut(cx).preferred_add_content_source = *value;
    })
    .detach();

    SettingItemWidget::Any(Rc::new(move |_, _| {
        Select::new(&dropdown)
            .menu_width(px(200.0))
            .into_any_element()
    }))
}

fn open_reset_all_confirm(
    settings_root: Entity<crate::settings::SettingsRoot>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.title(t::settings::reset_all_confirm_title()).child(
            v_flex()
                .gap_3()
                .child(t::settings::reset_all_confirm_message())
                .child(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .child(
                            Button::new("cancel-reset-all")
                                .label(t::common::cancel())
                                .on_click(|_, window, cx| {
                                    window.close_all_dialogs(cx);
                                }),
                        )
                        .child(
                            Button::new("confirm-reset-all")
                                .label(t::settings::reset_all_confirm())
                                .danger()
                                .on_click({
                                    let settings_root = settings_root.clone();
                                    move |_, window, cx| {
                                        reset_all_settings(&settings_root, window, cx);
                                    }
                                }),
                        ),
                ),
        )
    });
}

/// Restore every launcher preference to its default value, persist it,
/// and refresh all setting controls. Only `InterfaceConfig` is touched:
/// instances, worlds, servers, accounts and downloaded content are left
/// alone.
fn reset_all_settings(
    settings_root: &Entity<crate::settings::SettingsRoot>,
    window: &mut Window,
    cx: &mut App,
) {
    {
        let config = InterfaceConfig::get_mut(cx);
        *config = InterfaceConfig::default();
        // Stay exactly on the defaults (an explicit Default Dark theme is
        // stored as `None` and must not flip back to Vexora Dark on restart).
        config.first_run = false;
    }
    t::set_lang(&InterfaceConfig::get(cx).language);
    InterfaceConfig::apply_theme(cx, true);
    InterfaceConfig::force_save(cx);
    _ = settings_root.update(cx, |root, cx| {
        root.rebuild_pages(window, cx);
    });
    window.close_all_dialogs(cx);
    cx.refresh_windows();
}

fn create_language_dropdown(window: &mut Window, cx: &mut App) -> SettingItemWidget {
    let languages = std::iter::once(NamedDropdownItem {
        name: DropdownName::Translated(t::settings::general::general::language::system),
        item: SharedString::new_static("system"),
    })
    .chain(
        t::languages()
            .into_iter()
            .map(|(id, name)| NamedDropdownItem {
                name: DropdownName::Literal(SharedString::new_static(*name)),
                item: SharedString::new_static(*id),
            }),
    )
    .collect();

    let initial = match &InterfaceConfig::get(cx).language {
        t::Language::System => "system".into(),
        t::Language::Code(code) => code.into(),
    };
    let dropdown = SearchableNamedDropdown::create_and_select(languages, initial, window, cx);

    cx.subscribe(&dropdown, |_, event: &SelectEvent<_>, cx| {
        let SelectEvent::Confirm(Some(value)) = event else {
            return;
        };
        let language = match &**value {
            "system" => t::Language::System,
            lang => t::Language::Code(lang.into()),
        };
        t::set_lang(&language);
        InterfaceConfig::get_mut(cx).language = language;
        cx.refresh_windows();
    })
    .detach();

    SettingItemWidget::Any(Rc::new(move |_, _| {
        Select::new(&dropdown)
            .menu_width(px(200.0))
            .search_placeholder(t::common::search())
            .into_any_element()
    }))
}

fn create_live_game_output_dropdown(window: &mut Window, cx: &mut App) -> SettingItemWidget {
    let options = vec![
        NamedDropdownItem {
            name: DropdownName::translated(
                t::settings::general::general::live_game_output_display::tab_on_instance_page,
            ),
            item: LiveGameOutputDisplay::TabOnInstancePage,
        },
        NamedDropdownItem {
            name: DropdownName::translated(
                t::settings::general::general::live_game_output_display::separate_window,
            ),
            item: LiveGameOutputDisplay::SeparateWindow,
        },
        NamedDropdownItem {
            name: DropdownName::translated(
                t::settings::general::general::live_game_output_display::hidden,
            ),
            item: LiveGameOutputDisplay::Hidden,
        },
    ];

    let initial = InterfaceConfig::get(cx).live_game_output_display;
    let dropdown = NamedDropdown::create_and_select(options, initial, window, cx);

    cx.subscribe(&dropdown, |_, event: &SelectEvent<_>, cx| {
        let SelectEvent::Confirm(Some(value)) = event else {
            return;
        };
        InterfaceConfig::get_mut(cx).live_game_output_display = *value;
        cx.refresh_windows();
    })
    .detach();

    SettingItemWidget::Any(Rc::new(move |_, _| {
        Select::new(&dropdown)
            .menu_width(px(200.0))
            .into_any_element()
    }))
}
