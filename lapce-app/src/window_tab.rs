use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        mpsc::{Sender, channel},
    },
};

use alacritty_terminal::vte::ansi::Handler;
use floem::keyboard::{Key, NamedKey};
use floem::{
    ViewId,
    action::open_file,
    ext_event::{create_ext_action, create_signal_from_channel},
    file::FileDialogOptions,
    keyboard::Modifiers,
    kurbo::Size,
    peniko::kurbo::{Point, Rect},
    prelude::SignalTrack,
    reactive::{
        Memo, ReadSignal, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith,
        WriteSignal, use_context,
    },
    text::{Attrs, AttrsList, FamilyOwned, LineHeightValue, TextLayout},
    views::editor::{
        core::buffer::rope_text::RopeText, core::register::Clipboard,
        text::SystemClipboard,
    },
};
use lapce_core::{
    command::FocusCommand, directory::Directory, meta, mode::Mode,
    register::Register,
};
use lapce_rpc::{
    RpcError,
    core::CoreNotification,
    file::{Naming, PathObject},
    proxy::{ProxyResponse, ProxyRpcHandler},
    source_control::FileDiff,
    terminal::TermId,
};
use lsp_types::{MessageType, ShowMessageParams};
use serde_json::Value;
use tracing::{Level, debug, error, event};

use crate::{
    about::AboutData,
    alert::{AlertBoxData, AlertButton},
    command::{
        CommandExecuted, CommandKind, InternalCommand, LapceCommand,
        LapceWorkbenchCommand, WindowCommand,
    },
    config::LapceConfig,
    db::LapceDb,
    doc::DocContent,
    editor::location::{EditorLocation, EditorPosition},
    editor_tab::EditorTabChild,
    file_explorer::data::FileExplorerData,
    find::Find,
    global_search::GlobalSearchData,
    i18n::I18n,
    id::WindowTabId,
    keypress::{EventRef, KeyPressData, KeyPressFocus, condition::Condition},
    listener::Listener,
    main_split::{MainSplitData, SplitData, SplitDirection, SplitMoveDirection},
    palette::{PaletteData, PaletteStatus, kind::PaletteKind},
    panel::{
        data::{PanelData, PanelSection, default_panel_order, in_scope_panel_order},
        kind::PanelKind,
        position::PanelContainerPosition,
    },
    proxy::{ProxyData, new_proxy},
    source_control::SourceControlData,
    terminal::{
        event::{TermEvent, TermNotification, terminal_update_process},
        panel::TerminalPanelData,
    },
    tracing::*,
    window::WindowCommonData,
    workspace::{LapceWorkspace, LapceWorkspaceType, WorkspaceInfo},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Focus {
    Workbench,
    Palette,
    AboutPopup,
    Panel(PanelKind),
}

#[derive(Clone)]
pub enum DragContent {
    Panel(PanelKind),
    EditorTab(EditorTabChild),
}

impl DragContent {
    pub fn is_panel(&self) -> bool {
        matches!(self, DragContent::Panel(_))
    }
}

#[derive(Clone)]
pub struct CommonData {
    pub i18n: I18n,
    pub workspace: Arc<LapceWorkspace>,
    pub scope: Scope,
    pub focus: RwSignal<Focus>,
    pub keypress: RwSignal<KeyPressData>,
    pub register: RwSignal<Register>,
    pub find: Find,
    pub workbench_size: RwSignal<Size>,
    pub window_origin: RwSignal<Point>,
    pub internal_command: Listener<InternalCommand>,
    pub lapce_command: Listener<LapceCommand>,
    pub workbench_command: Listener<LapceWorkbenchCommand>,
    pub term_tx: Sender<(TermId, TermEvent)>,
    pub term_notification_tx: Sender<TermNotification>,
    pub proxy: ProxyRpcHandler,
    pub view_id: RwSignal<ViewId>,
    pub ui_line_height: Memo<f64>,
    pub dragging: RwSignal<Option<DragContent>>,
    pub config: ReadSignal<Arc<LapceConfig>>,
    // the current focused view which will receive keyboard events
    pub keyboard_focus: RwSignal<Option<ViewId>>,
    pub window_common: Rc<WindowCommonData>,
}

impl std::fmt::Debug for CommonData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommonData")
            .field("workspace", &self.workspace)
            .finish()
    }
}

#[derive(Clone)]
pub struct WindowTabData {
    pub scope: Scope,
    pub window_tab_id: WindowTabId,
    pub workspace: Arc<LapceWorkspace>,
    pub palette: PaletteData,
    pub main_split: MainSplitData,
    pub file_explorer: FileExplorerData,
    pub panel: PanelData,
    pub terminal: TerminalPanelData,
    pub source_control: SourceControlData,
    pub global_search: GlobalSearchData,
    pub about_data: AboutData,
    pub alert_data: AlertBoxData,
    pub layout_rect: RwSignal<Rect>,
    pub title_height: RwSignal<f64>,
    pub status_height: RwSignal<f64>,
    pub proxy: ProxyData,
    pub set_config: WriteSignal<Arc<LapceConfig>>,
    pub update_in_progress: RwSignal<bool>,
    pub messages: RwSignal<Vec<(String, ShowMessageParams)>>,
    pub common: Rc<CommonData>,
}

impl std::fmt::Debug for WindowTabData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowTabData")
            .field("window_tab_id", &self.window_tab_id)
            .finish()
    }
}

impl KeyPressFocus for WindowTabData {
    fn get_mode(&self) -> Mode {
        Mode::Normal
    }

    fn check_condition(&self, condition: Condition) -> bool {
        match condition {
            Condition::PanelFocus => {
                matches!(self.common.focus.get_untracked(), Focus::Panel(_))
            }
            Condition::SourceControlFocus => {
                self.common.focus.get_untracked()
                    == Focus::Panel(PanelKind::SourceControl)
            }
            _ => false,
        }
    }

    fn run_command(
        &self,
        command: &LapceCommand,
        _count: Option<usize>,
        _mods: Modifiers,
    ) -> CommandExecuted {
        match &command.kind {
            CommandKind::Workbench(cmd) => {
                self.run_workbench_command(cmd.clone(), None);
            }
            CommandKind::Focus(cmd) => {
                if self.common.focus.get_untracked() == Focus::Workbench {
                    match cmd {
                        FocusCommand::SplitClose => {
                            self.main_split.editor_tab_child_close_active();
                        }
                        FocusCommand::SplitVertical => {
                            self.main_split.split_active(SplitDirection::Vertical);
                        }
                        FocusCommand::SplitHorizontal => {
                            self.main_split.split_active(SplitDirection::Horizontal);
                        }
                        FocusCommand::SplitRight => {
                            self.main_split
                                .split_move_active(SplitMoveDirection::Right);
                        }
                        FocusCommand::SplitLeft => {
                            self.main_split
                                .split_move_active(SplitMoveDirection::Left);
                        }
                        FocusCommand::SplitUp => {
                            self.main_split
                                .split_move_active(SplitMoveDirection::Up);
                        }
                        FocusCommand::SplitDown => {
                            self.main_split
                                .split_move_active(SplitMoveDirection::Down);
                        }
                        FocusCommand::SplitExchange => {
                            self.main_split.split_exchange_active();
                        }
                        _ => {
                            return CommandExecuted::No;
                        }
                    }
                }
            }
            _ => {
                return CommandExecuted::No;
            }
        }

        CommandExecuted::Yes
    }

    fn receive_char(&self, _c: &str) {}
}

impl WindowTabData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cx: Scope,
        workspace: Arc<LapceWorkspace>,
        window_common: Rc<WindowCommonData>,
    ) -> Self {
        let cx = cx.create_child();
        let db: Arc<LapceDb> = use_context().unwrap();

        let workspace_info = if workspace.path.is_some() {
            db.get_workspace_info(&workspace).ok()
        } else {
            let mut info = db.get_workspace_info(&workspace).ok();
            if let Some(info) = info.as_mut() {
                info.split.children.clear();
            }
            info
        };

        let config = LapceConfig::load(&workspace);
        let i18n = I18n::new(cx, &config.ui.language);
        let lapce_command = Listener::new_empty(cx);
        let workbench_command = Listener::new_empty(cx);
        let internal_command = Listener::new_empty(cx);
        let keypress = cx.create_rw_signal(KeyPressData::new(cx, &config));

        let (term_tx, term_rx) = channel();
        let (term_notification_tx, term_notification_rx) = channel();
        {
            let term_notification_tx = term_notification_tx.clone();
            std::thread::Builder::new()
                .name("terminal update process".to_owned())
                .spawn(move || {
                    terminal_update_process(term_rx, term_notification_tx);
                })
                .unwrap();
        }

        let proxy = new_proxy(workspace.clone(), term_tx.clone());
        let (config, set_config) = cx.create_signal(Arc::new(config));

        let focus = cx.create_rw_signal(Focus::Workbench);
        let register = cx.create_rw_signal(Register::default());
        let view_id = cx.create_rw_signal(ViewId::new());
        let find = Find::new(cx);

        let ui_line_height = cx.create_memo(move |_| {
            let config = config.get();
            let mut text_layout = TextLayout::new();

            let family: Vec<FamilyOwned> =
                FamilyOwned::parse_list(&config.ui.font_family).collect();
            let attrs = Attrs::new()
                .family(&family)
                .font_size(config.ui.font_size() as f32)
                .line_height(LineHeightValue::Normal(1.8));
            let attrs_list = AttrsList::new(attrs);
            text_layout.set_text("W", attrs_list, None);
            text_layout.size().height
        });

        let common = Rc::new(CommonData {
            i18n: i18n.clone(),
            workspace: workspace.clone(),
            scope: cx,
            keypress,
            focus,
            register,
            find,
            internal_command,
            lapce_command,
            workbench_command,
            term_tx,
            term_notification_tx,
            proxy: proxy.proxy_rpc.clone(),
            view_id,
            ui_line_height,
            dragging: cx.create_rw_signal(None),
            workbench_size: cx.create_rw_signal(Size::ZERO),
            config,
            window_origin: cx.create_rw_signal(Point::ZERO),
            keyboard_focus: cx.create_rw_signal(None),
            window_common: window_common.clone(),
        });

        let main_split = MainSplitData::new(cx, common.clone());
        let source_control =
            SourceControlData::new(cx, main_split.editors, common.clone());
        let file_explorer =
            FileExplorerData::new(cx, main_split.editors, common.clone());

        if let Some(info) = workspace_info.as_ref() {
            let root_split = main_split.root_split;
            info.split.to_data(main_split.clone(), None, root_split);
        } else {
            let root_split = main_split.root_split;
            let root_split_data = {
                let cx = cx.create_child();
                let root_split_data = SplitData {
                    scope: cx,
                    parent_split: None,
                    split_id: root_split,
                    children: Vec::new(),
                    direction: SplitDirection::Horizontal,
                    window_origin: Point::ZERO,
                    layout_rect: Rect::ZERO,
                };
                cx.create_rw_signal(root_split_data)
            };
            main_split.splits.update(|splits| {
                splits.insert(root_split, root_split_data);
            });
        }

        let palette = PaletteData::new(
            cx,
            workspace.clone(),
            main_split.clone(),
            keypress.read_only(),
            source_control.clone(),
            common.clone(),
        );

        let title_height = cx.create_rw_signal(0.0);
        let status_height = cx.create_rw_signal(0.0);
        let panel_available_size = cx.create_memo(move |_| {
            let title_height = title_height.get();
            let status_height = status_height.get();
            let num_window_tabs = window_common.num_window_tabs.get();
            let window_size = window_common.size.get();
            Size::new(
                window_size.width,
                window_size.height
                    - title_height
                    - status_height
                    - if num_window_tabs > 1 {
                        window_common.window_tab_header_height.get()
                    } else {
                        0.0
                    },
            )
        });
        let panel = workspace_info
            .as_ref()
            .map(|i| {
                let panel_order = in_scope_panel_order(
                    db.get_panel_orders()
                        .unwrap_or_else(|_| default_panel_order()),
                );
                PanelData {
                    panels: cx.create_rw_signal(panel_order),
                    styles: cx.create_rw_signal(i.panel.styles.clone()),
                    size: cx.create_rw_signal(i.panel.size.clone()),
                    available_size: panel_available_size,
                    sections: cx.create_rw_signal(
                        i.panel
                            .sections
                            .iter()
                            .map(|(key, value)| (*key, cx.create_rw_signal(*value)))
                            .collect(),
                    ),
                    common: common.clone(),
                }
            })
            .unwrap_or_else(|| {
                let panel_order = in_scope_panel_order(
                    db.get_panel_orders()
                        .unwrap_or_else(|_| default_panel_order()),
                );
                PanelData::new(
                    cx,
                    panel_order,
                    panel_available_size,
                    im::HashMap::new(),
                    common.clone(),
                )
            });

        let terminal = TerminalPanelData::new(
            workspace.clone(),
            common.config.get_untracked().terminal.get_default_profile(),
            common.clone(),
            main_split.clone(),
        );
        let global_search = GlobalSearchData::new(cx, main_split.clone());

        {
            let notification = create_signal_from_channel(term_notification_rx);
            let terminal = terminal.clone();
            cx.create_effect(move |_| {
                notification.with(|notification| {
                    if let Some(notification) = notification.as_ref() {
                        match notification {
                            TermNotification::SetTitle { term_id, title } => {
                                terminal.set_title(term_id, title);
                            }
                            TermNotification::RequestPaint => {
                                view_id.get_untracked().request_paint();
                            }
                        }
                    }
                });
            });
        }

        let about_data = AboutData::new(cx, common.focus);
        let alert_data = AlertBoxData::new(cx, common.clone());

        let window_tab_data = Self {
            scope: cx,
            window_tab_id: WindowTabId::next(),
            workspace,
            palette,
            main_split,
            terminal,
            panel,
            file_explorer,
            source_control,
            global_search,
            about_data,
            alert_data,
            layout_rect: cx.create_rw_signal(Rect::ZERO),
            title_height,
            status_height,
            proxy,
            set_config,
            update_in_progress: cx.create_rw_signal(false),
            messages: cx.create_rw_signal(Vec::new()),
            common,
        };

        {
            let active_editor = window_tab_data.main_split.active_editor;
            let internal_command = window_tab_data.common.internal_command;
            cx.create_effect(move |_| {
                active_editor.track();
                internal_command.send(InternalCommand::ResetBlinkCursor);
            });
        }

        {
            let window_tab_data = window_tab_data.clone();
            window_tab_data.common.lapce_command.listen(move |cmd| {
                window_tab_data.run_lapce_command(cmd);
            });
        }

        {
            let window_tab_data = window_tab_data.clone();
            window_tab_data.common.workbench_command.listen(move |cmd| {
                window_tab_data.run_workbench_command(cmd, None);
            });
        }

        {
            let window_tab_data = window_tab_data.clone();
            let internal_command = window_tab_data.common.internal_command;
            internal_command.listen(move |cmd| {
                window_tab_data.run_internal_command(cmd);
            });
        }

        {
            let window_tab_data = window_tab_data.clone();
            let notification = window_tab_data.proxy.notification;
            cx.create_effect(move |_| {
                notification.with(|rpc| {
                    if let Some(rpc) = rpc.as_ref() {
                        window_tab_data.handle_core_notification(rpc);
                    }
                });
            });
        }

        window_tab_data
    }

    pub fn reload_config(&self) {
        let config = LapceConfig::load(&self.workspace);
        self.common.i18n.set_preference(&config.ui.language);
        self.common.keypress.update(|keypress| {
            keypress.update_keymaps(&config);
        });

        self.set_config.set(Arc::new(config.clone()));
    }

    pub fn run_lapce_command(&self, cmd: LapceCommand) {
        match cmd.kind {
            CommandKind::Workbench(command) => {
                self.run_workbench_command(command, cmd.data);
            }
            CommandKind::Scroll(_)
            | CommandKind::Focus(_)
            | CommandKind::Edit(_)
            | CommandKind::Move(_) => {
                if self.palette.status.get_untracked() != PaletteStatus::Inactive {
                    self.palette.run_command(&cmd, None, Modifiers::empty());
                } else if let Some(editor_data) =
                    self.main_split.active_editor.get_untracked()
                {
                    editor_data.run_command(&cmd, None, Modifiers::empty());
                } else {
                    // TODO: dispatch to current focused view?
                }
            }
            CommandKind::MotionMode(_) => {}
            CommandKind::MultiSelection(_) => {}
        }
    }

    pub fn run_workbench_command(
        &self,
        cmd: LapceWorkbenchCommand,
        data: Option<Value>,
    ) {
        use LapceWorkbenchCommand::*;
        match cmd {
            // ==== Modal ====
            EnableModal => {
                let internal_command = self.common.internal_command;
                internal_command.send(InternalCommand::SetModal { modal: true });
            }
            DisableModal => {
                let internal_command = self.common.internal_command;
                internal_command.send(InternalCommand::SetModal { modal: false });
            }

            // ==== Files / Folders ====
            OpenFolder => {
                {
                    let window_command = self.common.window_common.window_command;
                    let mut options = FileDialogOptions::new()
                        .title(self.common.i18n.text("dialog.choose-folder"))
                        .select_directories();
                    options = if let Some(parent) = self.workspace.path.as_ref().and_then(|x| x.parent()) {
                        options.force_starting_directory(parent)
                    } else {
                        options
                    };
                    open_file(options, move |file| {
                        if let Some(mut file) = file {
                            let workspace = LapceWorkspace {
                                kind: LapceWorkspaceType::Local,
                                path: Some(if let Some(path) = file.path.pop() {
                                    path
                                } else {
                                    tracing::error!("No path");
                                    return;
                                }),
                                last_open: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs(),
                            };
                            window_command
                                .send(WindowCommand::SetWorkspace { workspace });
                        }
                    });
                }
            }
            CloseFolder => {
                {
                    let window_command = self.common.window_common.window_command;
                    let workspace = LapceWorkspace {
                        kind: LapceWorkspaceType::Local,
                        path: None,
                        last_open: 0,
                    };
                    window_command.send(WindowCommand::SetWorkspace { workspace });
                }
            }
            OpenFile => {
                {
                    let internal_command = self.common.internal_command;
                    let options = FileDialogOptions::new()
                        .title(self.common.i18n.text("dialog.choose-file"));
                    open_file(options, move |file| {
                        if let Some(mut file) = file {
                            internal_command.send(InternalCommand::OpenFile {
                                path: if let Some(path) = file.path.pop() {
                                    path
                                } else {
                                    tracing::error!("No path");
                                    return;
                                },
                            })
                        }
                    });
                }
            }
            NewFile => {
                self.main_split.new_file();
            }
            RevealActiveFileInFileExplorer => {
                if let Some(editor_data) = self.main_split.active_editor.get() {
                    let doc = editor_data.doc();
                    let path = if let DocContent::File { path, .. } =
                        doc.content.get_untracked()
                    {
                        Some(path)
                    } else {
                        None
                    };
                    let Some(path) = path else { return };
                    let path = path.parent().unwrap_or(&path);

                    open_uri(path);
                }
            }

            SaveAll => {
                self.main_split.editors.with_editors_untracked(|editors| {
                    let mut paths = HashSet::new();
                    for (_, editor_data) in editors.iter() {
                        let doc = editor_data.doc();
                        let should_save = if let DocContent::File { path, .. } =
                            doc.content.get_untracked()
                        {
                            if paths.contains(&path) {
                                false
                            } else {
                                paths.insert(path.clone());

                                true
                            }
                        } else {
                            false
                        };

                        if should_save {
                            editor_data.save(true, || {});
                        }
                    }
                });
            }

            // ==== Configuration / Info Files and Folders ====
            OpenSettings => {
                self.main_split.open_settings();
            }
            OpenSettingsFile => {
                if let Some(path) = LapceConfig::settings_file() {
                    self.main_split.jump_to_location(
                        EditorLocation {
                            path,
                            position: None,
                            scroll_offset: None,
                            ignore_unconfirmed: false,
                            same_editor_tab: false,
                        },
                    );
                }
            }
            OpenSettingsDirectory => {
                if let Some(dir) = Directory::config_directory() {
                    open_uri(&dir);
                }
            }
            OpenThemeColorSettings => {
                self.main_split.open_theme_color_settings();
            }
            OpenKeyboardShortcuts => {
                self.main_split.open_keymap();
            }
            OpenKeyboardShortcutsFile => {
                if let Some(path) = LapceConfig::keymaps_file() {
                    self.main_split.jump_to_location(
                        EditorLocation {
                            path,
                            position: None,
                            scroll_offset: None,
                            ignore_unconfirmed: false,
                            same_editor_tab: false,
                        },
                    );
                }
            }
            OpenLogFile => {
                if let Some(dir) = Directory::logs_directory() {
                    self.open_paths(&[PathObject::from_path(
                        dir.join(format!(
                            "lapce.{}.log",
                            chrono::prelude::Local::now().format("%Y-%m-%d")
                        )),
                        false,
                    )])
                }
            }
            OpenLogsDirectory => {
                if let Some(dir) = Directory::logs_directory() {
                    open_uri(&dir);
                }
            }
            OpenThemesDirectory => {
                if let Some(dir) = Directory::themes_directory() {
                    open_uri(&dir);
                }
            }
            OpenGrammarsDirectory => {
                if let Some(dir) = Directory::grammars_directory() {
                    open_uri(&dir);
                }
            }
            OpenQueriesDirectory => {
                if let Some(dir) = Directory::queries_directory() {
                    open_uri(&dir);
                }
            }

            ExportCurrentThemeSettings => {
                self.main_split.export_theme();
            }

            // ==== Window ====
            ReloadWindow => {
                self.common.window_common.window_command.send(
                    WindowCommand::SetWorkspace {
                        workspace: (*self.workspace).clone(),
                    },
                );
            }
            NewWindow => {
                self.common
                    .window_common
                    .window_command
                    .send(WindowCommand::NewWindow);
            }
            CloseWindow => {
                self.common
                    .window_common
                    .window_command
                    .send(WindowCommand::CloseWindow);
            }
            // ==== Window Tabs ====
            NewWindowTab => {
                self.common.window_common.window_command.send(
                    WindowCommand::NewWorkspaceTab {
                        workspace: LapceWorkspace::default(),
                        end: false,
                    },
                );
            }
            CloseWindowTab => {
                self.common
                    .window_common
                    .window_command
                    .send(WindowCommand::CloseWorkspaceTab { index: None });
            }
            NextWindowTab => {
                self.common
                    .window_common
                    .window_command
                    .send(WindowCommand::NextWorkspaceTab);
            }
            PreviousWindowTab => {
                self.common
                    .window_common
                    .window_command
                    .send(WindowCommand::PreviousWorkspaceTab);
            }

            // ==== Editor Tabs ====
            NextEditorTab => {
                if let Some(editor_tab_id) =
                    self.main_split.active_editor_tab.get_untracked()
                {
                    self.main_split.editor_tabs.with_untracked(|editor_tabs| {
                        let Some(editor_tab) = editor_tabs.get(&editor_tab_id)
                        else {
                            return;
                        };

                        let new_index = editor_tab.with_untracked(|editor_tab| {
                            if editor_tab.children.is_empty() {
                                None
                            } else if editor_tab.active
                                == editor_tab.children.len() - 1
                            {
                                Some(0)
                            } else {
                                Some(editor_tab.active + 1)
                            }
                        });

                        if let Some(new_index) = new_index {
                            editor_tab.update(|editor_tab| {
                                editor_tab.active = new_index;
                            });
                        }
                    });
                }
            }
            PreviousEditorTab => {
                if let Some(editor_tab_id) =
                    self.main_split.active_editor_tab.get_untracked()
                {
                    self.main_split.editor_tabs.with_untracked(|editor_tabs| {
                        let Some(editor_tab) = editor_tabs.get(&editor_tab_id)
                        else {
                            return;
                        };

                        let new_index = editor_tab.with_untracked(|editor_tab| {
                            if editor_tab.children.is_empty() {
                                None
                            } else if editor_tab.active == 0 {
                                Some(editor_tab.children.len() - 1)
                            } else {
                                Some(editor_tab.active - 1)
                            }
                        });

                        if let Some(new_index) = new_index {
                            editor_tab.update(|editor_tab| {
                                editor_tab.active = new_index;
                            });
                        }
                    });
                }
            }

            // ==== Terminal ====
            NewTerminalTab => {
                self.terminal.new_tab(
                    self.common
                        .config
                        .get_untracked()
                        .terminal
                        .get_default_profile(),
                );
                if !self.panel.is_panel_visible(&PanelKind::Terminal) {
                    self.panel.show_panel(&PanelKind::Terminal);
                }
                self.common.focus.set(Focus::Panel(PanelKind::Terminal));
            }
            CloseTerminalTab => {
                self.terminal.close_tab(None);
                if self
                    .terminal
                    .tab_info
                    .with_untracked(|info| info.tabs.is_empty())
                {
                    if self.panel.is_panel_visible(&PanelKind::Terminal) {
                        self.panel.hide_panel(&PanelKind::Terminal);
                    }
                    self.common.focus.set(Focus::Workbench);
                } else {
                    if !self.panel.is_panel_visible(&PanelKind::Terminal) {
                        self.panel.show_panel(&PanelKind::Terminal);
                    }
                    self.common.focus.set(Focus::Panel(PanelKind::Terminal));
                }
            }
            NextTerminalTab => {
                self.terminal.next_tab();
                if !self.panel.is_panel_visible(&PanelKind::Terminal) {
                    self.panel.show_panel(&PanelKind::Terminal);
                }
                self.common.focus.set(Focus::Panel(PanelKind::Terminal));
            }
            PreviousTerminalTab => {
                self.terminal.previous_tab();
                if !self.panel.is_panel_visible(&PanelKind::Terminal) {
                    self.panel.show_panel(&PanelKind::Terminal);
                }
                self.common.focus.set(Focus::Panel(PanelKind::Terminal));
            }

            // ==== Palette Commands ====
            PaletteHelp => self.palette.run(PaletteKind::PaletteHelp),
            PaletteHelpAndFile => self.palette.run(PaletteKind::HelpAndFile),
            PaletteLine => {
                self.palette.run(PaletteKind::Line);
            }
            Palette => {
                self.palette.run(PaletteKind::File);
            }
            PaletteCommand => {
                self.palette.run(PaletteKind::Command);
            }
            PaletteWorkspace => {
                self.palette.run(PaletteKind::Workspace);
            }
            PaletteSCMReferences => {
                self.palette.run(PaletteKind::SCMReferences);
            }
            ChangeColorTheme => {
                self.palette.run(PaletteKind::ColorTheme);
            }
            ChangeIconTheme => {
                self.palette.run(PaletteKind::IconTheme);
            }
            ChangeFileLanguage => {
                self.palette.run(PaletteKind::Language);
            }
            ChangeFileLineEnding => {
                self.palette.run(PaletteKind::LineEnding);
            }
            DiffFiles => self.palette.run(PaletteKind::DiffFiles),

            // ==== UI ====
            ZoomIn => {
                let mut scale =
                    self.common.window_common.window_scale.get_untracked();
                scale += 0.1;
                if scale > 4.0 {
                    scale = 4.0
                }
                self.common.window_common.window_scale.set(scale);

                LapceConfig::update_file(
                    "ui",
                    "scale",
                    toml_edit::Value::from(scale),
                );
            }
            ZoomOut => {
                let mut scale =
                    self.common.window_common.window_scale.get_untracked();
                scale -= 0.1;
                if scale < 0.1 {
                    scale = 0.1
                }
                self.common.window_common.window_scale.set(scale);

                LapceConfig::update_file(
                    "ui",
                    "scale",
                    toml_edit::Value::from(scale),
                );
            }
            ZoomReset => {
                self.common.window_common.window_scale.set(1.0);

                LapceConfig::update_file(
                    "ui",
                    "scale",
                    toml_edit::Value::from(1.0),
                );
            }

            ToggleMaximizedPanel => {
                if let Some(data) = data {
                    if let Ok(kind) = serde_json::from_value::<PanelKind>(data) {
                        self.panel.toggle_maximize(&kind);
                    }
                } else {
                    self.panel.toggle_active_maximize();
                }
            }
            HidePanel => {
                if let Some(data) = data {
                    if let Ok(kind) = serde_json::from_value::<PanelKind>(data) {
                        self.hide_panel(kind);
                    }
                }
            }
            ShowPanel => {
                if let Some(data) = data {
                    if let Ok(kind) = serde_json::from_value::<PanelKind>(data) {
                        self.show_panel(kind);
                    }
                }
            }
            TogglePanelFocus => {
                if let Some(data) = data {
                    if let Ok(kind) = serde_json::from_value::<PanelKind>(data) {
                        self.toggle_panel_focus(kind);
                    }
                }
            }
            TogglePanelVisual => {
                if let Some(data) = data {
                    if let Ok(kind) = serde_json::from_value::<PanelKind>(data) {
                        self.toggle_panel_visual(kind);
                    }
                }
            }
            TogglePanelLeftVisual => {
                self.toggle_container_visual(&PanelContainerPosition::Left);
            }
            TogglePanelRightVisual => {
                self.toggle_container_visual(&PanelContainerPosition::Right);
            }
            TogglePanelBottomVisual => {
                self.toggle_container_visual(&PanelContainerPosition::Bottom);
            }
            ToggleTerminalFocus => {
                self.toggle_panel_focus(PanelKind::Terminal);
            }
            ToggleSourceControlFocus => {
                self.toggle_panel_focus(PanelKind::SourceControl);
            }
            ToggleFileExplorerFocus => {
                self.toggle_panel_focus(PanelKind::FileExplorer);
            }
            ToggleSearchFocus => {
                self.toggle_panel_focus(PanelKind::Search);
            }
            ToggleTerminalVisual => {
                self.toggle_panel_visual(PanelKind::Terminal);
            }
            ToggleSourceControlVisual => {
                self.toggle_panel_visual(PanelKind::SourceControl);
            }
            ToggleFileExplorerVisual => {
                self.toggle_panel_visual(PanelKind::FileExplorer);
            }
            ToggleSearchVisual => {
                self.toggle_panel_visual(PanelKind::Search);
            }
            FocusEditor => {
                self.common.focus.set(Focus::Workbench);
            }
            FocusTerminal => {
                self.common.focus.set(Focus::Panel(PanelKind::Terminal));
            }
            OpenUIInspector => {
                self.common.view_id.get_untracked().inspect();
            }
            ShowEnvironment => {
                self.main_split.show_env();
            }

            // ==== Source Control ====
            SourceControlInit => {
                self.proxy.proxy_rpc.git_init();
            }
            CheckoutReference => match data {
                Some(reference) => {
                    if let Some(reference) = reference.as_str() {
                        self.proxy.proxy_rpc.git_checkout(reference.to_string());
                    }
                }
                None => error!("No ref provided"),
            },
            SourceControlCommit => {
                self.source_control.commit();
            }
            SourceControlCopyActiveFileRemoteUrl => {
                if let Some(editor_data) =
                    self.main_split.active_editor.get_untracked()
                {
                    if let DocContent::File { path, .. } =
                        editor_data.doc().content.get_untracked()
                    {
                        let window_tab = self.clone();
                        self.common.proxy.git_get_remote_file_url(
                            path,
                            create_ext_action(
                                self.scope,
                                move |result: Result<ProxyResponse, RpcError>| {
                                match result {
                                    Ok(ProxyResponse::GitGetRemoteFileUrl {
                                        file_url,
                                    }) => {
                                        let mut clipboard = SystemClipboard::new();
                                        clipboard.put_string(&file_url);
                                    }
                                    Ok(_) => {}
                                    Err(err) => window_tab.show_message(
                                        "Copy Remote File Url failure",
                                        &ShowMessageParams {
                                            typ: MessageType::ERROR,
                                            message: err.message,
                                        },
                                    ),
                                }
                            }),
                        );
                    }
                }
            }
            SourceControlDiscardActiveFileChanges => {
                let active_path = self
                    .main_split
                    .active_editor
                    .get_untracked()
                    .and_then(|editor_data| {
                        editor_data
                            .doc()
                            .content
                            .with_untracked(|content| content.path().cloned())
                    });
                let diff = active_path.and_then(|path| {
                    self.source_control
                        .file_diffs
                        .with_untracked(|diffs| diffs.get(&path).map(|(diff, _)| diff.clone()))
                });
                if let Some(diff) = diff {
                    match diff {
                        FileDiff::Added(path) => {
                            self.common.proxy.trash_path(path, Box::new(|_| {}));
                        }
                        FileDiff::Modified(path) | FileDiff::Deleted(path) => {
                            self.common.proxy.git_discard_files_changes(vec![path]);
                        }
                        FileDiff::Renamed(new_path, old_path) => {
                            self.common
                                .proxy
                                .git_discard_files_changes(vec![old_path]);
                            self.common
                                .proxy
                                .trash_path(new_path, Box::new(|_| {}));
                        }
                    }
                }
            }
            SourceControlDiscardTargetFileChanges => {
                if let Some(diff) = data
                    .and_then(|data| serde_json::from_value::<FileDiff>(data).ok())
                {
                    match diff {
                        FileDiff::Added(path) => {
                            self.common.proxy.trash_path(path, Box::new(|_| {}));
                        }
                        FileDiff::Modified(path) | FileDiff::Deleted(path) => {
                            self.common.proxy.git_discard_files_changes(vec![path]);
                        }
                        FileDiff::Renamed(new_path, old_path) => {
                            self.common
                                .proxy
                                .git_discard_files_changes(vec![old_path]);
                            self.common
                                .proxy
                                .trash_path(new_path, Box::new(|_| {}));
                        }
                    }
                }
            }
            SourceControlDiscardWorkspaceChanges => {
                let workspace_has_changes = self
                    .source_control
                    .file_diffs
                    .with_untracked(|diffs| !diffs.is_empty());
                if !workspace_has_changes {
                    return;
                }
                let internal_command = self.common.internal_command;
                let proxy = self.common.proxy.clone();
                let i18n = self.common.i18n.clone();
                // checkout_index only restores tracked files, so untracked/added
                // files have to be removed explicitly for "discard all" to hold
                // its promise.
                let added_files: Vec<PathBuf> = self
                    .source_control
                    .file_diffs
                    .with_untracked(|diffs| {
                        diffs
                            .iter()
                            .filter_map(|(_, (diff, _))| match diff {
                                FileDiff::Added(path) => Some(path.clone()),
                                _ => None,
                            })
                            .collect()
                    });
                self.common.internal_command.send(InternalCommand::ShowAlert {
                    title: i18n.text("dialog.discard-workspace-changes.title"),
                    msg: i18n.text("dialog.discard-workspace-changes.message"),
                    buttons: vec![AlertButton {
                        text: i18n.text("dialog.discard-workspace-changes.confirm"),
                        action: Rc::new(move || {
                            internal_command.send(InternalCommand::HideAlert);
                            for path in &added_files {
                                proxy.trash_path(path.clone(), Box::new(|_| {}));
                            }
                            proxy.git_discard_workspace_changes();
                        }),
                    }],
                });
            }

            // ==== UI ====
            ShowAbout => {
                self.about_data.open();
            }

            // ==== Updating ====
            RestartToUpdate => {
                if let Some(release) = self
                    .common
                    .window_common
                    .latest_release
                    .get_untracked()
                    .as_ref()
                {
                    let release = release.clone();
                    let update_in_progress = self.update_in_progress;
                    if release.version != *meta::VERSION {
                        if let Ok(process_path) = env::current_exe() {
                            update_in_progress.set(true);
                            let send = create_ext_action(
                                self.common.scope,
                                move |_started| {
                                    update_in_progress.set(false);
                                },
                            );
                            std::thread::Builder::new().name("RestartToUpdate".to_owned()).spawn(move || {
                                let do_update = || -> anyhow::Result<()> {
                                    let src =
                                        crate::update::download_release(&release)?;

                                    let path =
                                        crate::update::extract(&src, &process_path)?;

                                    crate::update::restart(&path)?;

                                    Ok(())
                                };

                                if let Err(err) = do_update() {
                                    error!("Failed to update: {err}");
                                }

                                send(false);
                            }).unwrap();
                        }
                    }
                }
            }

            // ==== Movement ====
            #[cfg(target_os = "macos")]
            InstallToPATH => {
                self.common.internal_command.send(
                    InternalCommand::ExecuteProcess {
                        program: String::from("osascript"),
                        arguments: vec![String::from("-e"), format!(r#"do shell script "ln -sf '{}' /usr/local/bin/lapce" with administrator privileges"#, std::env::args().next().unwrap())],
                    }
                )
            }
            #[cfg(target_os = "macos")]
            UninstallFromPATH => {
                self.common.internal_command.send(
                    InternalCommand::ExecuteProcess {
                        program: String::from("osascript"),
                        arguments: vec![String::from("-e"), String::from(r#"do shell script "rm /usr/local/bin/lapce" with administrator privileges"#)],
                    }
                )
            }
            JumpLocationForward => {
                self.main_split.jump_location_forward(false);
            }
            JumpLocationBackward => {
                self.main_split.jump_location_backward(false);
            }
            JumpLocationForwardLocal => {
                self.main_split.jump_location_forward(true);
            }
            JumpLocationBackwardLocal => {
                self.main_split.jump_location_backward(true);
            }
            Quit => {
                floem::quit_app();
            }
            RevealInPanel => {
                if let Some(editor_data) =
                    self.main_split.active_editor.get_untracked()
                {
                    self.show_panel(PanelKind::FileExplorer);
                    self.panel
                        .section_open(PanelSection::FileExplorer).update(|x| {
                        *x = true;
                    });
                    if let DocContent::File {path, ..} = editor_data.doc().content.get_untracked() {
                        self.file_explorer.reveal_in_file_tree(path);
                    }
                }
            }
            SourceControlOpenActiveFileRemoteUrl => {
                if let Some(editor_data) =
                    self.main_split.active_editor.get_untracked()
                {
                    if let DocContent::File {path, ..} = editor_data.doc().content.get_untracked() {
                        let offset = editor_data.cursor().with_untracked(|c| c.offset());
                        let line = editor_data.doc()
                            .buffer
                            .with_untracked(|buffer| buffer.line_of_offset(offset));
                        let window_tab = self.clone();
                        self.common.proxy.git_get_remote_file_url(
                            path,
                            create_ext_action(
                                self.scope,
                                move |result: Result<ProxyResponse, RpcError>| {
                                match result {
                                    Ok(ProxyResponse::GitGetRemoteFileUrl {
                                        file_url,
                                    }) => {
                                        if let Err(err) =
                                            open::that(format!("{}#L{}", file_url, line))
                                        {
                                            error!(
                                                "Failed to open file in github: {}",
                                                err
                                            );
                                        }
                                    }
                                    Ok(_) => {}
                                    Err(err) => window_tab.show_message(
                                        "Open Remote File Url failure",
                                        &ShowMessageParams {
                                            typ: MessageType::ERROR,
                                            message: err.message,
                                        },
                                    ),
                                }
                            }),
                        );

                    }
                }
            }
            RevealInFileExplorer => {
                if let Some(editor_data) =
                    self.main_split.active_editor.get_untracked()
                {
                    if let DocContent::File {path, ..} = editor_data.doc().content.get_untracked() {
                        let path = path.parent().unwrap_or(&path);
                        if !path.exists() {
                            return;
                        }
                        if let Err(err) = open::that(path) {
                            error!(
                            "Failed to reveal file in system file explorer: {}",
                            err
                        );
                        }
                    }
                }
            }
            GoToLocation => {
                if let Some(editor_data) =
                    self.main_split.active_editor.get_untracked()
                {
                    let doc = editor_data.doc();
                    let path = match if doc.loaded() {
                        doc.content.with_untracked(|c| c.path().cloned())
                    } else {
                        None
                    } {
                        Some(path) => path,
                        None => return,
                    };
                    let offset = editor_data.cursor().with_untracked(|c| c.offset());
                    let internal_command = self.common.internal_command;

                    internal_command.send(InternalCommand::MakeConfirmed);
                    internal_command.send(InternalCommand::GoToLocation { location: EditorLocation {
                        path,
                        position: Some(EditorPosition::Offset(offset)),
                        scroll_offset: None,
                        ignore_unconfirmed: false,
                        same_editor_tab: false,
                    } });
                }
            }
        }
    }

    pub fn run_internal_command(&self, cmd: InternalCommand) {
        match cmd {
            InternalCommand::ReloadConfig => {
                self.reload_config();
            }
            InternalCommand::MakeConfirmed => {
                if let Some(editor) = self.main_split.active_editor.get_untracked() {
                    editor.confirmed.set(true);
                }
            }
            InternalCommand::OpenFile { path } => {
                self.main_split.jump_to_location(EditorLocation {
                    path,
                    position: None,
                    scroll_offset: None,
                    ignore_unconfirmed: false,
                    same_editor_tab: false,
                });
            }
            InternalCommand::OpenAndConfirmedFile { path } => {
                self.main_split.jump_to_location(EditorLocation {
                    path,
                    position: None,
                    scroll_offset: None,
                    ignore_unconfirmed: false,
                    same_editor_tab: false,
                });
                if let Some(editor) = self.main_split.active_editor.get_untracked() {
                    editor.confirmed.set(true);
                }
            }
            InternalCommand::OpenFileInNewTab { path } => {
                self.main_split.jump_to_location(EditorLocation {
                    path,
                    position: None,
                    scroll_offset: None,
                    ignore_unconfirmed: true,
                    same_editor_tab: false,
                });
            }
            InternalCommand::OpenFileChanges { path } => {
                self.main_split.open_file_changes(path);
            }
            InternalCommand::ReloadFileExplorer => {
                self.file_explorer.reload();
            }
            InternalCommand::TestPathCreation { new_path } => {
                let naming = self.file_explorer.naming;

                let send = create_ext_action(
                    self.scope,
                    move |response: Result<ProxyResponse, RpcError>| match response {
                        Ok(_) => {
                            naming.update(Naming::set_ok);
                        }
                        Err(err) => {
                            naming.update(|naming| naming.set_err(err.message));
                        }
                    },
                );

                self.common.proxy.test_create_at_path(new_path, send);
            }
            InternalCommand::FinishRenamePath {
                current_path,
                new_path,
            } => {
                let send_current_path = current_path.clone();
                let send_new_path = new_path.clone();
                let file_explorer = self.file_explorer.clone();
                let editors = self.main_split.editors;

                let send = create_ext_action(
                    self.scope,
                    move |response: Result<ProxyResponse, RpcError>| match response {
                        Ok(response) => {
                            // Get the canonicalized new path from the proxy.
                            let new_path =
                                if let ProxyResponse::CreatePathResponse { path } =
                                    response
                                {
                                    path
                                } else {
                                    send_new_path
                                };

                            // If the renamed item is a file, update any editors the file is open
                            // in to use the new path.
                            // If the renamed item is a directory, update any editors in which a
                            // file the renamed directory is an ancestor of is open to use the
                            // file's new path.
                            let renamed_editors_content: Vec<_> = editors
                                .with_editors_untracked(|editors| {
                                    editors
                                        .values()
                                        .map(|editor| editor.doc().content)
                                        .filter(|content| {
                                            content.with_untracked(|content| {
                                                match content {
                                                    DocContent::File {
                                                        path,
                                                        ..
                                                    } => path.starts_with(
                                                        &send_current_path,
                                                    ),
                                                    _ => false,
                                                }
                                            })
                                        })
                                        .collect()
                                });

                            for content in renamed_editors_content {
                                content.update(|content| {
                                    if let DocContent::File { path, .. } = content {
                                        if let Ok(suffix) =
                                            path.strip_prefix(&send_current_path)
                                        {
                                            *path = new_path.join(suffix);
                                        }
                                    }
                                });
                            }

                            file_explorer.reload();
                            file_explorer.naming.set(Naming::None);
                        }
                        Err(err) => {
                            file_explorer
                                .naming
                                .update(|naming| naming.set_err(err.message));
                        }
                    },
                );

                self.file_explorer.naming.update(Naming::set_pending);
                self.common
                    .proxy
                    .rename_path(current_path.clone(), new_path, send);
            }
            InternalCommand::FinishNewNode { is_dir, path } => {
                let file_explorer = self.file_explorer.clone();
                let internal_command = self.common.internal_command;

                let send = create_ext_action(
                    self.scope,
                    move |response: Result<ProxyResponse, RpcError>| {
                        match response {
                            Ok(response) => {
                                file_explorer.reload();
                                file_explorer.naming.set(Naming::None);

                                // Open a new file in the editor
                                if let ProxyResponse::CreatePathResponse { path } =
                                    response
                                {
                                    if !is_dir {
                                        internal_command.send(
                                            InternalCommand::OpenFile { path },
                                        );
                                    }
                                }
                            }
                            Err(err) => {
                                file_explorer
                                    .naming
                                    .update(|naming| naming.set_err(err.message));
                            }
                        }
                    },
                );

                self.file_explorer.naming.update(Naming::set_pending);
                if is_dir {
                    self.common.proxy.create_directory(path, send);
                } else {
                    self.common.proxy.create_file(path, send);
                }
            }
            InternalCommand::FinishDuplicate { source, path } => {
                let file_explorer = self.file_explorer.clone();

                let send = create_ext_action(
                    self.scope,
                    move |response: Result<_, RpcError>| {
                        if let Err(err) = response {
                            file_explorer
                                .naming
                                .update(|naming| naming.set_err(err.message));
                        } else {
                            file_explorer.reload();
                            file_explorer.naming.set(Naming::None);
                        }
                    },
                );

                self.file_explorer.naming.update(Naming::set_pending);
                self.common.proxy.duplicate_path(source, path, send);
            }
            InternalCommand::GoToLocation { location } => {
                self.main_split.go_to_location(location);
            }
            InternalCommand::JumpToLocation { location } => {
                self.main_split.jump_to_location(location);
            }
            InternalCommand::Split {
                direction,
                editor_tab_id,
            } => {
                self.main_split.split(direction, editor_tab_id);
            }
            InternalCommand::SplitMove {
                direction,
                editor_tab_id,
            } => {
                self.main_split.split_move(direction, editor_tab_id);
            }
            InternalCommand::SplitExchange { editor_tab_id } => {
                self.main_split.split_exchange(editor_tab_id);
            }
            InternalCommand::EditorTabClose { editor_tab_id } => {
                self.main_split.editor_tab_close(editor_tab_id);
            }
            InternalCommand::EditorTabChildClose {
                editor_tab_id,
                child,
            } => {
                self.main_split
                    .editor_tab_child_close(editor_tab_id, child, false);
            }
            InternalCommand::EditorTabCloseByKind {
                editor_tab_id,
                child,
                kind,
            } => {
                self.main_split.editor_tab_child_close_by_kind(
                    editor_tab_id,
                    child,
                    kind,
                );
            }
            InternalCommand::SaveJumpLocation {
                path,
                offset,
                scroll_offset,
            } => {
                self.main_split
                    .save_jump_location(path, offset, scroll_offset);
            }
            InternalCommand::NewTerminal { profile } => {
                self.terminal.new_tab(profile);
            }
            InternalCommand::SplitTerminal { term_id } => {
                self.terminal.split(term_id);
            }
            InternalCommand::SplitTerminalNext { term_id } => {
                self.terminal.split_next(term_id);
            }
            InternalCommand::SplitTerminalPrevious { term_id } => {
                self.terminal.split_previous(term_id);
            }
            InternalCommand::SplitTerminalExchange { term_id } => {
                self.terminal.split_exchange(term_id);
            }
            InternalCommand::Search { pattern } => {
                self.main_split.set_find_pattern(pattern);
            }
            InternalCommand::FindEditorReceiveChar { s } => {
                self.main_split.find_editor.receive_char(&s);
            }
            InternalCommand::ReplaceEditorReceiveChar { s } => {
                self.main_split.replace_editor.receive_char(&s);
            }
            InternalCommand::FindEditorCommand {
                command,
                count,
                mods,
            } => {
                self.main_split
                    .find_editor
                    .run_command(&command, count, mods);
            }
            InternalCommand::ReplaceEditorCommand {
                command,
                count,
                mods,
            } => {
                self.main_split
                    .replace_editor
                    .run_command(&command, count, mods);
            }
            InternalCommand::FocusEditorTab { editor_tab_id } => {
                self.main_split.active_editor_tab.set(Some(editor_tab_id));
            }
            InternalCommand::SetColorTheme { name, save } => {
                if save {
                    // The config file is watched
                    LapceConfig::update_file(
                        "core",
                        "color-theme",
                        toml_edit::Value::from(name),
                    );
                } else {
                    let mut new_config = self.common.config.get_untracked();
                    Arc::make_mut(&mut new_config)
                        .set_color_theme(&self.workspace, &name);
                    self.set_config.set(new_config);
                }
            }
            InternalCommand::SetIconTheme { name, save } => {
                if save {
                    // The config file is watched
                    LapceConfig::update_file(
                        "core",
                        "icon-theme",
                        toml_edit::Value::from(name),
                    );
                } else {
                    let mut new_config = self.common.config.get_untracked();
                    Arc::make_mut(&mut new_config)
                        .set_icon_theme(&self.workspace, &name);
                    self.set_config.set(new_config);
                }
            }
            InternalCommand::SetModal { modal } => {
                LapceConfig::update_file(
                    "core",
                    "modal",
                    toml_edit::Value::from(modal),
                );
            }
            InternalCommand::OpenWebUri { uri } => {
                if !uri.is_empty() {
                    match open::that(&uri) {
                        Ok(_) => {
                            trace!(TraceLevel::TRACE, "opened web uri: {uri:?}");
                        }
                        Err(e) => {
                            trace!(
                                TraceLevel::ERROR,
                                "failed to open web uri: {uri:?}, error: {e}"
                            );
                        }
                    }
                }
            }
            InternalCommand::ShowAlert {
                title,
                msg,
                buttons,
            } => {
                self.show_alert(title, msg, buttons);
            }
            InternalCommand::HideAlert => {
                self.alert_data.active.set(false);
            }
            InternalCommand::SaveScratchDoc { doc } => {
                self.main_split.save_scratch_doc(doc);
            }
            InternalCommand::SaveScratchDoc2 { doc } => {
                self.main_split.save_scratch_doc2(doc);
            }
            InternalCommand::ResetBlinkCursor => {
                // All the editors share the blinking information and logic, so we can just reset
                // one of them.
                if let Some(e_data) = self.main_split.active_editor.get_untracked() {
                    e_data.editor.cursor_info.reset();
                }
            }
            InternalCommand::OpenDiffFiles {
                left_path,
                right_path,
            } => self.main_split.open_diff_files(left_path, right_path),
            InternalCommand::ExecuteProcess { program, arguments } => {
                let mut cmd = match std::process::Command::new(program)
                    .args(arguments)
                    .spawn()
                {
                    Ok(v) => v,
                    Err(e) => {
                        return event!(Level::ERROR, "Failed to spawn process: {e}");
                    }
                };

                match cmd.wait() {
                    Ok(v) => event!(Level::TRACE, "Process exited with status {v}"),
                    Err(e) => {
                        event!(Level::ERROR, "Proces exited with an error: {e}")
                    }
                };
            }
            InternalCommand::ClearTerminalBuffer {
                view_id,
                tab_index,
                terminal_index,
            } => {
                let Some(tab) = self.terminal.tab_info.with_untracked(|x| {
                    x.tabs.iter().find_map(|(index, data)| {
                        if index.get_untracked() == tab_index {
                            Some(data.clone())
                        } else {
                            None
                        }
                    })
                }) else {
                    error!("cound not find terminal tab data: index={tab_index}");
                    return;
                };
                let Some(raw) = tab.terminals.with_untracked(|x| {
                    x.iter().find_map(|(index, data)| {
                        if index.get_untracked() == terminal_index {
                            Some(data.raw.get_untracked())
                        } else {
                            None
                        }
                    })
                }) else {
                    error!("cound not find terminal data: index={terminal_index}");
                    return;
                };
                raw.write().term.reset_state();
                view_id.request_paint();
            }
        }
    }

    fn handle_core_notification(&self, rpc: &CoreNotification) {
        match rpc {
            CoreNotification::DiffInfo { diff } => {
                self.source_control.branch.set(diff.head.clone());
                self.source_control
                    .branches
                    .set(diff.branches.iter().cloned().collect());
                self.source_control
                    .tags
                    .set(diff.tags.iter().cloned().collect());
                self.source_control.file_diffs.update(|file_diffs| {
                    *file_diffs = diff
                        .diffs
                        .iter()
                        .cloned()
                        .map(|diff| {
                            let checked =
                                file_diffs.get(diff.path()).is_none_or(|(_, c)| *c);
                            (diff.path().clone(), (diff, checked))
                        })
                        .collect();
                });

                let docs = self.main_split.docs.get_untracked();
                for (_, doc) in docs {
                    doc.retrieve_head();
                }
            }
            CoreNotification::TerminalProcessStopped { term_id, exit_code } => {
                debug!("TerminalProcessStopped {:?}, {:?}", term_id, exit_code);
                if let Err(err) = self
                    .common
                    .term_tx
                    .send((*term_id, TermEvent::CloseTerminal))
                {
                    tracing::error!("{:?}", err);
                }
                self.terminal.terminal_stopped(term_id, *exit_code);
                if self
                    .terminal
                    .tab_info
                    .with_untracked(|info| info.tabs.is_empty())
                {
                    if self.panel.is_panel_visible(&PanelKind::Terminal) {
                        self.panel.hide_panel(&PanelKind::Terminal);
                    }
                    self.common.focus.set(Focus::Workbench);
                }
            }
            CoreNotification::TerminalLaunchFailed { term_id, error } => {
                self.terminal.launch_failed(term_id, error);
            }
            CoreNotification::OpenPaths { paths } => {
                self.open_paths(paths);
            }
            CoreNotification::OpenFileChanged { path, content } => {
                self.main_split.open_file_changed(path, content);
            }
            CoreNotification::ShowMessage { title, message } => {
                self.show_message(title, message);
            }
            CoreNotification::WorkspaceFileChange => {
                self.file_explorer.reload();
            }
            _ => {}
        }
    }

    pub fn key_down<'a>(&self, event: impl Into<EventRef<'a>> + Copy) -> bool {
        if self.alert_data.active.get_untracked() {
            // The alert swallows all keystrokes; Escape dismisses it.
            if let EventRef::Keyboard(key_event) = event.into() {
                if key_event.key.logical_key == Key::Named(NamedKey::Escape) {
                    self.alert_data.active.set(false);
                    return true;
                }
            }
            return false;
        }
        let focus = self.common.focus.get_untracked();
        let keypress = self.common.keypress.get_untracked();
        let handle = match focus {
            Focus::Workbench => self.main_split.key_down(event, &keypress),
            Focus::Palette => Some(keypress.key_down(event, &self.palette)),
            Focus::AboutPopup => Some(keypress.key_down(event, &self.about_data)),
            Focus::Panel(PanelKind::Terminal) => {
                self.terminal.key_down(event, &keypress)
            }
            Focus::Panel(PanelKind::Search) => {
                Some(keypress.key_down(event, &self.global_search))
            }
            Focus::Panel(PanelKind::SourceControl) => {
                Some(keypress.key_down(event, &self.source_control))
            }
            _ => None,
        };

        if let Some(handle) = &handle {
            if handle.handled {
                true
            } else {
                keypress
                    .handle_keymatch(
                        self,
                        handle.keymatch.clone(),
                        handle.keypress.clone(),
                    )
                    .handled
            }
        } else {
            keypress.key_down(event, self).handled
        }
    }

    pub fn workspace_info(&self) -> WorkspaceInfo {
        let main_split_data = self
            .main_split
            .splits
            .get_untracked()
            .get(&self.main_split.root_split)
            .cloned()
            .unwrap();
        WorkspaceInfo {
            split: main_split_data.get_untracked().split_info(self),
            panel: self.panel.panel_info(),
        }
    }

    /// Get the mode for the current editor or terminal
    pub fn mode(&self) -> Mode {
        if self.common.config.get().core.modal {
            let mode = if self.common.focus.get() == Focus::Workbench {
                self.main_split
                    .active_editor
                    .get()
                    .map(|editor| editor.cursor().with(|c| c.get_mode()))
            } else {
                None
            };

            mode.unwrap_or(Mode::Normal)
        } else {
            Mode::Insert
        }
    }

    pub fn toggle_panel_visual(&self, kind: PanelKind) {
        if self.panel.is_panel_visible(&kind) {
            self.hide_panel(kind);
        } else {
            self.show_panel(kind);
        }
    }

    /// Toggle a specific kind of panel.
    fn toggle_panel_focus(&self, kind: PanelKind) {
        let should_hide = match kind {
            PanelKind::FileExplorer => {
                // Some panels don't accept focus (yet). Fall back to visibility check
                // in those cases.
                self.panel.is_panel_visible(&kind)
            }
            PanelKind::Terminal | PanelKind::SourceControl | PanelKind::Search => {
                self.is_panel_focused(kind)
            }
        };
        if should_hide {
            self.hide_panel(kind);
        } else {
            self.show_panel(kind);
        }
    }

    /// Toggle a panel on one of the sides.
    fn toggle_container_visual(&self, position: &PanelContainerPosition) {
        let shown = !self.panel.is_container_shown(position, false);
        self.panel.set_shown(&position.first(), shown);
        self.panel.set_shown(&position.second(), shown);

        if shown {
            if let Some((kind, _)) = self
                .panel
                .active_panel_at_position(&position.second(), false)
            {
                self.show_panel(kind);
            }

            if let Some((kind, _)) = self
                .panel
                .active_panel_at_position(&position.first(), false)
            {
                self.show_panel(kind);
            }
        } else {
            if let Some((kind, _)) = self
                .panel
                .active_panel_at_position(&position.second(), false)
            {
                self.hide_panel(kind);
            }

            if let Some((kind, _)) = self
                .panel
                .active_panel_at_position(&position.first(), false)
            {
                self.hide_panel(kind);
            }
        }
    }

    fn is_panel_focused(&self, kind: PanelKind) -> bool {
        // Moving between e.g. Search and Problems doesn't affect focus, so we need to also check
        // visibility.
        self.common.focus.get_untracked() == Focus::Panel(kind)
            && self.panel.is_panel_visible(&kind)
    }

    fn hide_panel(&self, kind: PanelKind) {
        self.panel.hide_panel(&kind);
        self.common.focus.set(Focus::Workbench);
    }

    pub fn show_panel(&self, kind: PanelKind) {
        if kind == PanelKind::Terminal
            && self
                .terminal
                .tab_info
                .with_untracked(|info| info.tabs.is_empty())
        {
            self.terminal.new_tab(
                self.common
                    .config
                    .get_untracked()
                    .terminal
                    .get_default_profile(),
            );
        }
        self.panel.show_panel(&kind);
        if kind == PanelKind::Search
            && self.common.focus.get_untracked() == Focus::Workbench
        {
            let active_editor = self.main_split.active_editor.get_untracked();
            let word = active_editor.map(|editor| editor.word_at_cursor());
            if let Some(word) = word {
                if !word.is_empty() {
                    self.global_search.set_pattern(word);
                }
            }
        }
        self.common.focus.set(Focus::Panel(kind));
    }

    pub fn open_paths(&self, paths: &[PathObject]) {
        let (folders, files): (Vec<&PathObject>, Vec<&PathObject>) =
            paths.iter().partition(|p| p.is_dir);

        for folder in folders {
            self.common.window_common.window_command.send(
                WindowCommand::NewWorkspaceTab {
                    workspace: LapceWorkspace {
                        kind: self.workspace.kind.clone(),
                        path: Some(folder.path.clone()),
                        last_open: 0,
                    },
                    end: false,
                },
            );
        }

        for file in files {
            let position = file.linecol.map(|pos| {
                EditorPosition::Position(lsp_types::Position {
                    line: pos.line.saturating_sub(1) as u32,
                    character: pos.column.saturating_sub(1) as u32,
                })
            });

            self.common
                .internal_command
                .send(InternalCommand::GoToLocation {
                    location: EditorLocation {
                        path: file.path.clone(),
                        position,
                        scroll_offset: None,
                        // Create a new editor for the file, so we don't change any current unconfirmed
                        // editor
                        ignore_unconfirmed: true,
                        same_editor_tab: false,
                    },
                });
        }
    }

    pub fn show_alert(&self, title: String, msg: String, buttons: Vec<AlertButton>) {
        self.alert_data.title.set(title);
        self.alert_data.msg.set(msg);
        self.alert_data.buttons.set(buttons);
        self.alert_data.active.set(true);
    }

    fn show_message(&self, title: &str, message: &ShowMessageParams) {
        self.messages.update(|messages| {
            messages.push((title.to_string(), message.clone()));
        });
    }
}

/// Open path with the default application without blocking.
fn open_uri(path: &Path) {
    match open::that(path) {
        Ok(_) => {
            debug!("opened active file: {path:?}");
        }
        Err(e) => {
            error!("failed to open active file: {path:?}, error: {e}");
        }
    }
}
