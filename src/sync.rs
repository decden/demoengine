use rust_rocket::{Event, Rocket};
use time;

pub trait SyncTracker {
    fn require_track(&mut self, track: &str);

    fn update(&mut self);
    fn get_time(&self) -> f64;
    fn get_value(&self, track: &str) -> Option<f32>;
}

// Describes the time at which playback started, or was resumed
pub struct PlayStartPoint {
    pub base_time: f64,
    pub real_time: f64,
}

pub struct RocketSyncTracker {
    rocket: Rocket,
    fps: f64,
    time: f64,
    play_start_point: Option<PlayStartPoint>,
}
impl RocketSyncTracker {
    pub fn new(fps: f64) -> Result<Self, String> {
        let mut tracker = RocketSyncTracker {
            rocket: Rocket::new().map_err(|e| format!("{:?}", e))?,
            fps: fps,
            time: 0.0,
            play_start_point: None,
        };
        tracker.play();
        Ok(tracker)
    }

    fn pause(&mut self) {
        if let Some(p) = self.play_start_point.take() {
            self.time = p.base_time + (time::precise_time_s() - p.real_time);
        }
    }

    fn play(&mut self) {
        self.play_start_point = Some(PlayStartPoint {
            base_time: self.time,
            real_time: time::precise_time_s(),
        });
    }

    fn go_to_time(&mut self, time: f64) {
        if self.play_start_point.is_some() {
            self.pause();
            self.time = time;
            self.play();
        } else {
            self.time = time;
        }
    }
}
impl SyncTracker for RocketSyncTracker {
    fn require_track(&mut self, track: &str) {
        self.rocket.get_track_mut(track);
    }

    fn update(&mut self) {
        while let Some(event) = self.rocket.poll_events() {
            match event {
                Event::SetRow(r) => {
                    let time = r as f64 / self.fps;
                    self.go_to_time(time);
                }
                Event::Pause(pause) => {
                    if pause {
                        self.pause();
                    } else {
                        self.play();
                    }
                }
                _ => {}
            }
        }

        if let Some(ref p) = self.play_start_point {
            self.time = p.base_time + (time::precise_time_s() - p.real_time);
            self.rocket.set_row((self.time * self.fps) as u32);
        }
    }

    fn get_time(&self) -> f64 {
        self.time
    }
    fn get_value(&self, track: &str) -> Option<f32> {
        let value = self
            .rocket
            .get_track(track)
            .map(|t| t.get_value((self.time * self.fps) as f32));
        value
    }
}
