use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use regex::Regex;
use relm4::{ComponentSender, Worker};

use crate::AppInMsg;
use crate::{
    epub_process,
    process::{process, MyArgs},
};

pub struct AsyncHandler;

#[derive(Debug)]
pub enum AsyncHandlerInMsg {
    ConvertMP3(PathBuf),
    SplitAudio(MyArgs, PathBuf),
}

impl AsyncHandler {
    fn create_command() -> Command {
        if cfg!(unix) {
            Command::new("ffmpeg")
        } else if cfg!(windows) {
            Command::new("ffmpeg.exe")
        } else {
            panic!("Unsupported OS possibly.")
        }
    }

    fn update_buffer(contents: &str, clear: bool, sender: &ComponentSender<Self>) {
        sender
            .output(AppInMsg::UpdateBuffer(contents.to_string(), clear))
            .unwrap();
    }

    fn convert_mp3(
        &self,
        audio_path: PathBuf,
        // audio_ext: Option<crate::AudioExt>,
        sender: &ComponentSender<AsyncHandler>,
    ) {
        let regex = Regex::new(r"size=.* time=(.*?) .* speed=(.*x)").unwrap();
        let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
        // let thread_tx = tx.clone();
        let audio_ext = audio_path.extension().unwrap_or_default();
        //TODO if can be removed probably
        if audio_ext == "m4b" {
            let mut converted_path = audio_path.clone();
            converted_path.set_extension("mp3");
            AsyncHandler::update_buffer(
                "Converting to mp3, this'll take a few minutes...",
                false,
                sender,
            );
            let mut command = AsyncHandler::create_command();
            command.stdout(Stdio::piped()).stderr(Stdio::piped()).args([
                "-stats",
                "-v",
                "quiet",
                "-n", //TODO reeeeeeeeeeemove someday
                // "-y",
                "-i",
                audio_path.as_os_str().to_str().unwrap_or(""),
                "-vn",
                "-acodec",
                "libmp3lame",
                converted_path.as_os_str().to_str().unwrap_or(""),
            ]);
            let mut child = command.spawn().unwrap();
            let mut stderr = child.stderr.take().unwrap();

            thread::spawn(move || loop {
                let mut buf = [0; 80];
                match stderr.read(&mut buf) {
                    Err(err) => {
                        println!("{}] Error reading from stream: {}", line!(), err);
                        break;
                    }
                    Ok(got) => {
                        if got == 0 {
                            tx.send(String::from("STOP")).unwrap();
                            break;
                        } else {
                            let str = String::from_utf8_lossy(&buf);
                            let str = regex.replace_all(&str, "Converting... $1 - $2");
                            let str = str.trim_end_matches('\0');
                            let str = str.trim_end_matches('\r');
                            tx.send(str.to_string()).unwrap();
                        }
                    }
                }
            });

            // let sender2 = sender.clone();
            loop {
                if let Ok(msg) = rx.recv() {
                    if msg == "STOP" {
                        AsyncHandler::update_buffer("Converting Done!", false, sender);
                        break;
                    } else {
                        AsyncHandler::update_buffer(&msg, true, sender);
                    }
                }
            }
        }
    }

    fn split_audio(&self, mut args: MyArgs, path: PathBuf, sender: &ComponentSender<AsyncHandler>) {
        let audio_ext = path.extension().unwrap_or_default();
        // let path =
        if audio_ext == "m4b" {
            let mut converted_path = path.clone();
            converted_path.set_extension("mp3");
            args.audiobook = converted_path;
        }

        let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
        let thread_tx = tx.clone();
        thread::spawn(move || {
            let game_folder = args.game_folder.clone();
            let epub = args.epub.clone();
            process(args, thread_tx.clone());
            if let Some(ep) = epub {
                thread_tx
                    .send(String::from("Trying to insert epub images in the script"))
                    .unwrap();
                let mut epubimager = epub_process::EpubImager::new(ep, game_folder);
                let remaining = epubimager.do_the_epub_thing();
                if remaining > 0 {
                    thread_tx
                        .send(format!(
                            "{remaining} images were not implemented in the script"
                        ))
                        .unwrap();
                }
            }

            thread_tx.send(String::from("STOP")).unwrap();
        });
        loop {
            if let Ok(msg) = rx.recv() {
                if msg == "STOP" {
                    AsyncHandler::update_buffer("Processing done!", false, sender);
                    sender.output(AppInMsg::Ended).unwrap();
                    break;
                } else {
                    AsyncHandler::update_buffer(&msg, true, sender);
                }
            }
        }
    }
}

impl Worker for AsyncHandler {
    type Init = ();
    type Input = AsyncHandlerInMsg;
    type Output = AppInMsg;

    fn init(_init: Self::Init, _sender: ComponentSender<Self>) -> Self {
        Self
    }

    fn update(&mut self, msg: AsyncHandlerInMsg, sender: ComponentSender<Self>) {
        match msg {
            AsyncHandlerInMsg::SplitAudio(args, path) => {
                self.split_audio(args, path, &sender);
            }

            AsyncHandlerInMsg::ConvertMP3(audio_path) => {
                self.convert_mp3(audio_path, &sender);
                sender.output(AppInMsg::StartAudioSplit).unwrap();
            }
        }
    }
}
