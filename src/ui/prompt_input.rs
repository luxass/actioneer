use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::ui::prompt::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Up,
    Down,
    Toggle,
    ToggleAll,
    ToggleFile,
    ToggleCollapse,
    PageUp,
    PageDown,
    Home,
    End,
    Invert,
    SelectNone,
    Accept,
    Cancel,
    Resize,
    ScrollLeft,
    ScrollRight,
    Ignore,
}

pub(crate) trait EventSource {
    fn next_event(&mut self) -> std::io::Result<Event>;
}

pub struct RealEventSource;

impl EventSource for RealEventSource {
    fn next_event(&mut self) -> std::io::Result<Event> {
        event::read()
    }
}

#[cfg(test)]
pub struct TestEventSource {
    events: std::vec::IntoIter<Event>,
}

#[cfg(test)]
impl TestEventSource {
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events: events.into_iter(),
        }
    }
}

#[cfg(test)]
impl EventSource for TestEventSource {
    fn next_event(&mut self) -> std::io::Result<Event> {
        self.events
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "no more events"))
    }
}

pub fn read_key<E: EventSource>(event_source: &mut E) -> Result<Key, Error> {
    loop {
        let event = event_source.next_event()?;
        match event {
            Event::Key(key_event) => {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }

                return Ok(match key_event.code {
                    KeyCode::Up | KeyCode::Char('k') => Key::Up,
                    KeyCode::Down | KeyCode::Char('j') => Key::Down,
                    KeyCode::Left => Key::ScrollLeft,
                    KeyCode::Right => Key::ScrollRight,
                    KeyCode::Enter => Key::Accept,
                    KeyCode::Char(' ') | KeyCode::Char('x') => Key::Toggle,
                    KeyCode::Char('a') => Key::ToggleAll,
                    KeyCode::Char('f') => Key::ToggleFile,
                    KeyCode::Tab => Key::ToggleCollapse,
                    KeyCode::PageUp => Key::PageUp,
                    KeyCode::PageDown => Key::PageDown,
                    KeyCode::Home => Key::Home,
                    KeyCode::End => Key::End,
                    KeyCode::Char('i') => Key::Invert,
                    KeyCode::Char('n') => Key::SelectNone,
                    KeyCode::Char('q') | KeyCode::Esc => Key::Cancel,
                    KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Err(Error::Interrupted);
                    }
                    _ => Key::Ignore,
                });
            }
            Event::Resize(_, _) => return Ok(Key::Resize),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::*;

    fn make_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn char_event(c: char) -> Event {
        make_event(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn special_event(code: KeyCode) -> Event {
        make_event(code, KeyModifiers::NONE)
    }

    #[test]
    fn read_key_maps_up() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Up)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Up);
    }

    #[test]
    fn read_key_maps_down() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Down)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Down);
    }

    #[test]
    fn read_key_maps_left() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Left)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::ScrollLeft);
    }

    #[test]
    fn read_key_maps_right() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Right)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::ScrollRight);
    }

    #[test]
    fn read_key_maps_j_to_down() {
        let mut src = TestEventSource::new(vec![char_event('j')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Down);
    }

    #[test]
    fn read_key_maps_k_to_up() {
        let mut src = TestEventSource::new(vec![char_event('k')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Up);
    }

    #[test]
    fn read_key_maps_enter_to_accept() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Enter)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Accept);
    }

    #[test]
    fn read_key_maps_space_to_toggle() {
        let mut src = TestEventSource::new(vec![char_event(' ')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Toggle);
    }

    #[test]
    fn read_key_maps_x_to_toggle() {
        let mut src = TestEventSource::new(vec![char_event('x')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Toggle);
    }

    #[test]
    fn read_key_maps_a_to_toggle_all() {
        let mut src = TestEventSource::new(vec![char_event('a')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::ToggleAll);
    }

    #[test]
    fn read_key_maps_f_to_toggle_file() {
        let mut src = TestEventSource::new(vec![char_event('f')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::ToggleFile);
    }

    #[test]
    fn read_key_maps_tab_to_collapse() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Tab)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::ToggleCollapse);
    }

    #[test]
    fn read_key_maps_page_up() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::PageUp)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::PageUp);
    }

    #[test]
    fn read_key_maps_page_down() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::PageDown)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::PageDown);
    }

    #[test]
    fn read_key_maps_home() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Home)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Home);
    }

    #[test]
    fn read_key_maps_end() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::End)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::End);
    }

    #[test]
    fn read_key_maps_i_to_invert() {
        let mut src = TestEventSource::new(vec![char_event('i')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Invert);
    }

    #[test]
    fn read_key_maps_n_to_select_none() {
        let mut src = TestEventSource::new(vec![char_event('n')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::SelectNone);
    }

    #[test]
    fn read_key_maps_q_to_cancel() {
        let mut src = TestEventSource::new(vec![char_event('q')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Cancel);
    }

    #[test]
    fn read_key_maps_esc_to_cancel() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Esc)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Cancel);
    }

    #[test]
    fn read_key_maps_ctrl_c_to_interrupted() {
        let mut src =
            TestEventSource::new(vec![make_event(KeyCode::Char('c'), KeyModifiers::CONTROL)]);
        let result = read_key(&mut src);
        assert!(matches!(result, Err(Error::Interrupted)));
    }

    #[test]
    fn read_key_ignores_unknown_keys() {
        let mut src = TestEventSource::new(vec![
            char_event('z'),
            special_event(KeyCode::F(1)),
            char_event('y'),
            char_event(' '),
        ]);
        read_key(&mut src).unwrap();
        read_key(&mut src).unwrap();
        read_key(&mut src).unwrap();
        assert_eq!(read_key(&mut src).unwrap(), Key::Toggle);
    }
}
