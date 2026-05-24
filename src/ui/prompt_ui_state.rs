use std::collections::HashSet;

use crate::model::ResolvedUpdate;

#[derive(Debug, PartialEq)]
pub enum VisibleRow {
    FileHeader { file: String },
    Update { original_index: usize },
}

pub struct PromptState {
    pub selected: Vec<bool>,
    pub collapsed: HashSet<String>,
    pub cursor: usize,
    pub h_scroll: usize,
}

impl PromptState {
    pub fn new(count: usize) -> Self {
        Self {
            selected: vec![false; count],
            collapsed: HashSet::new(),
            cursor: 0,
            h_scroll: 0,
        }
    }

    pub fn clamp_cursor(&mut self, visible_rows: &[VisibleRow]) {
        if self.cursor >= visible_rows.len() {
            self.cursor = visible_rows.len().saturating_sub(1);
        }
    }

    pub fn move_up(&mut self, visible_rows: &[VisibleRow]) {
        if self.cursor > 0 {
            self.cursor -= 1;
        } else {
            self.cursor = visible_rows.len() - 1;
        }
    }

    pub fn move_down(&mut self, visible_rows: &[VisibleRow]) {
        if self.cursor + 1 < visible_rows.len() {
            self.cursor += 1;
        } else {
            self.cursor = 0;
        }
    }

    pub fn toggle_at(&mut self, visible_rows: &[VisibleRow], updates: &[ResolvedUpdate]) {
        match &visible_rows[self.cursor] {
            VisibleRow::FileHeader { file } => self.toggle_file(updates, file),
            VisibleRow::Update { original_index } => {
                self.selected[*original_index] = !self.selected[*original_index];
            }
        }
    }

    pub fn toggle_all(&mut self) {
        let all_selected = self.selected.iter().all(|s| *s);
        self.selected.fill(!all_selected);
    }

    pub fn toggle_file_at_cursor(
        &mut self,
        visible_rows: &[VisibleRow],
        updates: &[ResolvedUpdate],
    ) {
        let file = file_at_cursor(visible_rows, self.cursor, updates);
        self.toggle_file(updates, &file);
    }

    fn toggle_file(&mut self, updates: &[ResolvedUpdate], file: &str) {
        let all_selected = updates
            .iter()
            .zip(self.selected.iter())
            .filter(|(update, _)| update.file() == file)
            .all(|(_, s)| *s);

        for (update, s) in updates.iter().zip(self.selected.iter_mut()) {
            if update.file() == file {
                *s = !all_selected;
            }
        }
    }

    pub fn toggle_collapse(&mut self, visible_rows: &[VisibleRow], updates: &[ResolvedUpdate]) {
        let file = file_at_cursor(visible_rows, self.cursor, updates);
        if self.collapsed.contains(&file) {
            self.collapsed.remove(&file);
        } else {
            self.collapsed.insert(file);
        }
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.cursor = self.cursor.saturating_sub(page_size);
    }

    pub fn page_down(&mut self, page_size: usize, visible_rows: &[VisibleRow]) {
        self.cursor = (self.cursor + page_size).min(visible_rows.len() - 1);
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self, visible_rows: &[VisibleRow]) {
        self.cursor = visible_rows.len() - 1;
    }

    pub fn invert(&mut self) {
        for s in &mut self.selected {
            *s = !*s;
        }
    }

    pub fn select_none(&mut self) {
        self.selected.fill(false);
    }

    pub fn scroll_left(&mut self) {
        self.h_scroll = self.h_scroll.saturating_sub(4);
    }

    pub fn scroll_right(&mut self) {
        self.h_scroll += 4;
    }

    pub fn selected_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.selected
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (*s).then_some(i))
    }

    pub fn selected_count(&self) -> usize {
        self.selected.iter().filter(|s| **s).count()
    }
}

pub fn build_visible_rows(
    updates: &[ResolvedUpdate],
    collapsed: &HashSet<String>,
) -> Vec<VisibleRow> {
    let mut rows = Vec::new();
    let mut last_file: Option<&str> = None;

    for (index, update) in updates.iter().enumerate() {
        let file = update.file();
        let is_first = last_file != Some(file);

        if is_first {
            rows.push(VisibleRow::FileHeader {
                file: file.to_string(),
            });
            if !collapsed.contains(file) {
                rows.push(VisibleRow::Update {
                    original_index: index,
                });
            }
        } else if !collapsed.contains(file) {
            rows.push(VisibleRow::Update {
                original_index: index,
            });
        }

        last_file = Some(file);
    }

    rows
}

fn file_at_cursor(
    visible_rows: &[VisibleRow],
    cursor: usize,
    updates: &[ResolvedUpdate],
) -> String {
    if cursor >= visible_rows.len() {
        return updates[0].file().to_string();
    }
    match &visible_rows[cursor] {
        VisibleRow::FileHeader { file } => file.clone(),
        VisibleRow::Update { original_index } => updates[*original_index].file().to_string(),
    }
}

pub fn file_selection_counts(
    updates: &[ResolvedUpdate],
    selected: &[bool],
    file: &str,
) -> (usize, usize) {
    updates
        .iter()
        .zip(selected.iter())
        .filter(|(update, _)| update.file() == file)
        .fold((0, 0), |(sel, total), (_, s)| {
            (sel + usize::from(*s), total + 1)
        })
}

#[cfg(test)]
mod tests {
    use crate::model::{ResolvedUpdate, UpdateSource, UpdateTarget, ValidationState};

    use super::*;

    fn make_update(file: &str, action: &str) -> ResolvedUpdate {
        ResolvedUpdate::new(
            action,
            "build",
            "v1.0.0",
            ValidationState::new("abc1234", "1.0.0", false),
            UpdateTarget::new("v2.0.0", "v2.0.0", false),
            UpdateSource::new(file, 10, 20, 30),
            false,
        )
    }

    #[test]
    fn build_visible_rows_single_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
        ];
        let collapsed = HashSet::new();
        let rows = build_visible_rows(&updates, &collapsed);

        assert_eq!(rows.len(), 3);
        assert!(matches!(&rows[0], VisibleRow::FileHeader { file } if file == "a.yml"));
        assert!(matches!(&rows[1], VisibleRow::Update { original_index } if *original_index == 0));
        assert!(matches!(&rows[2], VisibleRow::Update { original_index } if *original_index == 1));
    }

    #[test]
    fn build_visible_rows_multiple_files() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("b.yml", "actions/setup-node"),
        ];
        let collapsed = HashSet::new();
        let rows = build_visible_rows(&updates, &collapsed);

        assert_eq!(rows.len(), 4);
        assert!(matches!(&rows[0], VisibleRow::FileHeader { file } if file == "a.yml"));
        assert!(matches!(&rows[1], VisibleRow::Update { original_index } if *original_index == 0));
        assert!(matches!(&rows[2], VisibleRow::FileHeader { file } if file == "b.yml"));
        assert!(matches!(&rows[3], VisibleRow::Update { original_index } if *original_index == 1));
    }

    #[test]
    fn build_visible_rows_collapsed_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
        ];
        let mut collapsed = HashSet::new();
        collapsed.insert("a.yml".to_string());
        let rows = build_visible_rows(&updates, &collapsed);

        assert_eq!(rows.len(), 1);
        assert!(matches!(&rows[0], VisibleRow::FileHeader { file } if file == "a.yml"));
    }

    #[test]
    fn invert_selected_flips_all() {
        let mut state = PromptState::new(3);
        state.selected = vec![true, false, true];
        state.invert();
        assert_eq!(state.selected, vec![false, true, false]);
    }

    #[test]
    fn toggle_all_selects_when_none_selected() {
        let mut state = PromptState::new(3);
        state.toggle_all();
        assert_eq!(state.selected, vec![true, true, true]);
    }

    #[test]
    fn toggle_all_deselects_when_all_selected() {
        let mut state = PromptState::new(3);
        state.selected = vec![true, true, true];
        state.toggle_all();
        assert_eq!(state.selected, vec![false, false, false]);
    }

    #[test]
    fn toggle_all_selects_when_partial() {
        let mut state = PromptState::new(3);
        state.selected = vec![true, false, true];
        state.toggle_all();
        assert_eq!(state.selected, vec![true, true, true]);
    }

    #[test]
    fn toggle_file_selects_all_in_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
            make_update("b.yml", "actions/cache"),
        ];
        let mut state = PromptState::new(3);
        state.toggle_file(&updates, "a.yml");
        assert_eq!(state.selected, vec![true, true, false]);
    }

    #[test]
    fn toggle_file_deselects_all_in_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
            make_update("b.yml", "actions/cache"),
        ];
        let mut state = PromptState::new(3);
        state.selected = vec![true, true, false];
        state.toggle_file(&updates, "a.yml");
        assert_eq!(state.selected, vec![false, false, false]);
    }

    #[test]
    fn file_selection_counts_counts_selected_entries_in_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
            make_update("b.yml", "actions/cache"),
        ];
        let selected = vec![true, false, true];

        assert_eq!(file_selection_counts(&updates, &selected, "a.yml"), (1, 2));
        assert_eq!(file_selection_counts(&updates, &selected, "b.yml"), (1, 1));
    }

    #[test]
    fn selected_indices_returns_selected_positions() {
        let mut state = PromptState::new(4);
        state.selected = vec![false, true, false, true];
        assert_eq!(state.selected_indices().collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn selected_count_returns_correct_total() {
        let mut state = PromptState::new(4);
        state.selected = vec![false, true, false, true];
        assert_eq!(state.selected_count(), 2);
    }
}
