//! The preset store and the built-in test sources.
//!
//! Presets live in a directory of `.json` files, so the same files serve the
//! Mac app and later the Pi. On the Mac the default is `<repo>/presets`;
//! override with `PEDAL_PRESETS`.

use std::path::PathBuf;

use dsp_graph::Doc;

/// A directory of preset JSON files.
pub struct Store {
    pub dir: PathBuf,
    /// (file name, doc) of every loadable preset.
    pub presets: Vec<(String, Doc)>,
    pub error: Option<String>,
}

impl Store {
    pub fn discover() -> Self {
        let dir = preset_dir();
        let mut s = Self { dir, presets: Vec::new(), error: None };
        s.reload();
        s
    }

    pub fn reload(&mut self) {
        self.presets.clear();
        self.error = None;
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) => {
                self.error = Some(format!("{}: {e}", self.dir.display()));
                return;
            }
        };
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for p in files {
            let name = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            match std::fs::read_to_string(&p) {
                Ok(text) => match Doc::from_json(&text) {
                    Ok(doc) => self.presets.push((name, doc)),
                    Err(e) => {
                        self.error = Some(format!("{}: {e}", p.display()));
                    }
                },
                Err(e) => self.error = Some(format!("{}: {e}", p.display())),
            }
        }
    }

    /// Write a doc as `<dir>/<slug>.json`, avoiding clobbering.
    pub fn save(&mut self, name: &str, doc: &Doc) -> Result<PathBuf, String> {
        let slug = slugify(name);
        let mut path = self.dir.join(format!("{slug}.json"));
        let mut k = 2;
        while path.exists() {
            path = self.dir.join(format!("{slug}-{k}.json"));
            k += 1;
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let json = doc.to_json();
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(path)
    }

    /// Write a doc under an exact slug, refusing to overwrite. Used by rename,
    /// where clobber-avoidance would silently produce a different preset name.
    pub fn write_exact(&mut self, slug: &str, doc: &Doc) -> Result<PathBuf, String> {
        if slug.is_empty() {
            return Err("preset name is empty".to_string());
        }
        let path = self.dir.join(format!("{slug}.json"));
        if path.exists() {
            return Err(format!("preset already exists: {slug}"));
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        std::fs::write(&path, doc.to_json()).map_err(|e| e.to_string())?;
        Ok(path)
    }
}

/// File-system-safe form of a preset name.
pub fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

pub fn preset_dir() -> PathBuf {
    if let Ok(p) = std::env::var("PEDAL_PRESETS") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    // The app runs from <repo>/target/release/pedal-app; the presets sit at
    // <repo>/presets. Walk up until we find them.
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().skip(1) {
            let candidate = ancestor.join("presets");
            if candidate.is_dir() {
                return candidate;
            }
        }
    }
    PathBuf::from("presets")
}

// ---------------------------------------------------------------------------
// Test sources (no guitar required)
// ---------------------------------------------------------------------------

/// A plucked-string riff, Karplus-Strong, polyphonic.
///
/// The pitch glide at the start of each note is what keeps a Karplus-Strong
/// loop from sounding like a buzz: the delay length sweeps from the previous
/// note's pitch, so the string "bends" into tune.
pub struct Riff {
    voices: Vec<Voice>,
    /// Sample counter; the riff is free-running.
    step: usize,
    _sr: f32,
}

struct Voice {
    line: Vec<f32>,
    pos: usize,
    /// Samples left in the note.
    left: usize,
    gain: f32,
}

impl Voice {
    fn new(sr: f32, freq: f32, len: usize, gain: f32) -> Self {
        let n = (sr / freq).max(2.0) as usize;
        let mut line = Vec::with_capacity(n);
        for _ in 0..n {
            line.push(rand() * 2.0 - 1.0);
        }
        Self { line, pos: 0, left: len, gain }
    }

    fn tick(&mut self) -> f32 {
        if self.left == 0 {
            return 0.0;
        }
        self.left -= 1;
        let n = self.line.len();
        let i = self.pos;
        let prev = self.line[(i + n - 1) % n];
        let cur = self.line[i];
        let out = cur;
        // Damped averaging: the low-pass that makes it a string, not a buzz.
        self.line[i] = 0.5 * (cur + prev) * 0.996;
        self.pos = (i + 1) % n;
        out * self.gain
    }
}

/// A cheap deterministic noise source (no rand crate, no allocation in tick).
fn rand() -> f32 {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u32> = const { Cell::new(0x1234_5678) };
    }
    STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        s.set(x);
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    })
}

/// A2 A-minor → F → C → G, one note per beat, two voices.
const CHORDS: &[[f32; 3]] = &[
    [110.0, 164.81, 220.0],   // Am
    [87.31, 130.81, 174.61],  // F
    [130.81, 164.81, 196.0],  // C
    [98.0, 146.83, 196.0],    // G
];

impl Riff {
    pub fn new(sr: f32) -> Self {
        Self { voices: Vec::new(), step: 0, _sr: sr }
    }
}

impl Riff {
    pub fn tick(&mut self, sr: f32) -> f32 {
        if self.step % (sr as usize / 2) == 0 {
            // Every half second: a root, a fifth, an octave.
            let chord = CHORDS[(self.step / (sr as usize / 2)) as usize % CHORDS.len()];
            let len = (sr * 0.45) as usize;
            for (k, &f) in chord.iter().enumerate() {
                self.voices.push(Voice::new(sr, f * (1.0 + k as f32 * 0.001), len, 0.22));
            }
            // Keep at most 6 voices; drop the oldest.
            while self.voices.len() > 6 {
                self.voices.remove(0);
            }
        }
        self.step = self.step.wrapping_add(1);
        let mut s = 0.0;
        for v in &mut self.voices {
            s += v.tick();
        }
        s * 0.5
    }
}
