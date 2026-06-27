use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use crossterm::event::{self, Event as CxEvent, KeyEvent};

pub enum Event {
    Tick,
    Key(KeyEvent),
    Resize(u16, u16),
}

pub struct EventHandler {
    rx: mpsc::Receiver<Event>,
}

impl EventHandler {
    /// Spawn a background thread that polls crossterm events and sends ticks at
    /// `tick_ms` millisecond intervals.
    pub fn new(tick_ms: u64) -> Self {
        let (tx, rx) = mpsc::channel();
        let rate = Duration::from_millis(tick_ms);

        thread::spawn(move || {
            let mut last_tick = Instant::now();
            loop {
                let timeout = rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or(Duration::ZERO);

                if event::poll(timeout).unwrap_or(false)
                    && let Ok(ev) = event::read()
                {
                    let msg = match ev {
                        CxEvent::Key(k) => Some(Event::Key(k)),
                        CxEvent::Resize(w, h) => Some(Event::Resize(w, h)),
                        _ => None,
                    };
                    if let Some(m) = msg
                        && tx.send(m).is_err()
                    {
                        break;
                    }
                }

                if last_tick.elapsed() >= rate {
                    if tx.send(Event::Tick).is_err() {
                        break;
                    }
                    last_tick = Instant::now();
                }
            }
        });

        Self { rx }
    }

    /// Block until the next event is available. Returns `None` when the sender
    /// thread has exited (channel closed).
    pub fn next(&self) -> Option<Event> {
        self.rx.recv().ok()
    }
}
