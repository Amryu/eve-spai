//! Plays speech as it is made. On Linux the samples are piped into PipeWire's or PulseAudio's player,
//! so they sound as they arrive and nothing new is linked in; elsewhere each sentence plays as a
//! WAV in turn. Volume is applied to the samples, so the app's master volume and mute hold.

#[cfg(all(unix, not(target_os = "macos")))]
use std::io::Write;
#[cfg(all(unix, not(target_os = "macos")))]
use std::process::{Child, Command, Stdio};

/// 16-bit mono WAV around `pcm`. Linux streams raw samples and never needs one.
#[cfg_attr(all(unix, not(target_os = "macos")), allow(dead_code))]
pub fn wav(rate: u32, pcm: &[i16]) -> Vec<u8> {
    let data = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

/// The samples of a 16-bit mono or stereo PCM WAV (stereo is mixed down), and its rate.
pub fn read_wav(b: &[u8]) -> Option<(u32, Vec<i16>)> {
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let (mut rate, mut channels, mut bits) = (0u32, 1u16, 16u16);
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let body = b.get(i + 8..(i + 8 + len).min(b.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            channels = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            if bits != 16 || channels == 0 {
                return None;
            }
            let frames = body.chunks_exact(2 * channels as usize);
            let pcm = frames
                .map(|f| {
                    let sum: i32 = f.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]) as i32).sum();
                    (sum / channels as i32) as i16
                })
                .collect();
            return Some((rate, pcm));
        }
        i += 8 + len + (len & 1);
    }
    None
}

pub fn gain(pcm: &mut [i16], g: f32) {
    if (g - 1.0).abs() < 0.001 {
        return;
    }
    for s in pcm {
        *s = (*s as f32 * g).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    }
}

#[derive(Default)]
pub struct Player {
    #[cfg(all(unix, not(target_os = "macos")))]
    child: Option<(Child, u32)>,
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    queue: Option<std::sync::Arc<queue::Queue>>,
}

impl Player {
    /// Queues `pcm` at `rate` behind what is already playing.
    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn play(&mut self, rate: u32, pcm: &[i16]) {
        if pcm.is_empty() {
            return;
        }
        if self.child.as_ref().is_none_or(|(_, r)| *r != rate) {
            self.finish();
            self.child = spawn_pipe(rate).map(|c| (c, rate));
        }
        let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        let failed = match self.child.as_mut().and_then(|(c, _)| c.stdin.as_mut()) {
            Some(stdin) => stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err(),
            None => true,
        };
        if failed {
            self.stop();
        }
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    pub fn play(&mut self, rate: u32, pcm: &[i16]) {
        if pcm.is_empty() {
            return;
        }
        self.queue.get_or_insert_with(queue::Queue::start).push(wav(rate, pcm));
    }

    /// Lets what is queued play out, and waits for it.
    pub fn finish(&mut self) {
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some((mut c, _)) = self.child.take() {
            drop(c.stdin.take());
            let _ = c.wait();
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        if let Some(q) = &self.queue {
            q.wait_idle();
        }
    }

    /// Silences it now.
    pub fn stop(&mut self) {
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some((mut c, _)) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        if let Some(q) = &self.queue {
            q.clear();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_pipe(rate: u32) -> Option<Child> {
    let pw = Command::new("pw-play")
        .args(["-a", "--rate", &rate.to_string(), "--channels", "1", "--format", "s16", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    pw.or_else(|_| {
        Command::new("paplay")
            .args(["--raw", &format!("--rate={rate}"), "--format=s16le", "--channels=1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    })
    .ok()
}

/// Sentences played one after another on their own thread, so a stop can cut in.
#[cfg(not(all(unix, not(target_os = "macos"))))]
mod queue {
    use std::collections::VecDeque;
    use std::sync::{Arc, Condvar, Mutex};

    #[derive(Default)]
    struct State {
        items: VecDeque<Vec<u8>>,
        playing: bool,
        #[cfg(target_os = "macos")]
        child: Option<std::process::Child>,
    }

    pub struct Queue {
        state: Mutex<State>,
        cv: Condvar,
    }

    impl Queue {
        pub fn start() -> Arc<Self> {
            let q = Arc::new(Queue { state: Mutex::new(State::default()), cv: Condvar::new() });
            let worker = q.clone();
            let _ = std::thread::Builder::new().name("ai-speech-out".into()).spawn(move || worker.run());
            q
        }

        pub fn push(&self, wav: Vec<u8>) {
            self.state.lock().unwrap_or_else(|e| e.into_inner()).items.push_back(wav);
            self.cv.notify_all();
        }

        pub fn wait_idle(&self) {
            let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            while st.playing || !st.items.is_empty() {
                st = self.cv.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        }

        pub fn clear(&self) {
            let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.items.clear();
            #[cfg(target_os = "macos")]
            if let Some(c) = st.child.as_mut() {
                let _ = c.kill();
            }
            #[cfg(target_os = "windows")]
            super::stop_windows();
        }

        fn run(&self) {
            loop {
                let wav = {
                    let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if let Some(w) = st.items.pop_front() {
                            st.playing = true;
                            break w;
                        }
                        st.playing = false;
                        self.cv.notify_all();
                        st = self.cv.wait(st).unwrap_or_else(|e| e.into_inner());
                    }
                };
                self.play_one(&wav);
            }
        }

        #[cfg(target_os = "windows")]
        fn play_one(&self, wav: &[u8]) {
            super::play_windows(wav);
        }

        #[cfg(target_os = "macos")]
        fn play_one(&self, wav: &[u8]) {
            let path = std::env::temp_dir().join(format!("eve-spai-say-{}.wav", std::process::id()));
            if std::fs::write(&path, wav).is_err() {
                return;
            }
            let Ok(child) = std::process::Command::new("afplay").arg(&path).spawn() else { return };
            self.state.lock().unwrap_or_else(|e| e.into_inner()).child = Some(child);
            // Waited on without the lock held, so a stop can kill it meanwhile.
            loop {
                let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
                match st.child.as_mut().map(|c| c.try_wait()) {
                    Some(Ok(None)) => {}
                    _ => {
                        st.child = None;
                        return;
                    }
                }
                drop(st);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn play_windows(wav: &[u8]) {
    const SND_SYNC: u32 = 0;
    const SND_MEMORY: u32 = 0x0004;
    unsafe {
        PlaySoundW(wav.as_ptr() as *const u16, std::ptr::null_mut(), SND_SYNC | SND_MEMORY);
    }
}

#[cfg(target_os = "windows")]
#[link(name = "winmm")]
extern "system" {
    fn PlaySoundW(psz_sound: *const u16, hmod: *mut core::ffi::c_void, flags: u32) -> i32;
}

#[cfg(target_os = "windows")]
fn stop_windows() {
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trips_and_gain_scales() {
        let pcm: Vec<i16> = (0..100).map(|i| (i * 100) as i16).collect();
        let (rate, back) = read_wav(&wav(22050, &pcm)).unwrap();
        assert_eq!((rate, back.clone()), (22050, pcm));
        let mut half = back;
        gain(&mut half, 0.5);
        assert_eq!(half[10], 500);
        let mut loud = vec![30_000i16];
        gain(&mut loud, 2.0);
        assert_eq!(loud[0], i16::MAX, "clipped, not wrapped");
    }
}
