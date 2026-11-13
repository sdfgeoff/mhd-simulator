//! Bounded recorded outputs shared by analysis backends. Times are stored, never interpolated.
pub struct Sample<T> {
    pub time_s: f64,
    pub bytes: u64,
    pub output: T,
}
pub struct Timeline<T> {
    pub samples: Vec<Sample<T>>,
    pub selected: usize,
    pub capacity_frames: usize,
    pub dropped: usize,
}
impl<T> Default for Timeline<T> {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            selected: 0,
            capacity_frames: 10,
            dropped: 0,
        }
    }
}
impl<T> Timeline<T> {
    pub fn clear(&mut self) {
        self.samples.clear();
        self.selected = 0;
        self.dropped = 0;
    }
    pub fn current(&self) -> Option<&T> {
        self.samples.get(self.selected).map(|s| &s.output)
    }
    pub fn push(&mut self, sample: Sample<T>) {
        let follow = self.samples.is_empty() || self.selected + 1 == self.samples.len();
        // A repeated timestamp replaces the output at that time. A restarted run clears history.
        if self
            .samples
            .last()
            .is_some_and(|s| sample.time_s < s.time_s)
        {
            self.clear();
        }
        if self
            .samples
            .last()
            .is_some_and(|s| sample.time_s == s.time_s)
        {
            self.samples.pop();
        }
        self.samples.push(sample);
        while self.samples.len() > self.capacity_frames.max(1) {
            self.samples.remove(0);
            self.selected = self.selected.saturating_sub(1);
            self.dropped += 1;
        }
        if follow {
            self.selected = self.samples.len() - 1;
        }
        self.selected = self.selected.min(self.samples.len() - 1);
    }
    pub fn bytes(&self) -> u64 {
        self.samples.iter().map(|s| s.bytes).sum()
    }
    pub fn controls(&mut self, ui: &mut eframe::egui::Ui) -> bool {
        let old = self.selected;
        let old_time = self.samples.get(self.selected).map(|s| s.time_s);
        ui.horizontal(|ui| {
            ui.strong("Timeline");
            if self.samples.is_empty() {
                ui.label("Compute an analysis to record results");
                return;
            }
            if ui
                .button("|◀")
                .on_hover_text("First retained result")
                .clicked()
            {
                self.selected = 0;
            }
            if ui.button("◀").clicked() {
                self.selected = self.selected.saturating_sub(1);
            }
            if ui.button("▶").clicked() {
                self.selected = (self.selected + 1).min(self.samples.len() - 1);
            }
            if ui
                .button("▶|")
                .on_hover_text("Latest result; follow new results")
                .clicked()
            {
                self.selected = self.samples.len() - 1;
            }
            let last = self.samples.len() - 1;
            ui.scope(|ui| {
                ui.spacing_mut().slider_width = (ui.available_width() - 220.).max(80.);
                ui.add(eframe::egui::Slider::new(&mut self.selected, 0..=last).show_value(false));
            });
            ui.label(format!(
                "{:.3} µs · {}/{}",
                self.samples[self.selected].time_s * 1e6,
                self.selected + 1,
                self.samples.len()
            ));
        });
        ui.horizontal(|ui| {
            ui.label("History window");
            if ui
                .add(
                    eframe::egui::DragValue::new(&mut self.capacity_frames)
                        .range(1..=100)
                        .suffix(" frames"),
                )
                .changed()
            {
                while self.samples.len() > self.capacity_frames {
                    self.samples.remove(0);
                    self.selected = self.selected.saturating_sub(1);
                    self.dropped += 1;
                }
            }
        });
        if !self.samples.is_empty() {
            ui.small(format!("Recorded outputs · {:.1} MiB · {}-frame window · {} older frames discarded · stepping continues from latest solver time", self.bytes() as f64/1048576., self.capacity_frames, self.dropped));
        }
        old != self.selected || old_time != self.samples.get(self.selected).map(|s| s.time_s)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headless_pointer_scrubs_the_full_width_timeline() {
        use eframe::egui;
        let mut timeline = Timeline::default();
        for i in 0..9 {
            timeline.push(Sample {
                time_s: i as f64,
                bytes: 1,
                output: i,
            });
        }
        let context = egui::Context::default();
        let point = egui::pos2(300., 18.);
        for events in [
            vec![],
            vec![egui::Event::PointerMoved(point)],
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ] {
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 200.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        timeline.controls(ui);
                    });
                },
            );
        }
        assert!(
            (1..4).contains(&timeline.selected),
            "selected {}",
            timeline.selected
        );
        assert_eq!(timeline.current(), Some(&(timeline.selected as i32)));
    }
    #[test]
    fn default_window_retains_ten_frames_independent_of_bytes() {
        let mut timeline = Timeline::default();
        for i in 0..11 {
            timeline.push(Sample {
                time_s: i as f64,
                bytes: 128 * 1024 * 1024,
                output: i,
            });
        }
        assert_eq!(timeline.samples.len(), 10);
        assert_eq!(timeline.samples[0].output, 1);
        assert_eq!(timeline.current(), Some(&10));
        assert_eq!(timeline.bytes(), 1280 * 1024 * 1024);
    }
    #[test]
    fn retention_selection_and_restart() {
        let mut t = Timeline::default();
        t.capacity_frames = 3;
        for i in 0..3 {
            t.push(Sample {
                time_s: i as f64,
                bytes: 1,
                output: i,
            });
        }
        assert_eq!(t.current(), Some(&2));
        t.selected = 0;
        t.push(Sample {
            time_s: 3.,
            bytes: 1,
            output: 3,
        });
        assert_eq!(t.current(), Some(&1));
        assert_eq!(t.dropped, 1);
        t.push(Sample {
            time_s: 3.,
            bytes: 1,
            output: 30,
        });
        assert_eq!(t.samples.len(), 3);
        t.push(Sample {
            time_s: 0.,
            bytes: 1,
            output: 0,
        });
        assert_eq!(t.samples.len(), 1);
        assert_eq!(t.current(), Some(&0));
    }
}
