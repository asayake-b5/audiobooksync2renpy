use epub::doc::EpubDoc;
use getch::Getch;
use itertools::Itertools;
use rayon::prelude::{ParallelBridge, ParallelIterator};
use scraper::{Element, Selector};
use srtlib::{Subtitle, Subtitles, Timestamp};
use std::sync::mpsc::Sender;
use std::{
    collections::VecDeque,
    fmt::Write,
    fs::File,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

const CHUNK_SIZE: usize = 25;
const SILENCE_OGG: &[u8] = include_bytes!("../silence.ogg");
const SILENCE_MP3: &[u8] = include_bytes!("../silence.mp3");

fn timestamp_to_str(t: Timestamp) -> String {
    let (hours, mins, secs, millis) = t.get();
    let seconds_total: u64 = u64::from(hours) * 3600 + u64::from(mins) * 60 + u64::from(secs);
    format!("{}.{:0>3}", seconds_total, millis)
}

fn subtime_to_renpy(s: &Subtitle) -> String {
    format!(
        "<from {} to {}>",
        timestamp_to_str(s.start_time),
        timestamp_to_str(s.end_time)
    )
}

fn replace_rubies(text: &mut String, rubies: &mut VecDeque<[String; 3]>) {
    if rubies.front().is_none() {
        return;
    }
    let mut queue: Vec<[String; 2]> = Vec::with_capacity(5);
    while textdistance::str::overlap(&rubies[0][2], text) > 0.5 && text.contains(&rubies[0][0]) {
        let front = rubies.pop_front().unwrap();
        queue.push([front[0].clone(), front[1].clone()]);
        if rubies.is_empty() {
            break;
        }
    }
    while let Some(replacement) = queue.pop() {
        *text = text.replace(
            &replacement[0].to_string(),
            &format!(
                "{{rb}}{}{{/rb}}{{rt}}{}{{/rt}}",
                replacement[0], replacement[1]
            ),
        );
    }
}

fn _replace_rubies_old(text: &mut String, rubies: &mut VecDeque<[String; 3]>) {
    if rubies.front().is_none() {
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

fn prepare_ffmpeg_command(
    start: usize,
    count: usize,
    s: &[Subtitle],
    game_folder: &str,
) -> Vec<String> {
    let mut r = Vec::with_capacity(count * 10);
    println!("{}", s.len());
    //TODO off by one here?
    for i in 0..count {
        let n = start + i;
        let path_str = format!("{}/audio/audiobook-{n}.mp3", game_folder);
        let path = PathBuf::from(&path_str);
        if path.exists() {
            continue;
        }
        if s[i].start_time >= s[i].end_time {
            std::fs::write(&path, SILENCE_MP3).unwrap();
            continue;
        }
        r.extend(
            [
                "-c",
                "copy",
                "-ss",
                &s[i].start_time.to_string().replace(',', "."),
                "-to",
                &s[i].end_time.to_string().replace(',', "."),
                &path_str,
            ]
            .map(|s| s.to_string()),
        )
    }
    r
}

#[derive(Debug)]
pub struct MyArgs {
    pub game_folder: PathBuf,
    pub audiobook: PathBuf,
    pub subtitle: PathBuf,
    pub epub: Option<PathBuf>,
    pub split: bool,
    pub show_buggies: bool,
    pub start_offset: i32,
    pub end_offset: i32,
}

pub fn process(args: MyArgs, thread_tx: Sender<String>) {
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
                            dbg!(&es);
                            println!("weirdly formated rubies idk? TODO better handling");
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
    let mintime = Timestamp::new(0, 0, 0, args.start_offset.unsigned_abs() as u16);

    std::fs::create_dir_all(format!("{}/audio", args.game_folder.display())).unwrap();

    // Collect all subtitle text into a string.
    let mut subs_strings: Vec<String> = Vec::with_capacity(15000);
    let mut buggies: Vec<[String; 3]> = vec![];
    let mut subs2: Vec<Subtitle> = Vec::with_capacity(20000);
    subs.iter().tuple_windows().for_each(|(n, np1)| {
        let mut n2 = n.clone();
        if n.start_time > mintime {
            n2.start_time.add_milliseconds(args.start_offset);
        }
        n2.end_time = np1.start_time;
        n2.end_time.add_milliseconds(args.start_offset);
        subs2.push(n2);
        subs_strings.push(n.text.to_owned());
    });

    subs2.push(subs.last().unwrap().clone());
    subs_strings.push(subs.last().unwrap().text.to_owned());

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
    subs2.iter().enumerate().for_each(|(i, s)| {
        if i % 10 == 0 {
            writeln!(res, "    $renpy.force_autosave()").unwrap();
        }
        if args.split {
            writeln!(res, "    voice \"audiobook-{}.mp3\"", i).unwrap();
        } else {
            writeln!(
                res,
                "    voice \"{}{}\"",
                subtime_to_renpy(s),
                &args.audiobook.display()
            )
            .unwrap();
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

    let mut file = File::create(format!("{}/script.rpy", args.game_folder.display())).unwrap();
    use std::io::Write;
    file.write_all(res.as_bytes()).unwrap();
    if args.split {
        let n = AtomicUsize::new(0);
        let m = subs.len();
        subs2
            .chunks(CHUNK_SIZE)
            .enumerate()
            .par_bridge()
            // .par_chunks()
            .for_each(move |(i, s)| {
                let size = s.len();
                let prepared =
                    prepare_ffmpeg_command(i * size, size, s, &args.game_folder.to_string_lossy()); //TODO
                if !contin.load(Ordering::Relaxed) {
                    return;
                }
                let mut command = if cfg!(unix) {
                    Command::new("ffmpeg")
                } else if cfg!(windows) {
                    Command::new("cmd")
                } else {
                    panic!("Unsupported OS possibly.")
                };
                let args: Vec<String> = if cfg!(unix) {
                    [
                        "-hide_banner".to_string(),
                        "-loglevel".to_string(),
                        "error".to_string(),
                        "-vn".to_string(),
                        "-y".to_string(),
                        "-i".to_string(),
                        args.audiobook.to_string_lossy().to_string(),
                    ]
                    .iter()
                    .chain(prepared.iter())
                    .cloned()
                    .collect()
                } else if cfg!(windows) {
                    [
                        "-/C".to_string(),
                        "ffmpeg.exe".to_string(),
                        "-hide_banner".to_string(),
                        "-loglevel".to_string(),
                        "error".to_string(),
                        "-vn".to_string(),
                        "-y".to_string(),
                        "-i".to_string(),
                        args.audiobook.to_string_lossy().to_string(),
                    ]
                    .iter()
                    .chain(prepared.iter())
                    .cloned()
                    .collect()
                } else {
                    panic!("Unsupported OS possibly.")
                };

                let child = command.args(&args).output().unwrap();
                // dbg!(child);
                n.fetch_add(size, std::sync::atomic::Ordering::Relaxed);
                thread_tx.send(format!("{n:?}/{m} completed!\n")).unwrap();
                println!("{n:?}/{m} completed!");
            })
    }
}
