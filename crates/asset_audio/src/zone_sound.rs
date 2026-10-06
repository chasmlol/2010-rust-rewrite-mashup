use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::sound_catalog::SoundCatalog;
use crate::sound_load_iw5::Iw5SoundCapture;
use crate::sound_load_t5::T5SoundCapture;
use crate::{ZoneGame, ZoneOwner};

enum Capture {
    Iw4(SoundCatalog),
    Iw5(Iw5SoundCapture),
    T5(T5SoundCapture),
    T6(Option<SoundCatalog>),
}

pub struct ZoneSoundCapture {
    path: PathBuf,
    walk: &'static str,
    capture: Capture,
    stopped: Option<String>,
    claim: Option<Claim>,
}

impl ZoneSoundCapture {
    fn new(path: &Path, game: ZoneGame, walk: &'static str, claim: Option<Claim>) -> Self {
        let capture = match game {
            ZoneGame::Iw4 => {
                let mut catalog = SoundCatalog::default();
                catalog.set_capture_zone(ZoneOwner::from_zone_path(path));
                catalog.set_capture_game(ZoneGame::Iw4);
                Capture::Iw4(catalog)
            }
            ZoneGame::Iw5 => Capture::Iw5(Iw5SoundCapture::for_zone(path)),
            ZoneGame::T5 => Capture::T5(T5SoundCapture::for_zone(path)),
            ZoneGame::T6 => Capture::T6(None),
        };
        Self {
            path: path.to_path_buf(),
            walk,
            capture,
            stopped: None,
            claim,
        }
    }

    pub fn for_map(path: &Path, game: ZoneGame, walk: &'static str) -> Self {
        Self::new(path, game, walk, None)
    }

    pub fn claim_common(path: &Path, game: ZoneGame, walk: &'static str) -> Option<Self> {
        if !is_sound_source(path, game) {
            return None;
        }
        let claim = Claim::take(path)?;
        Some(Self::new(path, game, walk, Some(claim)))
    }

    pub fn finish(mut self, walk: Result<(), String>) -> Result<SoundCatalog, String> {
        let capture = std::mem::replace(&mut self.capture, Capture::Iw4(SoundCatalog::default()));
        match capture {
            Capture::Iw4(mut catalog) => {
                if let Some(stopped) = self.stopped.take() {
                    return Err(format!("sound capture stopped: {stopped}"));
                }
                walk.map_err(|error| format!("sound catalog walk stopped: {error}"))?;
                catalog.resolve_curve_knots();
                catalog.resolve_ent_channels();
                catalog.publish();
                Ok(catalog)
            }
            Capture::Iw5(capture) => {
                self.note_partial(&walk);
                Ok(capture.finish())
            }
            Capture::T5(capture) => {
                self.note_partial(&walk);
                Ok(capture.finish())
            }
            Capture::T6(Some(catalog)) => {
                self.note_partial(&walk);
                Ok(catalog)
            }
            Capture::T6(None) => Err("no T6 aliases were read".to_owned()),
        }
    }

    pub fn deposit(mut self, walk: Result<(), String>) {
        let claim = self.claim.take();
        let path = self.path.clone();
        let walk_name = self.walk;
        let result = self.finish(walk);
        match &result {
            Ok(catalog) => diag::info!(
                Zone,
                "sound source: `{}` captured in the {walk_name} walk — {} aliases {} loaded",
                path.display(),
                catalog.sounds.len(),
                catalog.loaded.len()
            ),
            Err(error) => diag::warn!(
                Zone,
                "sound source: `{}` failed in the {walk_name} walk: {error}",
                path.display()
            ),
        }
        if let Some(claim) = claim {
            claim.fill(result.map(Arc::new), walk_name);
        }
    }

    fn note_partial(&mut self, walk: &Result<(), String>) {
        let reason = self.stopped.take().or_else(|| walk.as_ref().err().cloned());
        if let Some(reason) = reason {
            diag::info!(
                Zone,
                "sound source: `{}` kept what the {} walk read before it stopped: {reason}",
                self.path.display(),
                self.walk
            );
        }
    }

    fn stop<E: std::fmt::Display>(&mut self, error: E) {
        if self.stopped.is_none() {
            self.stopped = Some(error.to_string());
        }
    }

    pub fn iw4(&mut self) -> Option<&mut SoundCatalog> {
        match (&mut self.capture, &self.stopped) {
            (Capture::Iw4(catalog), None) => Some(catalog),
            _ => None,
        }
    }

    pub fn iw5(&mut self) -> Option<&mut Iw5SoundCapture> {
        match (&mut self.capture, &self.stopped) {
            (Capture::Iw5(capture), None) => Some(capture),
            _ => None,
        }
    }

    pub fn set_t6(&mut self, catalog: SoundCatalog) {
        if let Capture::T6(slot) = &mut self.capture {
            *slot = Some(catalog);
        }
    }

    pub fn t5(&mut self) -> Option<&mut T5SoundCapture> {
        match (&mut self.capture, &self.stopped) {
            (Capture::T5(capture), None) => Some(capture),
            _ => None,
        }
    }

    pub fn guard<E: std::fmt::Display>(&mut self, result: Result<(), E>) {
        if let Err(error) = result {
            self.stop(error);
        }
    }
}

fn is_sound_source(path: &Path, game: ZoneGame) -> bool {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    let names: &[&str] = match game {
        ZoneGame::Iw4 => &[
            "code_post_gfx_mp",
            "localized_code_post_gfx_mp",
            "patch_mp",
            "common_mp",
            "localized_common_mp",
        ],
        ZoneGame::Iw5 => &["code_post_gfx_mp", "common_mp", "localized_common_mp"],
        ZoneGame::T5 => &["code_post_gfx_mp", "common_mp", "localized_common_mp"],
        ZoneGame::T6 => &["common_mp"],
    };
    names.iter().any(|name| stem.eq_ignore_ascii_case(name))
}

type Stored = Result<Arc<SoundCatalog>, String>;

enum Slot {
    Capturing,
    Ready { catalog: Stored, walk: &'static str },
}

struct Store {
    slots: Mutex<HashMap<PathBuf, Slot>>,
    changed: Condvar,
}

fn store() -> &'static Store {
    static STORE: std::sync::OnceLock<Store> = std::sync::OnceLock::new();
    STORE.get_or_init(|| Store {
        slots: Mutex::new(HashMap::new()),
        changed: Condvar::new(),
    })
}

fn store_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

struct Claim {
    key: PathBuf,
    filled: bool,
}

impl Claim {
    fn take(path: &Path) -> Option<Self> {
        let key = store_key(path);
        let store = store();
        let mut slots = store.slots.lock().unwrap_or_else(|e| e.into_inner());
        if slots.contains_key(&key) {
            return None;
        }
        slots.insert(key.clone(), Slot::Capturing);
        Some(Self { key, filled: false })
    }

    fn fill(mut self, catalog: Stored, walk: &'static str) {
        let store = store();
        let mut slots = store.slots.lock().unwrap_or_else(|e| e.into_inner());
        slots.insert(self.key.clone(), Slot::Ready { catalog, walk });
        self.filled = true;
        store.changed.notify_all();
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if self.filled {
            return;
        }
        let store = store();
        let mut slots = store.slots.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(slots.get(&self.key), Some(Slot::Capturing)) {
            slots.remove(&self.key);
        }
        store.changed.notify_all();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneSoundOrigin {
    Shared(&'static str),
    AudioOnly,
}

pub fn ensure_zone_sound(path: &Path) -> (Stored, ZoneSoundOrigin) {
    let key = store_key(path);
    let store = store();
    let started = Instant::now();
    let mut reported = false;
    let mut slots = store.slots.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        match slots.get(&key) {
            Some(Slot::Ready { catalog, walk }) => {
                return (catalog.clone(), ZoneSoundOrigin::Shared(walk));
            }
            Some(Slot::Capturing) => {
                if !reported && started.elapsed() >= Duration::from_secs(10) {
                    reported = true;
                    diag::warn!(
                        Zone,
                        "sound source: still waiting on the walk capturing `{}`",
                        path.display()
                    );
                }
                slots = store
                    .changed
                    .wait_timeout(slots, Duration::from_millis(250))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            None => break,
        }
    }
    if crate::zone_game_for_path(path) == Some(ZoneGame::T6) {
        return (
            Err(format!(
                "{}: T6 sounds come from the common walk, which has not run",
                path.display()
            )),
            ZoneSoundOrigin::AudioOnly,
        );
    }
    slots.insert(key.clone(), Slot::Capturing);
    drop(slots);
    let claim = Claim { key, filled: false };
    let walked = crate::sound_load::walk_zone_sound(path).map(Arc::new);
    claim.fill(walked.clone(), "audio-only");
    (walked, ZoneSoundOrigin::AudioOnly)
}

#[macro_export]
macro_rules! forward_iw4_sound {
    () => {
        fn capture_loaded_sound(
            &mut self,
            s: &fastfile_iw4::ZoneStream<'_>,
            header: fastfile_iw4::Ptr,
            pcm: fastfile_iw4::Ptr,
            data_len: usize,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result = fastfile_iw4::AssetLinkSink::capture_loaded_sound(
                    catalog, s, header, pcm, data_len,
                );
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_last_loaded_to_sound_file(
            &mut self,
            file: fastfile_iw4::Ptr,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result =
                    fastfile_iw4::AssetLinkSink::bind_last_loaded_to_sound_file(catalog, file);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_sound(
            &mut self,
            s: &fastfile_iw4::ZoneStream<'_>,
            list: fastfile_iw4::Ptr,
            count: usize,
            head: Option<fastfile_iw4::Ptr>,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result =
                    fastfile_iw4::AssetLinkSink::capture_sound(catalog, s, list, count, head);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_snd_curve(
            &mut self,
            s: &fastfile_iw4::ZoneStream<'_>,
            header: fastfile_iw4::Ptr,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result = fastfile_iw4::AssetLinkSink::capture_snd_curve(catalog, s, header);
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_streamed_sound_file(
            &mut self,
            file: fastfile_iw4::Ptr,
            dir: &str,
            name: &str,
        ) -> fastfile_iw4::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(catalog) = sound.iw4()
            {
                let result =
                    fastfile_iw4::AssetLinkSink::bind_streamed_sound_file(catalog, file, dir, name);
                sound.guard(result);
            }
            Ok(())
        }
    };
}

#[macro_export]
macro_rules! forward_iw5_sound {
    () => {
        fn capture_loaded_sound(
            &mut self,
            s: &fastfile_iw5::ZoneStream<'_>,
            header: fastfile_iw5::Ptr,
            pcm: fastfile_iw5::Ptr,
            data_len: usize,
        ) -> fastfile_iw5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.iw5()
            {
                let result = fastfile_iw5::AssetLinkSink::capture_loaded_sound(
                    capture, s, header, pcm, data_len,
                );
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_last_loaded_to_sound_file(
            &mut self,
            file: fastfile_iw5::Ptr,
        ) -> fastfile_iw5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.iw5()
            {
                let result =
                    fastfile_iw5::AssetLinkSink::bind_last_loaded_to_sound_file(capture, file);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_sound(
            &mut self,
            s: &fastfile_iw5::ZoneStream<'_>,
            list: fastfile_iw5::Ptr,
            count: usize,
            head: Option<fastfile_iw5::Ptr>,
        ) -> fastfile_iw5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.iw5()
            {
                let result =
                    fastfile_iw5::AssetLinkSink::capture_sound(capture, s, list, count, head);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_snd_curve(
            &mut self,
            s: &fastfile_iw5::ZoneStream<'_>,
            header: fastfile_iw5::Ptr,
        ) -> fastfile_iw5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.iw5()
            {
                let result = fastfile_iw5::AssetLinkSink::capture_snd_curve(capture, s, header);
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_streamed_sound_file(
            &mut self,
            file: fastfile_iw5::Ptr,
            dir: &str,
            name: &str,
        ) -> fastfile_iw5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.iw5()
            {
                let result =
                    fastfile_iw5::AssetLinkSink::bind_streamed_sound_file(capture, file, dir, name);
                sound.guard(result);
            }
            Ok(())
        }
    };
}

#[macro_export]
macro_rules! forward_t5_sound {
    () => {
        fn capture_snd_curves(
            &mut self,
            s: &fastfile_t5::ZoneStream<'_>,
            rows: fastfile_t5::Ptr,
            count: usize,
        ) -> fastfile_t5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.t5()
            {
                let result =
                    fastfile_t5::AssetLinkSink::capture_snd_curves(capture, s, rows, count);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_snd_groups(
            &mut self,
            s: &fastfile_t5::ZoneStream<'_>,
            rows: fastfile_t5::Ptr,
            count: usize,
        ) -> fastfile_t5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.t5()
            {
                let result =
                    fastfile_t5::AssetLinkSink::capture_snd_groups(capture, s, rows, count);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_loaded_sound(
            &mut self,
            s: &fastfile_t5::ZoneStream<'_>,
            header: fastfile_t5::Ptr,
            pcm: fastfile_t5::Ptr,
            data_len: usize,
        ) -> fastfile_t5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.t5()
            {
                let result = fastfile_t5::AssetLinkSink::capture_loaded_sound(
                    capture, s, header, pcm, data_len,
                );
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_last_loaded_to_sound_file(
            &mut self,
            file: fastfile_t5::Ptr,
        ) -> fastfile_t5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.t5()
            {
                let result =
                    fastfile_t5::AssetLinkSink::bind_last_loaded_to_sound_file(capture, file);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_sound(
            &mut self,
            s: &fastfile_t5::ZoneStream<'_>,
            list: fastfile_t5::Ptr,
            count: usize,
            head: Option<fastfile_t5::Ptr>,
        ) -> fastfile_t5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.t5()
            {
                let result =
                    fastfile_t5::AssetLinkSink::capture_sound(capture, s, list, count, head);
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_streamed_sound_file(
            &mut self,
            file: fastfile_t5::Ptr,
            dir: &str,
            name: &str,
        ) -> fastfile_t5::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.t5()
            {
                let result =
                    fastfile_t5::AssetLinkSink::bind_streamed_sound_file(capture, file, dir, name);
                sound.guard(result);
            }
            Ok(())
        }
    };
}

impl ZoneSoundCapture {
    pub fn iw4_loaded(
        &mut self,
        s: &fastfile_iw4::ZoneStream<'_>,
        ty: fastfile_iw4::AssetType,
        slot: fastfile_iw4::Ptr,
        insert_slot: Option<fastfile_iw4::Ptr>,
    ) {
        if let Some(catalog) = self.iw4() {
            let result = fastfile_iw4::AssetLinkSink::loaded(catalog, s, ty, slot, insert_slot);
            self.guard(result);
        }
    }

    pub fn iw4_alias(
        &mut self,
        ty: fastfile_iw4::AssetType,
        slot: fastfile_iw4::Ptr,
        target: fastfile_iw4::Ptr,
    ) {
        if let Some(catalog) = self.iw4() {
            let result = fastfile_iw4::AssetLinkSink::alias(catalog, ty, slot, target);
            self.guard(result);
        }
    }

    pub fn iw5_loaded(
        &mut self,
        s: &fastfile_iw5::ZoneStream<'_>,
        ty: fastfile_iw5::AssetType,
        slot: fastfile_iw5::Ptr,
        insert_slot: Option<fastfile_iw5::Ptr>,
    ) {
        if let Some(capture) = self.iw5() {
            let result = fastfile_iw5::AssetLinkSink::loaded(capture, s, ty, slot, insert_slot);
            self.guard(result);
        }
    }

    pub fn iw5_alias(
        &mut self,
        ty: fastfile_iw5::AssetType,
        slot: fastfile_iw5::Ptr,
        target: fastfile_iw5::Ptr,
    ) {
        if let Some(capture) = self.iw5() {
            let result = fastfile_iw5::AssetLinkSink::alias(capture, ty, slot, target);
            self.guard(result);
        }
    }

    pub fn raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) {
        if self.stopped.is_some() {
            return;
        }
        let catalog = match &mut self.capture {
            Capture::Iw4(catalog) => catalog,
            Capture::Iw5(capture) => capture.catalog_mut(),
            Capture::T5(capture) => capture.catalog_mut(),
            Capture::T6(_) => return,
        };
        catalog.ingest_rawfile(name, data, zlib_compressed);
    }
}
