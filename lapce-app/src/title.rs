use std::{rc::Rc, sync::Arc};

use floem::{
    View,
    event::EventListener,
    menu::{Menu, MenuItem},
    peniko::Color,
    reactive::{
        Memo, ReadSignal, RwSignal, SignalGet, SignalUpdate, SignalWith, create_memo,
    },
    style::{AlignItems, CursorStyle, JustifyContent},
    views::{Decorators, container, drag_window_area, empty, label, stack, svg},
};
use lapce_core::meta;
use lapce_rpc::proxy::ProxyStatus;

use crate::{
    app::{clickable_icon, not_clickable_icon, tooltip_label, window_menu},
    command::{LapceCommand, LapceWorkbenchCommand, WindowCommand},
    config::{LapceConfig, color::LapceColor, icon::LapceIcons},
    i18n::I18n,
    listener::Listener,
    main_split::MainSplitData,
    update::ReleaseInfo,
    window_tab::WindowTabData,
    workspace::LapceWorkspace,
};

fn left(
    workspace: Arc<LapceWorkspace>,
    lapce_command: Listener<LapceCommand>,
    workbench_command: Listener<LapceWorkbenchCommand>,
    config: ReadSignal<Arc<LapceConfig>>,
    i18n: I18n,
    proxy_status: RwSignal<Option<ProxyStatus>>,
    num_window_tabs: Memo<usize>,
) -> impl View {
    let is_local = workspace.kind.is_local();
    let is_macos = cfg!(target_os = "macos");
    stack((
        empty().style(move |s| {
            let should_hide = if is_macos {
                num_window_tabs.get() > 1
            } else {
                true
            };
            s.width(75.0).apply_if(should_hide, |s| s.hide())
        }),
        container(svg(move || config.get().ui_svg(LapceIcons::LOGO)).style(
            move |s| {
                let config = config.get();
                s.size(16.0, 16.0)
                    .color(config.color(LapceColor::LAPCE_ICON_ACTIVE))
            },
        ))
        .style(move |s| s.margin_horiz(10.0).apply_if(is_macos, |s| s.hide())),
        not_clickable_icon(
            || LapceIcons::MENU,
            || false,
            || false,
            i18n.text_signal("toolbar.menu"),
            config,
        )
        .popout_menu({
            let menu_i18n = i18n.clone();
            move || window_menu(lapce_command, workbench_command, menu_i18n.clone())
        })
        .style(move |s| {
            s.margin_left(4.0)
                .margin_right(6.0)
                .apply_if(is_macos, |s| s.hide())
        }),
        tooltip_label(
            config,
            container(svg(move || config.get().ui_svg(LapceIcons::REMOTE)).style(
                move |s| {
                    let config = config.get();
                    let size = (config.ui.icon_size() as f32 + 2.0).min(30.0);
                    s.size(size, size).color(if is_local {
                        config.color(LapceColor::LAPCE_ICON_ACTIVE)
                    } else {
                        match proxy_status.get() {
                            Some(_) => Color::WHITE,
                            None => config.color(LapceColor::LAPCE_ICON_ACTIVE),
                        }
                    })
                },
            )),
            i18n.text_signal("toolbar.connect-remote"),
        )
        .popout_menu(move || {
            #[allow(unused_mut)]
            let mut menu = Menu::new("").entry(
                MenuItem::new(i18n.text("toolbar.connect-ssh")).action(move || {
                    workbench_command.send(LapceWorkbenchCommand::ConnectSshHost);
                }),
            );
            if !is_local
                && proxy_status.get().is_some_and(|p| {
                    matches!(p, ProxyStatus::Connecting | ProxyStatus::Connected)
                })
            {
                menu = menu.entry(
                    MenuItem::new(i18n.text("toolbar.disconnect-remote")).action(
                        move || {
                            workbench_command
                                .send(LapceWorkbenchCommand::DisconnectRemote);
                        },
                    ),
                );
            }
            #[cfg(windows)]
            {
                menu = menu.entry(
                    MenuItem::new(i18n.text("toolbar.connect-wsl")).action(
                        move || {
                            workbench_command
                                .send(LapceWorkbenchCommand::ConnectWslHost);
                        },
                    ),
                );
            }
            menu
        })
        .style(move |s| {
            let config = config.get();
            let color = if is_local {
                Color::TRANSPARENT
            } else {
                match proxy_status.get() {
                    Some(ProxyStatus::Connected) => {
                        config.color(LapceColor::LAPCE_REMOTE_CONNECTED)
                    }
                    Some(ProxyStatus::Connecting) => {
                        config.color(LapceColor::LAPCE_REMOTE_CONNECTING)
                    }
                    Some(ProxyStatus::Disconnected) => {
                        config.color(LapceColor::LAPCE_REMOTE_DISCONNECTED)
                    }
                    None => Color::TRANSPARENT,
                }
            };
            s.height_pct(100.0)
                .padding_horiz(10.0)
                .items_center()
                .background(color)
                .hover(|s| {
                    s.cursor(CursorStyle::Pointer).background(
                        config.color(LapceColor::PANEL_HOVERED_BACKGROUND),
                    )
                })
                .active(|s| {
                    s.cursor(CursorStyle::Pointer).background(
                        config.color(LapceColor::PANEL_HOVERED_ACTIVE_BACKGROUND),
                    )
                })
        }),
        drag_window_area(empty())
            .style(|s| s.height_pct(100.0).flex_basis(0.0).flex_grow(1.0)),
    ))
    .style(move |s| {
        s.height_pct(100.0)
            .flex_basis(0.0)
            .flex_grow(1.0)
            .items_center()
    })
    .debug_name("Left Side of Top Bar")
}

fn middle(
    workspace: Arc<LapceWorkspace>,
    main_split: MainSplitData,
    workbench_command: Listener<LapceWorkbenchCommand>,
    config: ReadSignal<Arc<LapceConfig>>,
    i18n: I18n,
) -> impl View {
    let local_workspace = workspace.clone();
    let can_jump_backward = {
        let main_split = main_split.clone();
        create_memo(move |_| main_split.can_jump_location_backward(true))
    };
    let can_jump_forward =
        create_memo(move |_| main_split.can_jump_location_forward(true));

    let backward_i18n = i18n.clone();
    let jump_backward = move || {
        clickable_icon(
            || LapceIcons::LOCATION_BACKWARD,
            move || {
                workbench_command.send(LapceWorkbenchCommand::JumpLocationBackward);
            },
            || false,
            move || !can_jump_backward.get(),
            backward_i18n.text_signal("toolbar.jump-backward"),
            config,
        )
        .style(move |s| s.margin_horiz(6.0))
    };
    let forward_i18n = i18n.clone();
    let jump_forward = move || {
        clickable_icon(
            || LapceIcons::LOCATION_FORWARD,
            move || {
                workbench_command.send(LapceWorkbenchCommand::JumpLocationForward);
            },
            || false,
            move || !can_jump_forward.get(),
            forward_i18n.text_signal("toolbar.jump-forward"),
            config,
        )
        .style(move |s| s.margin_right(6.0))
    };

    let open_folder_i18n = i18n.clone();
    let open_folder = move || {
        let tooltip_i18n = open_folder_i18n.clone();
        let menu_i18n = open_folder_i18n.clone();
        not_clickable_icon(
            || LapceIcons::PALETTE_MENU,
            || false,
            || false,
            tooltip_i18n.text_signal("toolbar.open-folder-recent"),
            config,
        )
        .popout_menu(move || {
            let menu_i18n = menu_i18n.clone();
            Menu::new("")
                .entry(MenuItem::new(menu_i18n.text("toolbar.open-folder")).action(
                    move || {
                        workbench_command.send(LapceWorkbenchCommand::OpenFolder);
                    },
                ))
                .entry(
                    MenuItem::new(menu_i18n.text("toolbar.open-recent-workspace"))
                        .action(move || {
                            workbench_command
                                .send(LapceWorkbenchCommand::PaletteWorkspace);
                        }),
                )
        })
    };

    stack((
        stack((
            drag_window_area(empty())
                .style(|s| s.height_pct(100.0).flex_basis(0.0).flex_grow(1.0)),
            jump_backward(),
            jump_forward(),
        ))
        .style(|s| {
            s.flex_basis(0)
                .flex_grow(1.0)
                .justify_content(Some(JustifyContent::FlexEnd))
        }),
        container(
            stack((
                svg(move || config.get().ui_svg(LapceIcons::SEARCH)).style(
                    move |s| {
                        let config = config.get();
                        let icon_size = config.ui.icon_size() as f32;
                        s.size(icon_size, icon_size)
                            .color(config.color(LapceColor::LAPCE_ICON_ACTIVE))
                    },
                ),
                label({
                    let label_i18n = i18n.clone();
                    move || {
                        if let Some(s) = local_workspace.display() {
                            s
                        } else {
                            label_i18n.text("toolbar.open-folder")
                        }
                    }
                })
                .style(|s| s.padding_left(10).padding_right(5).selectable(false)),
                open_folder(),
            ))
            .style(|s| s.align_items(Some(AlignItems::Center))),
        )
        .on_event_stop(EventListener::PointerDown, |_| {})
        .on_click_stop(move |_| {
            if workspace.clone().path.is_some() {
                workbench_command.send(LapceWorkbenchCommand::PaletteHelpAndFile);
            } else {
                workbench_command.send(LapceWorkbenchCommand::PaletteWorkspace);
            }
        })
        .style(move |s| {
            let config = config.get();
            s.flex_basis(0)
                .flex_grow(10.0)
                .min_width(200.0)
                .max_width(500.0)
                .height(26.0)
                .justify_content(Some(JustifyContent::Center))
                .align_items(Some(AlignItems::Center))
                .border(1.0)
                .border_color(config.color(LapceColor::LAPCE_BORDER))
                .border_radius(6.0)
                .background(config.color(LapceColor::EDITOR_BACKGROUND))
        }),
        stack((
            clickable_icon(
                || LapceIcons::START,
                move || {
                    workbench_command.send(LapceWorkbenchCommand::PaletteRunAndDebug)
                },
                || false,
                || false,
                i18n.text_signal("toolbar.run-debug"),
                config,
            )
            .style(move |s| s.margin_horiz(6.0)),
            drag_window_area(empty())
                .style(|s| s.height_pct(100.0).flex_basis(0.0).flex_grow(1.0)),
        ))
        .style(move |s| {
            s.flex_basis(0)
                .flex_grow(1.0)
                .justify_content(Some(JustifyContent::FlexStart))
        }),
    ))
    .style(|s| {
        s.flex_basis(0)
            .flex_grow(2.0)
            .align_items(Some(AlignItems::Center))
            .justify_content(Some(JustifyContent::Center))
    })
    .debug_name("Middle of Top Bar")
}

fn right(
    window_command: Listener<WindowCommand>,
    workbench_command: Listener<LapceWorkbenchCommand>,
    latest_release: ReadSignal<Arc<Option<ReleaseInfo>>>,
    update_in_progress: RwSignal<bool>,
    num_window_tabs: Memo<usize>,
    window_maximized: RwSignal<bool>,
    config: ReadSignal<Arc<LapceConfig>>,
    i18n: I18n,
) -> impl View {
    let latest_version = create_memo(move |_| {
        let latest_release = latest_release.get();
        let latest_version =
            latest_release.as_ref().as_ref().map(|r| r.version.clone());
        if latest_version.is_some()
            && latest_version.as_deref() != Some(meta::VERSION)
        {
            latest_version
        } else {
            None
        }
    });

    let has_update = move || latest_version.with(|v| v.is_some());

    stack((
        drag_window_area(empty())
            .style(|s| s.height_pct(100.0).flex_basis(0.0).flex_grow(1.0)),
        stack((
            not_clickable_icon(
                || LapceIcons::SETTINGS,
                || false,
                || false,
                i18n.text_signal("toolbar.settings"),
                config,
            )
            .popout_menu({
                let menu_i18n = i18n.clone();
                move || {
                    Menu::new("")
                        .entry(
                            MenuItem::new(menu_i18n.text("toolbar.command-palette"))
                                .action(move || {
                                    workbench_command
                                        .send(LapceWorkbenchCommand::PaletteCommand)
                                }),
                        )
                        .separator()
                        .entry(
                            MenuItem::new(menu_i18n.text("menu.settings.open"))
                                .action(move || {
                                    workbench_command
                                        .send(LapceWorkbenchCommand::OpenSettings)
                                }),
                        )
                        .entry(
                            MenuItem::new(
                                menu_i18n.text("toolbar.open-keyboard-shortcuts"),
                            )
                            .action(move || {
                                workbench_command.send(
                                    LapceWorkbenchCommand::OpenKeyboardShortcuts,
                                )
                            }),
                        )
                        .entry(
                            MenuItem::new(
                                menu_i18n.text("toolbar.open-theme-settings"),
                            )
                            .action(move || {
                                workbench_command.send(
                                    LapceWorkbenchCommand::OpenThemeColorSettings,
                                )
                            }),
                        )
                        .separator()
                        .entry(if let Some(v) = latest_version.get_untracked() {
                            if update_in_progress.get_untracked() {
                                MenuItem::new(
                                    menu_i18n
                                        .text("toolbar.update-in-progress")
                                        .replace("{version}", &v),
                                )
                                .enabled(false)
                            } else {
                                MenuItem::new(
                                    menu_i18n
                                        .text("toolbar.restart-to-update")
                                        .replace("{version}", &v),
                                )
                                .action(move || {
                                    workbench_command
                                        .send(LapceWorkbenchCommand::RestartToUpdate)
                                })
                            }
                        } else {
                            MenuItem::new(menu_i18n.text("toolbar.no-update"))
                                .enabled(false)
                        })
                        .separator()
                        .entry(
                            MenuItem::new(menu_i18n.text("toolbar.about")).action(
                                move || {
                                    workbench_command
                                        .send(LapceWorkbenchCommand::ShowAbout)
                                },
                            ),
                        )
                }
            }),
            container(label(|| "1".to_string()).style(move |s| {
                let config = config.get();
                s.font_size(10.0)
                    .color(config.color(LapceColor::EDITOR_BACKGROUND))
                    .border_radius(100.0)
                    .margin_left(5.0)
                    .margin_top(10.0)
                    .background(config.color(LapceColor::EDITOR_CARET))
            }))
            .style(move |s| {
                let has_update = has_update();
                s.absolute()
                    .size_pct(100.0, 100.0)
                    .justify_end()
                    .items_end()
                    .pointer_events_none()
                    .apply_if(!has_update, |s| s.hide())
            }),
        ))
        .style(move |s| s.margin_horiz(6.0)),
        window_controls_view(
            window_command,
            true,
            num_window_tabs,
            window_maximized,
            config,
            i18n.clone(),
        ),
    ))
    .style(|s| {
        s.flex_basis(0)
            .flex_grow(1.0)
            .justify_content(Some(JustifyContent::FlexEnd))
    })
    .debug_name("Right of top bar")
}

pub fn title(window_tab_data: Rc<WindowTabData>) -> impl View {
    let workspace = window_tab_data.workspace.clone();
    let lapce_command = window_tab_data.common.lapce_command;
    let workbench_command = window_tab_data.common.workbench_command;
    let window_command = window_tab_data.common.window_common.window_command;
    let latest_release = window_tab_data.common.window_common.latest_release;
    let proxy_status = window_tab_data.common.proxy_status;
    let num_window_tabs = window_tab_data.common.window_common.num_window_tabs;
    let window_maximized = window_tab_data.common.window_common.window_maximized;
    let title_height = window_tab_data.title_height;
    let update_in_progress = window_tab_data.update_in_progress;
    let config = window_tab_data.common.config;
    let i18n = window_tab_data.common.i18n.clone();
    stack((
        left(
            workspace.clone(),
            lapce_command,
            workbench_command,
            config,
            i18n.clone(),
            proxy_status,
            num_window_tabs,
        ),
        middle(
            workspace,
            window_tab_data.main_split.clone(),
            workbench_command,
            config,
            i18n.clone(),
        ),
        right(
            window_command,
            workbench_command,
            latest_release,
            update_in_progress,
            num_window_tabs,
            window_maximized,
            config,
            i18n,
        ),
    ))
    .on_resize(move |rect| {
        let height = rect.height();
        if height != title_height.get_untracked() {
            title_height.set(height);
        }
    })
    .style(move |s| {
        let config = config.get();
        s.width_pct(100.0)
            .height(37.0)
            .items_center()
            .background(config.color(LapceColor::PANEL_BACKGROUND))
            .border_bottom(1.0)
            .border_color(config.color(LapceColor::LAPCE_BORDER))
    })
    .debug_name("Title / Top Bar")
}

pub fn window_controls_view(
    window_command: Listener<WindowCommand>,
    is_title: bool,
    num_window_tabs: Memo<usize>,
    window_maximized: RwSignal<bool>,
    config: ReadSignal<Arc<LapceConfig>>,
    i18n: I18n,
) -> impl View {
    stack((
        clickable_icon(
            || LapceIcons::WINDOW_MINIMIZE,
            || {
                floem::action::minimize_window();
            },
            || false,
            || false,
            i18n.text_signal("window.minimize"),
            config,
        )
        .style(|s| s.margin_right(16.0).margin_left(10.0)),
        clickable_icon(
            move || {
                if window_maximized.get() {
                    LapceIcons::WINDOW_RESTORE
                } else {
                    LapceIcons::WINDOW_MAXIMIZE
                }
            },
            move || {
                floem::action::set_window_maximized(
                    !window_maximized.get_untracked(),
                );
            },
            || false,
            || false,
            {
                let tooltip_i18n = i18n.clone();
                move || {
                    if window_maximized.get() {
                        tooltip_i18n.text("window.restore")
                    } else {
                        tooltip_i18n.text("window.maximize")
                    }
                }
            },
            config,
        )
        .style(|s| s.margin_right(16.0)),
        clickable_icon(
            || LapceIcons::WINDOW_CLOSE,
            move || {
                window_command.send(WindowCommand::CloseWindow);
            },
            || false,
            || false,
            i18n.text_signal("window.close"),
            config,
        )
        .style(|s| s.margin_right(6.0)),
    ))
    .style(move |s| {
        s.apply_if(
            cfg!(target_os = "macos")
                || !config.get_untracked().core.custom_titlebar
                || (is_title && num_window_tabs.get() > 1),
            |s| s.hide(),
        )
    })
}
