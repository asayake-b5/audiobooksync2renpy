extern crate relm4;

use dircpy::copy_dir;
use std::{convert::identity, env, path::PathBuf};

use relm4::{
    gtk::{
        self,
        prelude::{
            BoxExt, ButtonExt, EditableExt, EntryBufferExtManual, EntryExt, GtkWindowExt,
            OrientableExt, TextBufferExt, TextViewExt, WidgetExt,
        },
        Adjustment, EntryBuffer, FileFilter,
    },
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmApp,
    RelmWidgetExt, SimpleComponent, WorkerController,
};
use relm4_components::{
    open_button::{OpenButton, OpenButtonSettings},
    open_dialog::OpenDialogSettings,
};

use worker::{AsyncHandler, AsyncHandlerInMsg};

use crate::process::MyArgs;

mod epub_process;
mod process;
mod worker;

struct AppModel {
    open_srt: Controller<OpenButton>,
    srt_path: PathBuf,
    open_epub: Controller<OpenButton>,
    epub_path: Option<PathBuf>,
    open_audio: Controller<OpenButton>,
    audio_path: PathBuf,
    audio_ext: Option<AudioExt>,
    prefix: EntryBuffer,
    buffer: gtk::TextBuffer,
    offset_before: f64,
    gain: f64,
    speed: f64,
    show_button: bool,
    sensitive: bool,
    worker: WorkerController<AsyncHandler>,
}

#[derive(Debug, PartialEq, Eq)]
enum AudioExt {
    M4b,
    Mp3,
}

#[derive(Debug)]
pub enum DialogOrigin {
    Audio,
    Srt,
    Epub,
}

#[derive(Debug)]
pub enum OffsetDirection {
    Before,
    After,
}

#[derive(Debug)]
pub enum AppInMsg {
    Ended,
    UpdateBuffer(String, bool),
    Recheck,
    UpdateOffset(f64),
    UpdateGain(f64),
    UpdateSpeed(f64),
    Open(PathBuf, DialogOrigin),
    StartConversion(f64, f64),
    StartAudioSplit,
    Start,
}

#[derive(Debug)]
pub enum AppOutMsg {
    Scroll,
}

#[relm4::component]
impl SimpleComponent for AppModel {
    type Input = AppInMsg;

    type Output = AppOutMsg;
    type Init = u8;

    // Initialize the UI.
    fn init(
        _: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
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

        let epub_filter = FileFilter::new();
        epub_filter.add_pattern("*.epub");
        epub_filter.set_name(Some("Epub Files (.epub)"));

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

        let audio_filter = FileFilter::new();
        audio_filter.add_pattern("*.mp3");
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

        let model = AppModel {
            prefix: EntryBuffer::new(Some("MyAudiobook")),
            open_srt,
            open_audio,
            open_epub,
            audio_ext: None,
            buffer: gtk::TextBuffer::new(None),
            epub_path: None,
            srt_path: PathBuf::from(""),
            audio_path: PathBuf::from(""),
            show_button: false,
            offset_before: 0.0,
            worker: AsyncHandler::builder()
                .detach_worker(())
                .forward(sender.input_sender(), identity),
            sensitive: true,
            gain: 1.0,
            speed: 1.0,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppInMsg::Ended => {
                self.sensitive = true;
            }
            AppInMsg::UpdateBuffer(msg, delete) => {
                if delete {
                    let (mut start, mut end) = self.buffer.bounds();
                    self.buffer.delete(&mut start, &mut end);
                }
                self.buffer.insert_at_cursor(&msg);
            }
            AppInMsg::StartConversion(gain, speed) => {
                self.worker.emit(AsyncHandlerInMsg::ConvertMP3(
                    self.audio_path.clone(),
                    gain,
                    speed,
                ));
            }
            AppInMsg::StartAudioSplit => {
                //TODO fix
                let mut game_folder = env::current_dir().unwrap();
                game_folder.push(self.prefix.text());
                game_folder.push("game");
                copy_dir("template", self.prefix.text()).unwrap();

                let args = MyArgs {
                    epub: self.epub_path.clone(),
                    game_folder,
                    audiobook: self.audio_path.clone(),
                    subtitle: self.srt_path.clone(),
                    start_offset: self.offset_before as i64,
                    speed: self.speed,
                    gain: self.gain,
                    split: true,
                    show_buggies: true,
                };
                self.worker
                    .emit(AsyncHandlerInMsg::SplitAudio(args, self.audio_path.clone()))
            }
            AppInMsg::UpdateOffset(val) => {
                self.offset_before = val;
            }
            AppInMsg::UpdateGain(val) => {
                self.gain = val;
            }
            AppInMsg::UpdateSpeed(val) => {
                self.speed = val;
            }
            AppInMsg::Recheck => {
                self.show_button = self.prefix.length() > 0
                    && !self.audio_path.as_os_str().is_empty()
                    && !self.srt_path.as_os_str().is_empty();
            }
            AppInMsg::Open(path, origin) => {
                match origin {
                    DialogOrigin::Audio => {
                        if path.extension().unwrap() == "m4b" {
                            self.audio_ext = Some(AudioExt::M4b);
                        } else {
                            self.audio_ext = Some(AudioExt::Mp3);
                        }
                        self.audio_path = path
                    }
                    DialogOrigin::Srt => self.srt_path = path,
                    DialogOrigin::Epub => self.epub_path = Some(path),
                };
                self.show_button = self.prefix.length() > 0
                    && !self.audio_path.as_os_str().is_empty()
                    && !self.srt_path.as_os_str().is_empty();
            }
            AppInMsg::Start => {
                self.sensitive = false;
                if self.audio_ext == Some(AudioExt::M4b) {
                    sender.input(AppInMsg::StartConversion(self.gain, self.speed));
                    // self.worker
                    //     .emit(AsyncHandlerInMsg::ConvertMP3(self.audio_path.clone()));
                } else {
                    sender.input(AppInMsg::StartAudioSplit);
                }
            }
        }
    }

    view! {
        gtk::Window {
            set_title: Some("Audiobook to Renpy"),
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
                    #[watch]
                    set_sensitive: model.sensitive,
                    gtk::Label {
                        set_label: "Name"

                    },
                    gtk::Entry {
                        set_buffer: &model.prefix,
                        connect_changed => AppInMsg::Recheck,

                    },
                },


                gtk::Box {
                    set_spacing: 5,
                    set_margin_all: 5,
                    set_orientation: gtk::Orientation::Horizontal,
                    #[watch]
                    set_sensitive: model.sensitive,
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
                    #[watch]
                    set_sensitive: model.sensitive,
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
                    #[watch]
                    set_sensitive: model.sensitive,
                    gtk::Label {
                        set_label: "Path to the epub file (optional, for furigana and importing images),"
                    },
                    append = model.open_epub.widget(),
                    gtk::Label {
                        #[watch]
                        //TODO improve this iirc there’s a option thing in relm dsl
                        set_label: &model.epub_path.clone().unwrap_or_default().to_string_lossy()
                    }
                },

                gtk::Box {
                    set_spacing: 5,
                    set_margin_all: 5,
                    set_orientation: gtk::Orientation::Horizontal,
                    #[watch]
                    set_sensitive: model.sensitive,
                    gtk::Label {
                        set_label: "Offsets"
                    },
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            relm4::gtk::SpinButton::builder()
                            .adjustment(&Adjustment::new(0.0, -500.0, 500.0, 1.0, 0.0, 0.0))
                            .build(){
                                connect_value_changed[sender] => move |x| {
                                    sender.input(AppInMsg::UpdateOffset(x.value()))
                            }},
                            gtk::Label {
                                    set_label: "Before (ms)"
                                }
                        },
                },

                gtk::Box {
                    set_spacing: 5,
                    set_margin_all: 5,
                    set_orientation: gtk::Orientation::Horizontal,
                    #[watch]
                    set_sensitive: model.sensitive,
                    gtk::Label {
                        set_label: "Audio Adjustments (1.0 = 100%, 1.5 = 150% and so on)"
                    },
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            relm4::gtk::SpinButton::builder()
                            .adjustment(&Adjustment::new(1.0, 0.0, 5.0, 0.1, 0.0, 0.0))
                            .digits(2)
                            .build(){
                                connect_value_changed[sender] => move |x| {
                                    sender.input(AppInMsg::UpdateGain(x.value()))
                            }},
                            gtk::Label {
                                    set_label: "Volume"
                                }
                        },
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            relm4::gtk::SpinButton::builder()
                            .adjustment(&Adjustment::new(1.0, 0.0, 3.0, 0.01, 0.0, 0.0))
                            .digits(2)
                            .build(){
                                connect_value_changed[sender] => move |x| {
                                    sender.input(AppInMsg::UpdateSpeed(x.value()))
                            }},
                            gtk::Label {
                                    set_label: "Speed"
                                }
                        },

                },


                append = if model.show_button {
                    gtk::Button::with_label("Generate Deck !") {
                        #[watch]
                        set_sensitive: model.sensitive,
                        connect_clicked[sender] => move |_| {
                            sender.input(AppInMsg::Start);
                        }
                }} else {
                    gtk::Label{
                        set_label: "Please fill all mandatory fields"
                    }
                },


                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_margin_all: 5,

                    gtk::ScrolledWindow {
                        set_min_content_height: 380,

                        #[wrap(Some)]
                        set_child = &gtk::TextView {
                            set_buffer: Some(&model.buffer),
                            set_editable: false,

                            // #[watch]
                            // set_visible: model.file_name.is_some(),
                        },
                    }},
                // else if model.show_indicator {
                //     gtk::Spinner {
                //         set_spinning: true,
                //     }
                // }

            }
        }
    }
}

// #[tokio::main]
fn main() {
    let count = std::thread::available_parallelism().unwrap().get();
    rayon::ThreadPoolBuilder::new()
        .num_threads(count / 2)
        .build_global()
        .unwrap();
    let app = RelmApp::new("audiobook.renpy");
    app.run::<AppModel>(0);
}
