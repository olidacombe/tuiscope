//! # TuiScope
//!
//! Inspired by [telescope](https://github.com/nvim-telescope/telescope.nvim).
//!
//! A TUI fuzzy finder for rust apps. For example usage, see [examples](https://github.com/olidacombe/tuiscope/tree/main/examples) for usage.
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use itertools::Itertools;
use std::{cmp::Ordering, collections::HashMap, marker::PhantomData};
use tui::{
    prelude::*,
    widgets::{Block, List, ListItem, ListState, StatefulWidget},
};

#[derive(Default)]
pub struct FuzzyList<'a, K> {
    block: Option<Block<'a>>,
    matched_char_style: Style,
    selection_highlight_style: Style,
    unmatched_char_style: Style,
    _key_type: PhantomData<K>,
}

impl<'a, K> FuzzyList<'a, K> {
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn matched_char_style(mut self, style: Style) -> Self {
        self.matched_char_style = style;
        self
    }

    pub fn selection_highlight_style(mut self, style: Style) -> Self {
        self.selection_highlight_style = style;
        self
    }

    fn styled_line(&self, entry: &'a FuzzyListEntry<K>) -> Line {
        let raw = &entry.v;
        Line::from(
            highlight_sections_from_stringdices(raw, &entry.indices)
                .iter()
                .map(|section| match section {
                    HighlightStyle::None(sub) => Span::styled(*sub, self.unmatched_char_style),
                    HighlightStyle::Matched(sub) => Span::styled(*sub, self.matched_char_style),
                })
                .collect::<Vec<Span>>(),
        )
    }
}

#[derive(Clone)]
pub struct FuzzyListEntry<K> {
    // key of entry
    pub k: K,
    // value of entry
    pub v: String,
    // fuzzy match score
    pub score: i64,
    // fuzzy match indices (positions in `v`)
    pub indices: Vec<usize>,
}

impl<K> Ord for FuzzyListEntry<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score.cmp(&other.score)
    }
}

impl<K> PartialOrd for FuzzyListEntry<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K> PartialEq for FuzzyListEntry<K> {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl<K> Eq for FuzzyListEntry<K> {}

#[derive(Default)]
pub struct FuzzyFinder<K> {
    options: HashMap<K, String>,
    filter: String,
    filtered_list: Vec<FuzzyListEntry<K>>,
    state: ListState,
}

impl<K> FuzzyFinder<K>
where
    K: Copy,
{
    pub fn clear_filter(&mut self) -> &mut Self {
        self.filter = String::new();
        self.update_filtered_list();
        self
    }

    fn reset_selection(&mut self) -> &mut Self {
        if !self.filtered_list.is_empty() {
            self.state.select(Some(0));
        } else {
            self.state.select(None);
        }
        self
    }

    pub fn select_next(&mut self) -> &mut Self {
        if let Some(current) = self.state.selected() {
            self.select(current + 1);
        } else {
            self.reset_selection();
        }
        self
    }

    pub fn select_prev(&mut self) -> &mut Self {
        if let Some(current) = self.state.selected() {
            if current > 0 {
                self.select(current - 1);
            }
        } else {
            self.reset_selection();
        }
        self
    }

    fn select(&mut self, index: usize) -> &mut Self {
        let len = self.filtered_list.len();
        if len < 1 {
            return self.reset_selection();
        }
        self.state.select(Some(std::cmp::min(index, len - 1)));
        self
    }

    pub fn selection(&self) -> Option<FuzzyListEntry<K>> {
        self.state
            .selected()
            .and_then(|i| self.filtered_list.get(i).cloned())
    }

    pub fn set_filter(&mut self, filter: String) -> &mut Self {
        self.filter = filter;
        self.update_filtered_list();
        self
    }

    pub fn set_options(&mut self, options: HashMap<K, String>) -> &mut Self {
        self.options = options;
        self.update_filtered_list();
        self
    }

    fn update_filtered_list(&mut self) {
        let matcher = SkimMatcherV2::default();
        self.filtered_list = self
            .options
            .iter()
            .filter_map(|(k, v)| {
                matcher
                    .fuzzy_indices(v, &self.filter)
                    .map(|(score, indices)| FuzzyListEntry::<K> {
                        k: *k,
                        v: v.to_string(),
                        score,
                        indices,
                    })
            })
            .sorted()
            .rev()
            .collect();
        // TODO only if some change
        self.reset_selection();
    }
}

impl<'a, K> StatefulWidget for FuzzyList<'a, K> {
    type State = FuzzyFinder<K>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list: Vec<ListItem> = state
            .filtered_list
            .iter()
            .map(|entry| ListItem::new(self.styled_line(entry)))
            .collect();
        let mut list = List::new(list)
            .highlight_style(self.selection_highlight_style)
            .highlight_symbol("> ");
        if let Some(ref block) = self.block {
            list = list.block(block.clone());
        }
        StatefulWidget::render(list, area, buf, &mut state.state);
    }
}

#[derive(Debug, PartialEq)]
enum HighlightStyle<'a> {
    None(&'a str),
    Matched(&'a str),
}

fn highlight_sections_from_stringdices<'a>(
    string: &'a str,
    indices: &'a [usize],
) -> Vec<HighlightStyle<'a>> {
    let mut ret = Vec::new();
    let mut indices = indices.iter().peekable();
    let mut i: usize = 0;
    while let Some(m) = indices.next() {
        if *m > 0 {
            let sub = string.get(i..*m).unwrap();
            ret.push(HighlightStyle::None(sub));
        }
        i = *m;
        let mut j = *m + 1;
        while let Some(m) = indices.peek() {
            if *m > &j {
                break;
            }
            indices.next();
            j += 1;
        }
        let sub = string.get(i..j).unwrap();
        ret.push(HighlightStyle::Matched(sub));
        i = j;
    }
    if i < string.len() {
        let sub = string.get(i..).unwrap();
        ret.push(HighlightStyle::None(sub));
    }
    ret
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn no_highlight() {
        assert_eq!(
            highlight_sections_from_stringdices("abc", &Vec::new()),
            vec![HighlightStyle::None("abc")]
        );
    }

    #[test]
    fn highlight_one_char_at_start() {
        assert_eq!(
            highlight_sections_from_stringdices("abc", &[0]),
            vec![HighlightStyle::Matched("a"), HighlightStyle::None("bc")]
        );
    }

    #[test]
    fn highlight_one_char_at_end() {
        assert_eq!(
            highlight_sections_from_stringdices("abc", &[2]),
            vec![HighlightStyle::None("ab"), HighlightStyle::Matched("c")]
        );
    }

    #[test]
    fn highlight_three_char_at_start() {
        assert_eq!(
            highlight_sections_from_stringdices("abcde", &[0, 1, 2]),
            vec![HighlightStyle::Matched("abc"), HighlightStyle::None("de")]
        );
    }

    #[test]
    fn highlight_three_char_at_end() {
        assert_eq!(
            highlight_sections_from_stringdices("abcde", &[2, 3, 4]),
            vec![HighlightStyle::None("ab"), HighlightStyle::Matched("cde")]
        );
    }

    #[test]
    fn highlight_fun_mixture_one() {
        assert_eq!(
            highlight_sections_from_stringdices("abcdefghijk", &[1, 2, 5, 6, 7, 9]),
            vec![
                HighlightStyle::None("a"),
                HighlightStyle::Matched("bc"),
                HighlightStyle::None("de"),
                HighlightStyle::Matched("fgh"),
                HighlightStyle::None("i"),
                HighlightStyle::Matched("j"),
                HighlightStyle::None("k")
            ]
        )
    }
}
