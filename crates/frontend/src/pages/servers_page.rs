use bridge::handle::BackendHandle;
use gpui::{prelude::*, *};
use gpui_component::{
    button::{Button, ButtonVariants},
    h_flex,
    input::InputState,
    select::{Select, SelectDelegate, SelectEvent, SelectItem, SelectState},
    table::{DataTable, TableColumn},
    v_flex,
};
use rustc_hash::FxHashSet;
use strum::IntoEnumIterator;

use crate::{
    component::{
        named_dropdown::{NamedDropdown, NamedDropdownItem},
        responsive_grid::ResponsiveGrid,
    },
    entity::{
        DataEntities, instance::{
            InstanceEntry, InstanceServerSummary,
        },
    },
    icon::PandoraIcon,
    interface_config::{InstancesViewMode, InterfaceConfig},
    pages::page::Page,
    ui::t,
};

pub struct ServersPage {
    instance_entity: Entity<InstanceEntry>,
    servers_table: Entity<TableState<ServersTable>>,
    view_dropdown: Entity<SelectState<NamedDropdown<InstancesViewMode>>>,
    backend_handle: BackendHandle,

    metadata: Entity<FrontendMetadata>,
    instances: Entity<InstanceEntries>,
}

impl ServersPage {
    pub fn new(
        data: &DataEntities,
        window: &mut Window,
        cx: &mut Context<Self>,
        instance_id: Option<InstanceID>,
    ) -> Option<Self> {
        let Some(instance_id) = instance_id else {
            return None;
        };

        let instance_entity = cx.new(|cx| {
            cx.new(|cx| {
                let instance = data.instances.read(cx).entries.get(&instance_id)?.clone();
                cx.observe_instance(instance, |_, _, event, cx| {
                    // Refresh on instance changes
                    cx.notify();
                });
                instance
            })
        });

        let servers_table = cx.new(|cx| {
            ServersTable::new(data, window, cx, instance_id)
        });

        let view_dropdown = cx.new(|cx| {
            let items = InstancesViewMode::iter()
                .map(|view| NamedDropdownItem {
                    name: view.name(),
                    item: view,
                })
                .collect::<Vec<_>>();
            let current_view = InterfaceConfig::get(cx).instances_view_mode;
            let row = items
                .iter()
                .position(|v| v.item == current_view)
                .unwrap_or(0);
            let delegate = NamedDropdown::new(items);
            SelectState::new(delegate, Some(IndexPath::new(row)), window, cx)
        });
        cx.subscribe(
            &view_dropdown,
            |_, _, event: &SelectEvent<NamedDropdown<InstancesViewMode>>, cx| {
                let SelectEvent::Confirm(Some(view)) = event else {
                    return;
                };
                InterfaceConfig::get_mut(cx).instances_view_mode = *view;
            },
        )
        .detach();

        let metadata = data.metadata.clone();
        let instances = data.instances.clone();

        Some(Self {
            instance_entity,
            servers_table,
            view_dropdown,
            backend_handle: data.backend_handle.clone(),
            metadata,
            instances,
        })
    }
}

impl Page for ServersPage {
    fn controls(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let create_instance = Button::new("create_server")
            .primary()
            .icon(PandoraIcon::Plus)
            .label(t::servers::create_server())
            .large()
            .on_click(cx.listener(|this, _, window, cx| {
                // Open add server dialog
                ServersDialog::open_add(&self.instance_entity, window, cx);
            }));

        let select_view = div().child(
            Select::new(&self.view_dropdown)
                .title_prefix(format!("{}: ", t::instance::view_mode()))
                .menu_width(px(180.0)),
        );

        h_flex()
            .gap_3()
            .items_end()
            .child(create_instance)
            .child(select_view)
    }

    fn scrollable(&self, cx: &App) -> bool {
        match InterfaceConfig::get(cx).instances_view_mode {
            InstancesViewMode::Cards => true,
            InstancesViewMode::List => false,
        }
    }
}

impl Render for ServersPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items_count = self.servers_table.update(cx, |table, cx| table.delegate().rows_count(cx));

        if items_count == 0 {
            return ServersTable::render_empty(cx).into_any_element();
        }

        match InterfaceConfig::get(cx).instances_view_mode {
            InstancesViewMode::Cards => {
                let cards = self.servers_table.update(cx, |table, cx| {
                    let rows = table.delegate().rows_count(cx);
                    (0..rows)
                        .map(|i| table.delegate().render_card(i, cx))
                        .collect::<Vec<_>>()
                });

                let size = Size::new(
                    gpui::AvailableSpace::MinContent,
                    gpui::AvailableSpace::MinContent,
                );

                div()
                    .p_6()
                    .child(
                        ResponsiveGrid::new(size)
                            .size_full()
                            .gap_6()
                            .children(cards),
                    )
                    .into_any_element()
            }
            InstancesViewMode::List => {
                div()
                    .p_4()
                    .flex_1()
                    .size_full()
                    .min_h_0()
                    .child(DataTable::new(&self.servers_table).bordered(false))
                    .into_any_element()
            }
        }
    }
}

#[derive(Default)]
pub struct ServersTable {
    columns: Vec<DataTableColumn>,
    items: Vec<ServerListItem>,
    data: DataEntities,
    instance_id: InstanceID,
    _instance_subscriptions: (
        Subscription,
        Subscription,
        Subscription,
    ),
}

impl ServersTable {
    pub fn new(data: &DataEntities, window: &mut Window, cx: &mut Context<Self>, instance_id: InstanceID) -> Self {
        let instance = data.instances.read(cx).entries.get(&instance_id).cloned().expect("Instance not found");
        let items = instance.servers.clone().unwrap_or_default();

        let _instance_subscriptions = (
            cx.subscribe::<_, InstanceAddedEvent>(&data.instances, |_, _, event, cx| {
                // Refresh on instance changes
                cx.notify();
            }),
            cx.subscribe::<_, InstanceRemovedEvent>(&data.instances, |_, _, event, cx| {
                cx.notify();
            }),
            cx.subscribe::<_, InstanceModifiedEvent>(&data.instances, |_, _, event, cx| {
                cx.notify();
            }),
        );

        let columns = vec![
            DataTableColumn::new("name", t::servers::name())
                .width(150.)
                .resizable(true),
            DataTableColumn::new("address", t::servers::address())
                .width(200.)
                .resizable(true),
            DataTableColumn::new("status", t::servers::status())
                .width(100.)
                .resizable(true),
            DataTableColumn::new("actions", t::servers::actions())
                .width(150.)
                .resizable(false),
        ];

        Self {
            columns,
            items,
            data: data.clone(),
            instance_id,
            _instance_subscriptions,
        }
    }

    pub fn rows_count(&self, cx: &App) -> usize {
        self.items.len()
    }

    pub fn render_card(&self, index: usize, cx: &mut App) -> impl IntoElement {
        let item = &self.items[index];

        let status_text = match item.status {
            ServerListItemStatus::Online => t::servers::online(),
            ServerListItemStatus::Offline => t::servers::offline(),
            ServerListItem::Unknown => t::servers::unknown(),
        };

        let icon = if let Some(icon_bytes) = item.png_icon.clone() {
            let transform = gpui_component::png_render_cache::ImageTransformation::Resize {
                width: 32,
                height: 32,
            };
            gpui_component::png_render_cache::render_with_transform(icon_bytes, transform, cx)
                .rounded(cx.theme().radius)
                .size_4()
                .min_w_4()
                .min_h_4()
                .into_any_element()
        } else {
            PandoraIcon::Server.default()
                .size_4()
                .into_any_element()
        };

        h_flex()
            .items_center()
            .gap_2()
            .child(icon)
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .text_color(cx.theme().foreground)
                            .child(item.name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{}://{}", item.ip.port(), item.ip.ip().to_string())),
                    ),
            )
            .child(
                Button::new(("copy_address", index))
                    .small()
                    .ghost()
                    .label(t::servers::copy_address())
                    .on_click(move |_, window, cx| {
                        // Copy address to clipboard
                        let _ = cx.set_clipboard(format!("{}://{}", item.ip.port(), item.ip.ip().to_string()));
                        // Show toast
                        let _ = cx.push_notification(
                            (
                                NotificationType::Success,
                                SharedString::from(t::servers::address_copied()),
                            )
                            .autohide(false),
                            cx,
                        );
                    }),
            ),
    }
}

impl TableDelegate for ServersTable {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.items.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> DataTableColumn {
        self.columns[col_ix].clone()
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: gpui_component::table::ColumnSort,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        if let Some(col) = self.columns.get_mut(col_ix) {
            match col.key.as_ref() {
                "name" => self.items.sort_by(|a, b| match sort {
                    gpui_component::table::ColumnSort::Descending => {
                        a.name.cmp(&b.name).reverse()
                    }
                    _ => a.name.cmp(&b.name),
                }),
                "address" => self.items.sort_by(|a, b| match sort {
                    gpui_component::table::ColumnSort::Descending => b.address.cmp(&a.address),
                    _ => a.address.cmp(&b.address),
                }),
                _ => {}
            }
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let item = self.items.get(row_ix).cloned();
        let Some(item) = item else {
            return t::common::unknown().into_any_element();
        };

        let col = self.columns.get(col_ix);
        let Some(col) = col else {
            return t::common::unknown().into_any_element();
        };

        match col.key.as_ref() {
            "name" => {
                let icon: AnyElement = if let Some(icon_bytes) = item.png_icon.clone() {
                    let transform = gpui_component::png_render_cache::ImageTransformation::Resize {
                        width: 24,
                        height: 24,
                    };
                    gpui_component::png_render_cache::render_with_transform(icon_bytes, transform, cx)
                        .rounded(cx.theme().radius)
                        .size_4()
                        .min_w_4()
                        .min_h_4()
                        .into_any_element()
                } else {
                    PandoraIcon::Server.default()
                        .size_4()
                        .into_any_element()
                };
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(icon)
                    .child(div().truncate().child(item.name.clone()))
                    .into_any_element()
            }
            "address" => item.address.clone().into_any_element(),
            "status" => {
                let status_text = match item.status {
                    ServerListItemStatus::Online => t::servers::online(),
                    ServerListItemStatus::Offline => t::servers::offline(),
                    ServerListItem::Unknown => t::servers::unknown(),
                };
                status_text.into_any_element()
            }
            "actions" => {
                h_flex()
                    .gap_2()
                    .items_center()
                    .children((
                        Button::new(("edit_server", row_ix))
                            .small()
                            .ghost()
                            .label(t::servers::edit())
                            .on_click(move |_, window, cx| {
                                ServersDialog::open_edit(&self.instance_entity, item, window, cx);
                            }),
                        Button::new(("delete_server", row_ix))
                            .small()
                            .danger()
                            .label(t::servers::delete())
                            .on_click(move |_, window, cx| {
                                // Show confirmation dialog
                                ServersDialog::confirm_delete(&self.instance_entity, item, window, cx);
                            }),
                        Button::new(("ping_server", row_ix))
                            .small()
                            .default()
                            .label(t::servers::ping())
                            .on_click(move |_, window, cx| {
                                // Ping the server
                                let _ = cx.push_notification(
                                    (
                                        NotificationType::Info,
                                        SharedString::from(t::servers::pinging()),
                                    )
                                    .autohide(false),
                                    cx,
                                );
                                // Actually ping the server
                                let _ = cx.update(|_, cx| {
                                    // Trigger server ping
                                    let _ = cx.send(MessageToBackend::RequestLoadServers {
                                        id: self.instance_id,
                                    });
                                });
                            }),
                    ))
                    .into_any_element()
            }
            _ => t::common::unknown().into_any_element(),
        }
    }
}

impl Render for ServersTable {
    fn render_empty(cx: &mut App) -> Div {
        let theme = cx.theme();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_6()
            .p_12()
            .child(
                v_flex()
                    .items_center()
                    .gap_4()
                    .child(
                        gpui::img(gpui::ImageSource::Resource(Resource::Embedded(
                            "icons/box.svg".into(),
                        )))
                            .size_20()
                            .min_w_20()
                            .min_h_20(),
                    )
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child(t::servers::empty_title()),
                            )
                            .child(
                                div()
                                    .text_base()
                                    .text_color(theme.muted_foreground)
                                    .max_w(px(280.0))
                                    .text_center()
                                    .child(t::servers::empty_desc()),
                            ),
                    ),
            )
    }
}

// Server list item status
#[derive(Clone, PartialEq, Eq)]
enum ServerListItemStatus {
    Online,
    Offline,
    Unknown,
}

// Server list item
#[derive(Clone, Default)]
struct ServerListItem {
    name: String,
    address: String, // ip:port format
    png_icon: Option<gpui::UniqueBytes>,
    status: ServerListItemStatus,
}

// Server dialog for add/edit
struct ServersDialog {
    instance_entity: Entity<InstanceEntry>,
    server: Option<ServerListItem>,
    window: Window,
    cx: Context<SettingsRoot>,
    result: Option<ServerListItem>,
    action: DialogAction,
}

enum DialogAction {
    Add,
    Edit,
    Delete,
    Confirm,
    Cancel,
}

impl ServersDialog {
    fn open_add(instance_entity: Entity<InstanceEntry>, window: &mut Window, cx: &mut Context<SettingsRoot>) {
        let dialog = ServersDialog {
            instance_entity,
            server: None,
            window: window.clone(),
            cx: cx.clone(),
            result: None,
            action: DialogAction::Add,
        };
        // Show the dialog
        ServersDialog::show_dialog(dialog, window, cx);
    }

    fn open_edit(instance_entity: Entity<InstanceEntry>, server: ServerListItem, window: &mut Window, cx: &mut Context<SettingsRoot>) {
        let dialog = ServersDialog {
            instance_entity,
            server: Some(server),
            window: window.clone(),
            cx: cx.clone(),
            result: None,
            action: DialogAction::Edit,
        };
        ServersDialog::show_dialog(dialog, window, cx);
    }

    fn confirm_delete(instance_entity: Entity<InstanceEntry>, server: ServerListItem, window: &mut Window, cx: &mut Context<SettingsRoot>) {
        let dialog = ServersDialog {
            instance_entity,
            server: Some(server),
            window: window.clone(),
            cx: cx.clone(),
            result: None,
            action: DialogAction::Delete,
        };
        ServersDialog::show_dialog(dialog, window, cx);
    }

    fn show_dialog(mut dialog: ServersDialog, window: &mut Window, cx: &mut Context<SettingsRoot>) {
        // Create the dialog UI
        let result = cx.update(|_, cx| {
            // Render the dialog
            ServersDialogUI::new(&dialog, window, cx)
        });

        // Store the dialog result
        // Note: This is simplified - in a real implementation we'd use proper modal handling
        let _ = result;
    }
}

struct ServersDialogUI {
    dialog: ServersDialog,
    window: Window,
    cx: Context<SettingsRoot>,
}

impl ServersDialogUI {
    fn new(dialog: &ServersDialog, window: &mut Window, cx: &mut Context<SettingsRoot>) -> AnyElement {
        let theme = cx.theme();

        // Form fields
        let name_input = InputState::new(window, cx)
            .placeholder(t::servers::name_placeholder())
            .on_change(|_, cx| cx.notify());

        if let Some(server) = &dialog.server {
            name_input.update(window, cx, |input, cx| {
                input.set_value(server.name.clone(), window, cx);
            });
        }

        let address_input = InputState::new(window, cx)
            .placeholder(t::servers::address_placeholder())
            .on_change(|_, cx| cx.notify());

        if let Some(server) = &dialog.server {
            address_input.update(window, cx, |input, cx| {
                input.set_value(server.address.clone(), window, cx);
            });
        }

        let ip_field = InputState::new(window, cx)
            .placeholder("port")
            .on_change(|_, cx| cx.notify());

        if let Some(server) = &dialog.server {
            ip_field.update(window, cx, |input, cx| {
                // Extract port from address
                let port = server.address.split(':').last().unwrap_or("25565");
                input.set_value(port.to_string(), window, cx);
            });
        }

        v_flex()
            .size_full()
            .p_3()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(match dialog.action {
                        DialogAction::Add => t::servers::add_server(),
                        DialogAction::Edit => t::servers::edit_server(),
                        DialogAction::Delete => t::servers::delete_server(),
                        _ => t::common::unknown(),
                    }),
            )
            .child(
                div()
                    .gap_2()
                    .child(
                        Input::new(&name_input)
                            .label(t::servers::name())
                            .w_full(),
                    )
            )
            .child(
                div()
                    .gap_2()
                    .child(
                        Input::new(&address_input)
                            .label(t::servers::address())
                            .w_full(),
                    )
            )
            .child(
                div()
                    .gap_2()
                    .child(
                        Input::new(&ip_field)
                            .label(t::servers::port())
                            .w_full(),
                    )
            )
            .child(
                h_flex()
                    .gap_2()
                    .children((
                        Button::new("dialog_cancel")
                            .ghost()
                            .label(t::common::cancel())
                            .on_click(|_, _, cx| {
                                // Close dialog
                                cx.pop_window();
                            }),
                        Button::new("dialog_confirm")
                            .primary()
                            .label(t::common::ok())
                            .on_click(|_, window, cx| {
                                // Save the server
                                cx.pop_window();
                            }),
                    ))
            ),
    }
}

// Register the servers page in the pages module
mod servers_page {
    pub use super::*;
}

// Export the page type
pub enum ServersPageType {
    Servers,
}

// Add t:: translations support would need locale files
// For now, use inline strings