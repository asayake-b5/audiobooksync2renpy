use epub::doc::EpubDoc;
use image::GenericImage;
use scraper::Selector;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
enum EpubFragment {
    First(String),
    Last(String),
    Image(String),
}

impl EpubFragment {
    pub fn text(&self) -> String {
        match self {
            EpubFragment::First(text) => text.clone(),
            EpubFragment::Last(text) => text.clone(),
            EpubFragment::Image(text) => text.clone(),
        }
    }
}

pub struct EpubImager {
    epub: EpubDoc<BufReader<File>>,
    ids_to_filenames: Vec<(String, String)>,
    game_path: PathBuf,
}

pub fn filename_from_id(epub: &EpubDoc<BufReader<File>>, id: &str) -> Option<String> {
    let filename = epub.resources.get(id)?.0.clone();
    let base = filename.file_name()?.to_str()?.to_string();
    Some(base)
}

pub fn filename_to_renpy(filename: &str) -> String {
    let base = PathBuf::from(filename);
    let base = base.file_stem().unwrap().to_str().unwrap();
    format!(
        "    image {base} = \"{filename}\"
    window hide
    nvl hide
    scene {base}:
        blur 128
    pause
    menu test (nvl=True):
        \"Display Image?\"
        \"Yes\":
            window hide
            nvl hide
            scene {base}:
                blur 0
            pause
        \"No\":
            pass
    window show
"
    )
}

pub fn find_exact(lines: &[String], target: &str) -> Option<usize> {
    lines.iter().position(|e| *e == target)
}

pub fn find_image_line_from_file(lines: &[String], filename: &str) -> Option<usize> {
    let base = PathBuf::from(filename);
    let base = base.file_stem().unwrap().to_str().unwrap();
    let target = format!("image {base} = \"{filename}\"");
    lines.iter().position(|e| e.contains(&target))
}

pub fn find_sentence(lines: &[String], target: &str) -> Option<usize> {
    lines.iter().position(|e| {
        let e2 = e.trim().replace('"', "");
        !e2.is_empty() && (e2.contains(target.trim()) || target.contains(&e2))
    })
}

impl EpubImager {
    //TODO implement the logic from cover to have nicely centered things
    pub fn write_from_id(&mut self, id: &str) {
        if let Some(filename) = filename_from_id(&self.epub, id) {
            let mut path = self.game_path.clone();
            path.push("images");
            std::fs::create_dir_all(&path).unwrap();
            path.push(filename);
            if let Some(data) = self.epub.get_resource(id) {
                Self::write_image(&data.0, path.as_path());
            }
        }
    }

    pub fn write_image(data: &[u8], path: &Path) {
        let mut flat = image::RgbImage::new(1920, 1080);
        let image = image::load_from_memory(data).unwrap();
        let image = image
            .resize(1920, 1080, image::imageops::FilterType::Lanczos3)
            .into_rgb8();
        let dimensions = image.dimensions();
        let x = (1920 - dimensions.0) / 2;
        let y = (1080 - dimensions.1) / 2;
        flat.copy_from(&image, x, y).unwrap();
        flat.save(path).unwrap();
    }

    pub fn write_cover(&mut self) {
        let mut path = self.game_path.clone();
        path.push("gui");
        path.push("main_menu.png");
        if let Some(data) = self.epub.get_cover() {
            Self::write_image(&data.0, path.as_path());
        }
    }

    // pub fn filename_from_id_indexed(&self, id: &str) -> Option<String> {
    //     let a = self.ids_to_filenames.iter().find(|(i, _)| i == id)?;
    //     Some(a.1.to_owned())
    // }

    pub fn id_from_filename(&self, filename: &str) -> Option<String> {
        dbg!(&self.ids_to_filenames);
        let path = PathBuf::from(filename);
        let filename = path.file_name()?.to_str()?;
        let a = self.ids_to_filenames.iter().find(|(_, f)| f == filename)?;
        Some(a.0.to_owned())
    }

    pub fn new(path: PathBuf, renpy_path: PathBuf) -> Self {
        let epub = EpubDoc::new(path).unwrap();
        let ids_to_filenames: Vec<(String, String)> = epub
            .resources
            .clone()
            .keys()
            .map(|e| (e.to_string(), filename_from_id(&epub, e).unwrap()))
            .collect();

        Self {
            epub,
            ids_to_filenames,
            game_path: renpy_path,
        }
    }

    pub fn do_the_epub_thing(&mut self) -> usize {
        dbg!(&self.epub.resources);
        let mut script_path = self.game_path.clone();
        script_path.push("script.rpy");

        let mut script: Vec<String> = std::fs::read_to_string(&script_path)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        dbg!(&script);
        let mut fragments: Vec<EpubFragment> = Vec::new();
        let mut do_after: Vec<EpubFragment> = Vec::new();
        self.write_cover();
        loop {
            if let Some(v) = self.epub.get_current_str() {
                dbg!(&v);
                let document = scraper::Html::parse_document(&v.0);
                let selector_img = Selector::parse("image").unwrap();
                let selector_p = Selector::parse("p").unwrap();
                if let Some(first) = document.select(&selector_p).next() {
                    fragments.push(EpubFragment::First(
                        first
                            .last_child()
                            .unwrap()
                            .value()
                            .as_text()
                            .unwrap()
                            .to_string(),
                    ));
                }
                if let Some(last) = document.select(&selector_p).last() {
                    if let Some(text) = last.last_child().unwrap().value().as_text() {
                        fragments.push(EpubFragment::Last(text.to_string()));
                    } else if let Some(text) = last
                        .last_child()
                        .unwrap()
                        .last_child()
                        .unwrap()
                        .value()
                        .as_text()
                    {
                        fragments.push(EpubFragment::Last(text.to_string()));
                    }
                }

                for element in document.select(&selector_img) {
                    // For some reason, element.value().attr("href") returns none, so we need that.
                    for a in element.value().attrs() {
                        if a.0 == "href" {
                            fragments.push(EpubFragment::Image(a.1.into()));
                        }
                    }
                }
            }
            if !self.epub.go_next() {
                break;
            }
        }
        // dbg!(&fragments);
        if let EpubFragment::Image(image) = &fragments[0] {
            if let Some(pos) = find_exact(&script, "label start:") {
                script.insert(pos + 1, filename_to_renpy(image));
                self.write_from_id(&self.id_from_filename(image).unwrap());
            }
        }

        fragments.windows(3).for_each(|e| {
            let prev = &e[0];
            let ele = &e[1];
            let next = &e[2];
            if let EpubFragment::Image(image) = ele {
                if let EpubFragment::Image(p) = prev {
                    if let Some(pos) = find_image_line_from_file(&script, p) {
                        script.insert(pos + 1, filename_to_renpy(image));
                        self.write_from_id(&self.id_from_filename(image).unwrap());
                    } else if let EpubFragment::First(f) = next {
                        if let Some(pos_next) = find_sentence(&script, f) {
                            script.insert(pos_next - 1, filename_to_renpy(image));
                            self.write_from_id(&self.id_from_filename(image).unwrap());
                        }
                    } else {
                        do_after.push(prev.clone());
                        do_after.push(ele.clone());
                        do_after.push(next.clone());
                    }
                    //TODO if image in unput images, put image too
                    // println!("{}", filename_to_renpy(image));
                } else if let EpubFragment::Image(_n) = next {
                    if let EpubFragment::Last(last) = prev {
                        if let Some(pos_prev) = find_sentence(&script, last) {
                            script.insert(pos_prev + 1, filename_to_renpy(image));
                            self.write_from_id(&self.id_from_filename(image).unwrap());
                        } else {
                            do_after.push(prev.clone());
                            do_after.push(ele.clone());
                            do_after.push(next.clone());
                        }
                    }
                } else {
                    let pos_prev = find_sentence(&script, &prev.text());
                    let pos_next = find_sentence(&script, &next.text());
                    if pos_prev.is_some() && pos_next.is_some() {
                        let p_prev = pos_prev.unwrap();
                        let p_next = pos_next.unwrap();
                        if p_next - p_prev < 15 {
                            script.insert(p_prev + 1, filename_to_renpy(image));
                            self.write_from_id(&self.id_from_filename(image).unwrap());
                        }
                    }
                }
            }
        });

        // dbg!(do_after);
        let mut unadded = 0;
        if !do_after.is_empty() {
            unadded = do_after.len();
        }
        // do_after.chunks(3).for_each(|e| {
        //     let prev = &e[0];
        //     let ele = &e[1];
        //     let next = &e[2];
        //     // println!("--- {prev:?} \n {ele:?} \n {next:?} \n --",);
        //     // println!("{}", self.epub.get_resource_str("id_63").unwrap().0);

        //     // self.epub.set_current_page(0);
        //     // loop {
        //     //     self.epub.go_next()
        //     //     if self.epub.get_current_id()

        //     // }

        //     if let EpubFragment::Image(i) = ele {
        //         println!(
        //             "uri {:?}",
        //             self.epub
        //                 .resource_uri_to_chapter(&PathBuf::from("OEBPS/Image0006.gif"))
        //         );
        //         println!(
        //             "id {:?}",
        //             self.epub
        //                 .resource_id_to_chapter(&self.id_from_filename(i).unwrap())
        //         );
        //         // println!("{}", self.id_from_filename(i).unwrap());
        //         //     let id = self.id_from_filename(i).unwrap();
        //         //     if let Some(position) = self.epub.spine.iter().position(|e| **e == id){
        //         //         if let some(prev_id) =
        //         //     }
        //     }
        // });

        if let Some(EpubFragment::Image(image)) = &fragments.last() {
            script.push(filename_to_renpy(image));
            self.write_from_id(&self.id_from_filename(image).unwrap());
        }
        std::fs::write(&script_path, script.join("\n")).expect("Unable to write file");
        unadded
    }
}
