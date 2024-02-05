use std::path::PathBuf;

use process::MyArgs;
use relm4::{
    gtk::{
        self,
        prelude::{BoxExt, GtkWindowExt, OrientableExt},
        Adjustment, FileFilter,
    },
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent,
};
use relm4_components::{
    open_button::{OpenButton, OpenButtonSettings},
    open_dialog::OpenDialogSettings,
};

pub mod process;

struct AppModel {
    open_game: Controller<OpenButton>,
    game_path: PathBuf,
    open_srt: Controller<OpenButton>,
    srt_path: PathBuf,
    open_audio: Controller<OpenButton>,
    audio_path: PathBuf,
    open_epub: Controller<OpenButton>,
    epub_path: PathBuf,
}

#[derive(Debug)]
enum DialogOrigin {
    Audio,
    Game,
    Srt,
    Epub,
}

#[derive(Debug)]
enum AppInMsg {
    Recheck,
    Open(PathBuf, DialogOrigin),
}

#[relm4::component]
impl SimpleComponent for AppModel {
    type Input = AppInMsg;

    type Output = ();
    type Init = u8;

    // Initialize the UI.
    fn init(
        _: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let open_game = OpenButton::builder()
            .launch(OpenButtonSettings {
                dialog_settings: OpenDialogSettings {
                    folder_mode: true,
                    cancel_label: String::from("Cancel"),
                    accept_label: String::from("Select"),
                    create_folders: true,
                    is_modal: true,
                    ..OpenDialogSettings::default()
                },
                text: "Open folder",
                recently_opened_files: None,
                max_recent_files: 0,
            })
            .forward(sender.input_sender(), |path| {
                AppInMsg::Open(path, DialogOrigin::Game)
            });

        let srt_filter = FileFilter::new();
        srt_filter.add_pattern("*.srt");
        srt_filter.set_name(Some("Subtitle files (.srt)"));

        let open_srt = OpenButton::builder()
            .launch(OpenButtonSettings {
                dialog_settings: OpenDialogSettings {
                    folder_mode: false,
                    cancel_label: String::from("Cancel"),
                    accept_label: String::from("Select"),
                    create_folders: true,
                    is_modal: true,
                    // filter:
                    filters: vec![srt_filter],
                },
                text: "Open file",
                recently_opened_files: None,
                max_recent_files: 0,
            })
            .forward(sender.input_sender(), |path| {
                AppInMsg::Open(path, DialogOrigin::Srt)
            });

        let audio_filter = FileFilter::new();
        audio_filter.add_pattern("*.mp3");
        audio_filter.add_pattern("*.ogg");
        audio_filter.add_pattern("*.m4b");
        audio_filter.add_pattern("*.m4a");
        audio_filter.set_name(Some("Audio files (.mp3, .m4b, .m4a)"));

        let open_audio = OpenButton::builder()
            .launch(OpenButtonSettings {
                dialog_settings: OpenDialogSettings {
                    folder_mode: false,
                    cancel_label: String::from("Cancel"),
                    accept_label: String::from("Select"),
                    create_folders: true,
                    is_modal: true,
                    // filter:
                    filters: vec![audio_filter],
                },
                text: "Open file",
                recently_opened_files: None,
                max_recent_files: 0,
            })
            .forward(sender.input_sender(), |path| {
                AppInMsg::Open(path, DialogOrigin::Audio)
            });

        let epub_filter = FileFilter::new();
        epub_filter.add_pattern("*.epub");
        epub_filter.set_name(Some("epub files"));

        let open_epub = OpenButton::builder()
            .launch(OpenButtonSettings {
                dialog_settings: OpenDialogSettings {
                    folder_mode: false,
                    cancel_label: String::from("Cancel"),
                    accept_label: String::from("Select"),
                    create_folders: true,
                    is_modal: true,
                    // filter:
                    filters: vec![epub_filter],
                },
                text: "Open file",
                recently_opened_files: None,
                max_recent_files: 0,
            })
            .forward(sender.input_sender(), |path| {
                AppInMsg::Open(path, DialogOrigin::Epub)
            });

        let model = AppModel {
            open_game,
            open_srt,
            open_audio,
            open_epub,
            game_path: PathBuf::from(""),
            srt_path: PathBuf::from(""),
            audio_path: PathBuf::from(""),
            epub_path: PathBuf::from(""),
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppInMsg::Recheck => todo!(),
            AppInMsg::Open(path, origin) => match origin {
                DialogOrigin::Audio => self.audio_path = path,
                DialogOrigin::Game => self.game_path = path,
                DialogOrigin::Srt => self.srt_path = path,
                DialogOrigin::Epub => self.epub_path = path,
            },
        }
    }

    view! {
        gtk::Window {
            set_title: Some("Audiobook to Ren'py"),
            set_default_width: 600,
            set_default_height: 400,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 5,
                set_margin_all: 5,

                gtk::Box {
                    set_spacing: 5,
                    set_margin_all: 5,
                    set_orientation: gtk::Orientation::Horizontal,
                    gtk::Label {
                        set_label: "Folder to install game in"

                    },
                    append = model.open_game.widget(),
                    gtk::Label {
                        #[watch]
                        set_label: &model.game_path.to_string_lossy()
                    }
                },

                gtk::Box {
                    set_spacing: 5,
                    set_margin_all: 5,
                    set_orientation: gtk::Orientation::Horizontal,
                    gtk::Label {
                        set_label: "Path to the .srt file"

                    },
                    append = model.open_srt.widget(),
                    gtk::Label {
                        #[watch]
                        set_label: &model.srt_path.to_string_lossy()
                    }
                },
                gtk::Box {
                    set_spacing: 5,
                    set_margin_all: 5,
                    set_orientation: gtk::Orientation::Horizontal,
                    gtk::Label {
                        set_label: "Path to the audio file"
                    },
                    append = model.open_audio.widget(),
                    gtk::Label {
                        #[watch]
                        set_label: &model.audio_path.to_string_lossy()
                    }
                },

                gtk::Box {
                    set_spacing: 5,
                    set_margin_all: 5,
                    set_orientation: gtk::Orientation::Horizontal,
                    gtk::Label {
                        set_label: "Path to the epub file (optional)"
                    },
                    append = model.open_epub.widget(),
                    gtk::Label {
                        #[watch]
                        set_label: &model.epub_path.to_string_lossy()
                    }
                },

                gtk::Box {
                    set_spacing: 5,
                    set_margin_all: 5,
                    set_orientation: gtk::Orientation::Horizontal,
                    gtk::Label {
                        set_label: "Offsets"
                    },
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    relm4::gtk::SpinButton::builder()
                    .adjustment(&Adjustment::new(0.0, -500.0, 500.0, 1.0, 0.0, 0.0))
                    .build(){
                    },
                        gtk::Label {
                            set_label: "Before (ms)"
                        }
                    },
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    relm4::gtk::SpinButton::builder()
                    .adjustment(&Adjustment::new(0.0, -500.0, 500.0, 1.0, 0.0, 0.0))
                    .build(){
                    },
                        gtk::Label {
                            set_label: "After (ms)"
                        }
                    },

                    // append = spin_offset_before,
                },

                //   append = if model.show_button {
                // gtk::Button::with_label("Generate Deck !") {
                //     connect_clicked[sender] => move |_| {
                //         sender.input(AppInMsg::Start);
                //     }
                // }}
                // else if model.show_indicator {
                //     gtk::Spinner {
                //         set_spinning: true,
                //     }
                // }
                // else {
                //     gtk::Label{
                //         set_label: "Please fill all three fields"
                //     }
                // },

            }
        }
    }
}

fn main() {
    // rayon::ThreadPoolBuilder::new()
    //     .num_threads(4)
    //     .build_global()
    //     .unwrap();
    let args = MyArgs {
        game_folder: String::from("test/game"),
        audiobook: String::from("opuss.ogg"),
        subtitle: String::from("p4v3.srt"),
        epub: None,
        split: true,
        show_buggies: false,
        start_offset: 0,
        end_offset: 0,
    };
    process::process(args);
    // let app = RelmApp::new("relm4.test.simple");
    // app.run::<AppModel>(0);
}
