//! The slice oracle: a slice must typeset its criterion exactly as the
//! original does.  Each criterion line is sliced as a standalone file,
//! both are built with the real engine, and the region SyncTeX gives for
//! the line is cut out of each PDF, rendered and compared pixel by pixel,
//! with the text `pdftotext` reads from it.  `satex-oracle --slice` runs it
//! over whole corpora, the slice tests on a few criteria.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::machine::Machine;
use crate::query::{self, At, Direction};

/// What one criterion came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The region renders the same.
    Pass,
    /// The line typesets nothing, and the slice compiles.
    Empty,
    Mismatch(String),
    Fail(String),
}

impl Verdict {
    pub fn word(&self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Empty => "empty",
            Verdict::Mismatch(_) => "mismatch",
            Verdict::Fail(_) => "fail",
        }
    }

    fn parse(text: &str) -> Option<Verdict> {
        let (word, why) = text.split_once('\n').unwrap_or((text, ""));
        Some(match word.trim() {
            "pass" => Verdict::Pass,
            "empty" => Verdict::Empty,
            "mismatch" => Verdict::Mismatch(why.to_string()),
            "fail" => Verdict::Fail(why.to_string()),
            _ => return None,
        })
    }
}

/// DPI the regions are rendered at.
const DPI: &str = "144";
/// Room around a SyncTeX box, in bp, for what reaches below the baseline.
const PAD: f64 = 3.0;

/// The engine a file asks for (`% !TeX program = …`); a plain run is built
/// with pdfTeX so it has a PDF.
fn engine(source: &str) -> &'static str {
    let program = source.lines().take(5).find_map(|l| l.split_once("program").map(|(_, p)| p.to_string()));
    match program.map(|p| p.trim_start_matches([' ', '=']).trim().to_string()).as_deref() {
        Some("tex" | "pdftex" | "etex") => "pdftex",
        Some("lualatex") => "lualatex",
        Some("xelatex") => "xelatex",
        _ => "pdflatex",
    }
}

fn run(cmd: &mut Command) -> bool {
    cmd.output().is_ok_and(|o| o.status.success())
}

/// Build `main` in `dir` as a latexmk would: twice, with BibTeX when the
/// first run asked for it, and once more.
pub fn build(dir: &Path, main: &str, engine: &str) -> bool {
    let pass = || {
        run(Command::new(engine)
            .current_dir(dir)
            .args(["-synctex=1", "-interaction=nonstopmode", "-halt-on-error"])
            .arg(main))
    };
    if !pass() {
        return false;
    }
    let stem = Path::new(main).file_stem().unwrap_or_default().to_string_lossy().to_string();
    let aux = std::fs::read_to_string(dir.join(format!("{stem}.aux"))).unwrap_or_default();
    if aux.contains("\\bibdata") {
        run(Command::new("bibtex").current_dir(dir).arg(&stem));
    }
    pass() && pass()
}

/// Where `line` of `file` went, from the `.synctex.gz` beside `pdf`: on the
/// first page that holds any of its material, the line boxes (the outermost
/// hbox in a vbox) around that material, joined; in PDF coordinates.
fn region(dir: &Path, pdf: &str, file: &str, line: u32) -> Option<(i64, [f64; 4])> {
    let synctex = Path::new(pdf).with_extension("synctex.gz");
    let out = Command::new("gzip").arg("-dc").arg(dir.join(synctex)).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let wanted = format!("/{}", file.trim_start_matches("./"));
    let tags: Vec<String> = text
        .lines()
        .filter_map(|l| l.strip_prefix("Input:"))
        .filter_map(|l| l.split_once(':'))
        .filter(|(_, path)| path.replace("/./", "/").ends_with(&wanted))
        .map(|(tag, _)| tag.to_string())
        .collect();
    let sp = |n: &str| n.parse::<f64>().ok().map(|n| n / 65536.0 * 72.0 / 72.27);
    // Open boxes: whether an hbox, and its rectangle top-down.
    let mut open: Vec<(bool, [f64; 4])> = Vec::new();
    let (mut page, mut found): (i64, Option<(i64, [f64; 4])>) = (0, None);
    let add = |page: i64, r: [f64; 4], found: &mut Option<(i64, [f64; 4])>| match found {
        Some((p, a)) if *p == page => *a = [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])],
        Some(_) => {}
        None => *found = Some((page, r)),
    };
    for l in text.lines().skip_while(|l| !l.starts_with("Content:")) {
        let Some(kind) = l.chars().next() else { continue };
        match kind {
            '{' => page = l[1..].parse().unwrap_or(page),
            ']' | ')' => {
                open.pop();
            }
            '[' | '(' | 'x' | 'k' | 'g' | 'h' | 'v' | '$' => {
                let (head, rest) = l[1..].split_once(':').unwrap_or(("", ""));
                let mine =
                    head.split_once(',').is_some_and(|(t, n)| tags.iter().any(|w| w == t) && n == line.to_string());
                let fields: Vec<&str> = rest.split([':', ',']).collect();
                let rect = match fields[..] {
                    [h, v, w, height, depth, ..] => (|| {
                        let (h, v) = (sp(h)?, sp(v)?);
                        Some([h, v - sp(height)?, h + sp(w)?, v + sp(depth)?])
                    })(),
                    _ => None,
                };
                if matches!(kind, '[' | '(') {
                    open.push((kind == '(', rect.unwrap_or_default()));
                }
                if !mine {
                    continue;
                }
                // The line the material sits in: the outermost hbox whose parent is a vbox.
                let at = (0..open.len()).find(|&i| open[i].0 && (i == 0 || !open[i - 1].0));
                match at {
                    Some(i) => add(page, open[i].1, &mut found),
                    None => {
                        if let Some(r) = rect {
                            add(page, r, &mut found);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let (first, [x0, top, x1, bottom]) = found?;
    let bytes = std::fs::read(dir.join(pdf)).ok()?;
    let (_, height) = crate::pdfbox::page_box(&bytes, "media", first)?;
    let height = height as f64 / 65536.0 * 72.0 / 72.27;
    Some((first, [x0 - PAD, height - bottom - PAD, x1 + PAD, height - top + PAD]))
}

/// The region of `pdf` cut out as a page of its own, rendered: the PNG and
/// the text on it.
fn cut(dir: &Path, pdf: &str, (page, [x0, y0, x1, y1]): (i64, [f64; 4]), name: &str) -> Option<(PathBuf, String)> {
    let source = format!(
        "\\documentclass{{standalone}}\\usepackage{{graphicx}}\\begin{{document}}\
         \\includegraphics[page={page},viewport={x0:.4} {y0:.4} {x1:.4} {y1:.4},clip]{{{pdf}}}\\end{{document}}\n"
    );
    std::fs::write(dir.join(format!("{name}.tex")), source).ok()?;
    run(Command::new("pdflatex").current_dir(dir).args(["-interaction=nonstopmode", "-halt-on-error"]).arg(name))
        .then_some(())?;
    run(Command::new("pdftocairo")
        .current_dir(dir)
        .args(["-png", "-r", DPI, "-singlefile"])
        .arg(format!("{name}.pdf"))
        .arg(name))
    .then_some(())?;
    let text = Command::new("pdftotext").current_dir(dir).arg(format!("{name}.pdf")).arg("-").output().ok()?;
    Some((dir.join(format!("{name}.png")), String::from_utf8_lossy(&text.stdout).to_string()))
}

/// One document: where it is, and its built original.
pub struct Document {
    pub dir: PathBuf,
    pub main: String,
    work: PathBuf,
    engine: &'static str,
}

impl Document {
    /// Copy the document's directory into `work` and build it there.
    pub fn prepare(dir: &Path, main: &str, work: &Path) -> Result<Document, String> {
        let source = std::fs::read_to_string(dir.join(main)).map_err(|e| format!("{main}: {e}"))?;
        let engine = engine(&source);
        let original = work.join("original");
        let built = original.join(Path::new(main).with_extension("pdf"));
        if !built.is_file() {
            let _ = std::fs::remove_dir_all(&original);
            copy_dir(dir, &original).map_err(|e| format!("copying {}: {e}", dir.display()))?;
            if !build(&original, main, engine) {
                return Err(format!("the original does not build with {engine}"));
            }
        }
        Ok(Document { dir: dir.to_path_buf(), main: main.to_string(), work: work.to_path_buf(), engine })
    }

    /// Every line of the document's own files that holds code: the main file
    /// and what it inputs, as `(file, line)` relative to the document.
    pub fn criteria(&self) -> Vec<(String, u32)> {
        let original = self.work.join("original");
        let mut files = vec![self.main.clone()];
        let log =
            std::fs::read_to_string(original.join(Path::new(&self.main).with_extension("log"))).unwrap_or_default();
        for word in log.split(['(', ')', ' ', '\n']) {
            let name = word.trim_start_matches("./");
            if name.ends_with(".tex") && !name.starts_with('/') && name != self.main && original.join(name).is_file() {
                files.push(name.to_string());
            }
        }
        files.dedup();
        let mut out = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(original.join(&file)).unwrap_or_default();
            for (i, line) in text.lines().enumerate() {
                let code = line.split('%').next().unwrap_or_default().trim();
                if !code.is_empty() {
                    out.push((file.clone(), i as u32 + 1));
                }
            }
        }
        out
    }

    /// The standalone slices for `criteria`, written into their own
    /// directories: the run happens once for all of them.
    pub fn slice_all(&self, criteria: &[(String, u32)], trim: bool) -> Vec<(String, u32, PathBuf)> {
        let path = self.work.join("original").join(&self.main);
        let source = std::fs::read_to_string(&path).unwrap_or_default();
        let mut cfg = crate::config::Config::discover(&self.work.join("original"));
        cfg.slice.standalone = true;
        cfg.slice.trim = trim;
        let analysis = Machine::analyze(&source, Some(&path), &cfg);
        let mut out = Vec::new();
        for (file, line) in criteria {
            let dir = self.work.join(format!("{}-{line}", file.replace('/', "_")));
            if !dir.join("verdict").is_file() {
                let _ = std::fs::remove_dir_all(&dir);
                let _ = std::fs::create_dir_all(&dir);
                let at = At {
                    file: (file != &self.main).then(|| file.clone()),
                    line: *line,
                    col: 1,
                    end: Some((*line, u32::MAX)),
                };
                let records = query::slice(&analysis, &[], Some(&at), Direction::Backward);
                let (text, origin) = query::reconstruct_mapped(&analysis, &records, &source);
                let _ = std::fs::write(dir.join(&self.main), text);
                // Where the criterion's line went: the slice's line that comes from it.
                let here = origin
                    .iter()
                    .position(|o| o.as_ref().is_some_and(|(f, l)| l == line && Path::new(f).ends_with(file)));
                let _ = std::fs::write(dir.join("line"), here.map_or(String::new(), |i| (i + 1).to_string()));
            }
            out.push((file.clone(), *line, dir));
        }
        out
    }

    /// Build the slice in `dir` and compare the criterion's region.  The
    /// verdict is kept in `dir`, so a run can be resumed.
    pub fn check(&self, file: &str, line: u32, dir: &Path) -> Verdict {
        if let Some(v) = std::fs::read_to_string(dir.join("verdict")).ok().and_then(|t| Verdict::parse(&t)) {
            return v;
        }
        let verdict = self.compare(file, line, dir);
        let why = match &verdict {
            Verdict::Mismatch(why) | Verdict::Fail(why) => why.as_str(),
            _ => "",
        };
        let _ = std::fs::write(dir.join("verdict"), format!("{}\n{why}", verdict.word()));
        verdict
    }

    fn compare(&self, file: &str, line: u32, dir: &Path) -> Verdict {
        let original = self.work.join("original");
        let pdf = Path::new(&self.main).with_extension("pdf").to_string_lossy().to_string();
        if !build(dir, &self.main, self.engine) {
            return Verdict::Fail("the slice does not build".into());
        }
        let here: Option<u32> = std::fs::read_to_string(dir.join("line")).ok().and_then(|l| l.trim().parse().ok());
        let before = region(&original, &pdf, file, line);
        // A file the slice reads whole stays a file of its own, at its own lines.
        let after = match here {
            Some(l) => region(dir, &pdf, &self.main, l),
            None => region(dir, &pdf, file, line),
        };
        let (before, after) = match (before, after) {
            (None, None) => return Verdict::Empty,
            (Some(_), None) => return Verdict::Mismatch("the slice typesets nothing for the line".into()),
            (None, Some(_)) => return Verdict::Mismatch("the slice typesets the line, the original nothing".into()),
            (Some(b), Some(a)) => (b, a),
        };
        let size =
            |r: [f64; 4]| ((r[2] - r[0]) * 100.0).round() as i64 * 1_000_000 + ((r[3] - r[1]) * 100.0).round() as i64;
        if size(before.1) != size(after.1) {
            return Verdict::Mismatch(format!("the region is {:?} instead of {:?}", after.1, before.1));
        }
        let _ = std::fs::copy(original.join(&pdf), dir.join("original.pdf"));
        let (Some(want), Some(got)) = (cut(dir, "original.pdf", before, "want"), cut(dir, &pdf, after, "got")) else {
            return Verdict::Fail("cutting a region out failed".into());
        };
        if want.1 != got.1 {
            return Verdict::Mismatch(format!("the text is {:?} instead of {:?}", got.1.trim(), want.1.trim()));
        }
        if std::fs::read(&want.0).ok() != std::fs::read(&got.0).ok() {
            let diff = dir.join("diff.png");
            let _ = Command::new("compare").arg(&want.0).arg(&got.0).arg(&diff).output();
            return Verdict::Mismatch(format!("the pixels differ: {}", diff.display()));
        }
        Verdict::Pass
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Check every slice of `doc` in `jobs` threads; the verdicts in order.
pub fn check_all(doc: &Document, slices: &[(String, u32, PathBuf)], jobs: usize) -> Vec<Verdict> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let verdicts = std::sync::Mutex::new(vec![Verdict::Empty; slices.len()]);
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((file, line, dir)) = slices.get(i) else { break };
                    let v = doc.check(file, *line, dir);
                    verdicts.lock().unwrap_or_else(|e| e.into_inner())[i] = v;
                }
            });
        }
    });
    verdicts.into_inner().unwrap_or_else(|e| e.into_inner())
}
