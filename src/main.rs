use clap::Parser;
use epub::doc::EpubDoc;
use getch::Getch;
use rayon::prelude::{ParallelBridge, ParallelIterator};
use scraper::{Element, Selector};
use srtlib::{Subtitle, Subtitles, Timestamp};
use std::{
    collections::VecDeque,
    fmt::Write,
    fs::File,
    io::Stdout,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::channel,
        Arc,
    },
};

// TODO watch for panics because of add_millis
fn timestamp_to_str(mut t: Timestamp, offset: i32) -> String {
    t.add_milliseconds(offset);
    let (hours, mins, secs, millis) = t.get();
    let seconds_total: u64 = u64::from(hours) * 3600 + u64::from(mins) * 60 + u64::from(secs);
    format!("{}.{:0>3}", seconds_total, millis)
}
fn subtime_to_renpy(s: &Subtitle) -> String {
    format!(
        "<from {} to {}>",
        timestamp_to_str(s.start_time, -100),
        timestamp_to_str(s.end_time, 100)
    )
}

fn replace_rubies(text: &mut String, rubies: &mut VecDeque<[String; 3]>) {
    if rubies.get(0).is_none() {
        return;
    }
    let mut queue: Vec<[String; 2]> = Vec::with_capacity(5);
    while textdistance::str::overlap(&rubies[0][2], text) > 0.5
        && text.contains(&format!("{}{}", &rubies[0][0], &rubies[0][1]))
    {
        let front = rubies.pop_front().unwrap();
        queue.push([front[0].clone(), front[1].clone()]);
        if rubies.is_empty() {
            break;
        }
    }
    while let Some(replacement) = queue.pop() {
        *text = text.replace(
            &format!("{}{}", replacement[0], replacement[1]),
            &format!(
                "{{rb}}{}{{/rb}}{{rt}}{}{{/rt}}",
                replacement[0], replacement[1]
            ),
        );
    }
}

/// Convert a srt audiobook to a renpy visual novel
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// game folder, not the renpy folder but the folder named "game" inside renpy folder
    #[arg(short, long)]
    game_folder: String,

    /// Audiobook file (mp3)
    #[arg(short, long)]
    audiobook: String,

    /// Subtitle file
    #[arg(short, long)]
    subtitle: String,

    /// Epub (optional, used to parse ruby)
    #[arg(short, long)]
    epub: Option<String>,

    /// Render a splitted audio or not
    #[arg(long, default_value_t = false)]
    split: bool,

    ///Show bugged furigana attempts
    #[arg(long, default_value_t = false)]
    show_buggies: bool,
}
fn main() {
    let args = Args::parse();
    let mut rubies = None;

    let gch = Getch::new();
    let contin = Arc::new(AtomicBool::new(true));
    let contin_thread = contin.clone();
    let contin_ctrlc = contin.clone();

    ctrlc::set_handler(move || {
        println!(
            "Shutting gracefully, please wait a moment for the currently converting files to end."
        );
        contin_ctrlc.store(false, Ordering::Relaxed);
    })
    .expect("Error setting Ctrl-C handler");

    std::thread::spawn(move || loop {
        let a = gch.getch().unwrap();
        if a == 113 {
            println!(
            "Shutting gracefully, please wait a moment for the currently converting files to end."
        );
            contin_thread.store(false, Ordering::Relaxed);
        }
    });

    // while contin.load(Ordering::Relaxed) {
    //     println!("arr");
    // }

    if let Some(input_file) = args.epub {
        let doc = EpubDoc::new(input_file);
        assert!(doc.is_ok());
        let mut doc = doc.unwrap();
        let mut rubies_2: VecDeque<[String; 3]> = VecDeque::with_capacity(1000);

        while doc.go_next() {
            let current = doc.get_current_str();
            match current {
                Some((v, _)) => {
                    let document = scraper::Html::parse_document(&v);
                    let selector_ruby = Selector::parse("ruby").unwrap();
                    for element in document.select(&selector_ruby) {
                        let es = element.text().collect::<Vec<&str>>();
                        let context = element.parent_element().unwrap().text().collect::<String>();

                        if es.len() != 2 {
                            panic!("weirdly formated rubies idk? TODO better handling");
                        }

                        rubies_2.push_back([
                            es[0].to_owned(),
                            es[1].to_owned(),
                            context.to_owned(),
                        ]);
                    }
                }
                None => println!("Not Found\n"),
            }
        }
        rubies = Some(rubies_2);
    }

    let mut subs = Subtitles::parse_from_file(args.subtitle, Some("utf8"))
        .unwrap()
        .to_vec();

    subs.sort();

    std::fs::create_dir_all(&format!("{}/audio", args.game_folder)).unwrap();
    // TODO resume function kinda, maybe shortcut to interrupt

    // Collect all subtitle text into a string.
    let mut subs_strings: Vec<String> = Vec::with_capacity(15000);
    let mut buggies: Vec<[String; 3]> = vec![];
    subs.iter().for_each(|s| {
        subs_strings.push(s.text.to_owned());
    });
    if let Some(mut rubies) = rubies {
        while !rubies.is_empty() {
            subs_strings.iter_mut().for_each(|s| {
                replace_rubies(s, &mut rubies);
            });
            if let Some(ruby_top) = rubies.pop_front() {
                buggies.push(ruby_top);
            }
        }
    }

    let mut res = String::from("");
    let head = std::fs::read_to_string("top.txt").expect("Error reading the script top");
    writeln!(res, "{}", head).unwrap();
    writeln!(res, "label start:").unwrap();
    subs.iter().enumerate().for_each(|(i, s)| {
        if args.split {
            writeln!(res, "    voice \"audiobook-{}.mp3\"", i).unwrap();
        } else {
            writeln!(res, "    voice \"{}audiobook.mp3\"", subtime_to_renpy(s)).unwrap();
        }
        writeln!(res, "    \"{}\"", subs_strings[i]).unwrap();
    });
    writeln!(res, "return").unwrap();

    if !buggies.is_empty() {
        if args.show_buggies {
            println!("{:?}", buggies);
        } else {
            println!(
                "Some({}) ruby failed to be inserted in, use --show-bugies to display them (beware spoilers)",
                buggies.len()
            );
        }
    }

    let mut file = File::create(format!("{}/script.rpy", args.game_folder)).unwrap();
    use std::io::Write;
    file.write_all(res.as_bytes()).unwrap();
    if args.split {
        let n = AtomicUsize::new(0);
        let m = subs.len();
        subs.iter()
            .enumerate()
            .par_bridge()
            .for_each(move |(i, s)| {
                if !contin.load(Ordering::Relaxed) {
                    return;
                }
                if s.start_time == s.end_time {
                    Command::new("ffmpeg")
                        .args([
                            "-f",
                            "lavfi",
                            "-i",
                            "anullsrc=r=44100:cl=mono",
                            "-t",
                            "2",
                            "-q:a",
                            "9",
                            "-acodec",
                            "libmp3lame",
                            &format!("{}/audio/audiobook-{i}.mp3", args.game_folder),
                        ])
                        .spawn()
                        .unwrap();
                } else {
                    Command::new("ffmpeg")
                        .args([
                            "-n",
                            "-i",
                            &args.audiobook.to_string(),
                            "-c",
                            "copy",
                            "-ss",
                            &s.start_time.to_string().replace(',', "."),
                            "-to",
                            &s.end_time.to_string().replace(',', "."),
                            &format!("{}/audio/audiobook-{i}.mp3", args.game_folder),
                        ])
                        .output()
                        .unwrap();
                }
                n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                println!("{n:?}/{m} completed!");
            })
    }
}
