//! EVE's own settings files: the stream they are stored in, the parts of them this app shows and
//! edits, and the backups it keeps of them.

pub mod backup;
pub mod chat;
pub mod copy;
pub mod file;
#[cfg(test)]
pub(crate) mod fixture;
pub mod layout;
pub mod marshal;
pub mod neocom;
pub mod overview;
pub mod palette;
pub mod probes;
pub mod watchlist;

pub use file::SettingsFile;

#[cfg(test)]
mod real_files {
    use super::*;

    /// Run by hand: reads every file in `EVE_SETTINGS_SAMPLE_DIR`, checks that writing each model
    /// back unchanged leaves the bytes alone, and writes an edited copy of each into
    /// `EVE_SETTINGS_OUT_DIR` for an independent decoder to check.
    #[test]
    #[ignore]
    fn real_files_survive_model_edits() {
        let dir = std::path::PathBuf::from(std::env::var("EVE_SETTINGS_SAMPLE_DIR").expect("EVE_SETTINGS_SAMPLE_DIR"));
        let out = std::env::var("EVE_SETTINGS_OUT_DIR").ok().map(std::path::PathBuf::from);
        let mut seen = 0;
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("core_") || !name.ends_with(".dat") || name.contains("backup") {
                continue;
            }
            let bytes = std::fs::read(e.path()).unwrap();
            let mut f = SettingsFile::parse(&bytes).unwrap();
            seen += 1;

            let lay = layout::Layout::read(&f);
            lay.write(&lay, &mut f);
            let watch = watchlist::read(&f);
            watchlist::write(&mut f, &watch, &watch);
            let over = overview::read(&f);
            overview::write(&mut f, &over, &over);
            assert_eq!(f.bytes().unwrap(), bytes, "no-op model writes changed {name}");

            let mut edited = lay.clone();
            let screen = edited.resolution();
            for s in edited.shown(layout::Filter::default(), screen) {
                let [x, y, w, h] = s.rect;
                edited.set_rect(&s.id, [x + 1, y + 1, w, h], screen);
            }
            edited.add("market", screen);
            edited.write(&lay, &mut f);
            if let Some(mut bar) = neocom::read(&f) {
                bar.reverse();
                neocom::write(&mut f, &bar);
                assert_eq!(neocom::read(&f).unwrap(), bar);
            }
            if let Some(mut chans) = chat::read(&f) {
                chans.reverse();
                chat::write(&mut f, &chans);
            }
            let mut w2 = watch.clone();
            if let Some(l) = w2.lists.first_mut() {
                l.left.push(90_000_001);
            }
            watchlist::write(&mut f, &watch, &w2);
            let forms = probes::read(&f);
            if !forms.list.is_empty() {
                probes::write(&mut f, &forms);
                assert_eq!(probes::read(&f), forms);
            }
            let mut o2 = over.clone();
            if let Some(t) = o2.tabs.first_mut() {
                t.name = "Edited".into();
            }
            if let Some(p) = over.presets.first() {
                o2.duplicate_preset(&p.name);
            }
            overview::write(&mut f, &over, &o2);

            let written = f.bytes().unwrap();
            let back = SettingsFile::parse(&written).unwrap();
            assert_eq!(layout::Layout::read(&back), edited, "{name}");
            assert_eq!(watchlist::read(&back), w2, "{name}");
            assert_eq!(overview::read(&back), o2, "{name}");
            if let Some(out) = &out {
                std::fs::create_dir_all(out).unwrap();
                std::fs::write(out.join(&name), written).unwrap();
            }
        }
        println!("{seen} files");
        assert!(seen > 0);
    }

    /// Run by hand: how many windows each real character file shows per mode, and at which
    /// resolution. Counts only, nothing a settings file says.
    #[test]
    #[ignore]
    fn real_files_layout_census() {
        let dir = std::path::PathBuf::from(std::env::var("EVE_SETTINGS_SAMPLE_DIR").expect("EVE_SETTINGS_SAMPLE_DIR"));
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("core_char_") || name.contains("backup") {
                continue;
            }
            let f = SettingsFile::read(&e.path()).unwrap();
            let started = std::time::Instant::now();
            let l = layout::Layout::read(&f);
            let screen = l.resolution();
            let counts: Vec<String> = [layout::Mode::Space, layout::Mode::Station, layout::Mode::Structure]
                .into_iter()
                .map(|mode| {
                    let shown = l.shown(layout::Filter { mode, fleet: true, drones: true, minimized: true }, screen);
                    for s in &shown {
                        assert!(!l.label(&s.id).is_empty());
                        assert!(s.rect[2] > 0 && s.rect[3] > 0, "a window with no size");
                    }
                    let stacks = shown.iter().filter(|s| s.members.len() > 1).count();
                    format!("{} ({stacks} stacks)", shown.len())
                })
                .collect();
            println!(
                "{} windows on file, {}x{}: space {}, station {}, structure {}; {} addable; sidebar {}, channels {}, watch lists {}; read+layout {:?}",
                l.wins.len(),
                screen[0],
                screen[1],
                counts[0],
                counts[1],
                counts[2],
                l.addable(layout::Filter::default()).len(),
                neocom::read(&f).map_or(0, |b| b.len()),
                chat::read(&f).map_or(0, |c| c.len()),
                watchlist::read(&f).lists.len(),
                started.elapsed(),
            );
        }
    }
}
