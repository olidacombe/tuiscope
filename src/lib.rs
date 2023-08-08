//! # TuiScope
//!
//! Inspired by [telescope](https://github.com/nvim-telescope/telescope.nvim).
//!
//! A TUI fuzzy finder for rust apps. For example usage, see [examples](https://github.com/olidacombe/tuiscope/tree/main/examples).
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use itertools::Itertools;
use std::{
    cmp::Ordering,
    collections::HashMap,
    marker::PhantomData,
    ops::{Bound, RangeBounds},
    slice::SliceIndex,
};
use thiserror::Error;
use tracing::error;
use tui::{
    prelude::*,
    widgets::{Block, List, ListItem, ListState, StatefulWidget},
};

/// Ephemeral list widget for fuzzy matched items.
/// Highlights selected line and matched chars.
/// Orders items by match score.
///
/// # Example
///
/// ```
/// use tui::prelude::*;
/// use tui::widgets::*;
/// use tuiscope::FuzzyList;
///
/// let fuzzy_results = FuzzyList::<u32>::default()
///     .block(Block::default().borders(Borders::ALL).title("Matches"))
///     .matched_char_style(Style::default().fg(Color::Cyan))
///     .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
/// ```
#[derive(Default)]
pub struct FuzzyList<'a, K> {
    block: Option<Block<'a>>,
    matched_char_style: Style,
    selection_highlight_style: Style,
    unmatched_char_style: Style,
    _key_type: PhantomData<K>,
}

impl<'a, K> FuzzyList<'a, K> {
    /// Builder method to add a block specification to a `FuzzyList`
    ///
    /// # Example
    ///
    /// ```
    /// use tui::widgets::*;
    /// use tuiscope::FuzzyList;
    ///
    /// let fuzzy: FuzzyList<u32> = FuzzyList::default().block( Block::default().borders(Borders::ALL).title("Matches"));
    /// ```
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Builder method to set style for matched characters in fuzzy search
    ///
    /// # Example
    ///
    /// ```
    /// use tui::widgets::*;
    /// use tuiscope::FuzzyList;
    ///
    /// let fuzzy: FuzzyList<u32> = FuzzyList::default().block( Block::default().borders(Borders::ALL).title("Matches"));
    /// ```
    pub fn matched_char_style(mut self, style: Style) -> Self {
        self.matched_char_style = style;
        self
    }

    /// Builder method to set style for selected item in filtered fuzzy list
    ///
    /// # Example
    ///
    /// ```
    /// use tui::prelude::*;
    /// use tui::widgets::*;
    /// use tuiscope::FuzzyList;
    ///
    /// let fuzzy_results = FuzzyList::<u32>::default()
    ///     .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
    /// ```
    pub fn selection_highlight_style(mut self, style: Style) -> Self {
        self.selection_highlight_style = style;
        self
    }

    fn styled_line(&self, entry: &'a FuzzyListEntry<K>) -> Result<Line, MatchHighlightError> {
        let raw = &entry.v;
        Ok(Line::from(
            highlight_sections_from_stringdices(raw, &entry.indices)?
                .iter()
                .map(|section| match section {
                    HighlightStyle::None(sub) => Span::styled(*sub, self.unmatched_char_style),
                    HighlightStyle::Matched(sub) => Span::styled(*sub, self.matched_char_style),
                })
                .collect::<Vec<Span>>(),
        ))
    }
}

/// Return type for `FuzzyFinder<K>::selection`
#[derive(Clone)]
pub struct FuzzyListEntry<'a, K> {
    /// key of entry
    pub k: K,
    /// value of entry
    pub v: &'a str,
    /// fuzzy match score
    pub score: i64,
    /// fuzzy match indices (positions in `v`)
    pub indices: Vec<usize>,
}

impl<'a, K> Ord for FuzzyListEntry<'a, K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score.cmp(&other.score)
    }
}

impl<'a, K> PartialOrd for FuzzyListEntry<'a, K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a, K> PartialEq for FuzzyListEntry<'a, K> {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl<'a, K> Eq for FuzzyListEntry<'a, K> {}

/// State for `FuzzyList<K>`.  Hold on to one of these and pass to `render_stateful_widget`
///
/// # Example
///
/// ```
/// use tui::prelude::*;
/// use tui::widgets::*;
/// use tuiscope::{FuzzyFinder, FuzzyList};
///
/// fn ui<B: Backend>(f: &mut Frame<B>, state: &mut FuzzyFinder<u32>) {
///     let chunks = Layout::default()
///         .direction(Direction::Vertical)
///         .constraints([Constraint::Min(1)].as_ref())
///         .split(f.size());
///
///     let fuzzy_results = FuzzyList::<u32>::default()
///         .block(Block::default().borders(Borders::ALL).title("Options"))
///         .matched_char_style(Style::default().fg(Color::Cyan))
///         .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
///     f.render_stateful_widget(fuzzy_results, chunks[2], state);
/// }
/// ```
pub struct FuzzyFinder<'a, K> {
    /// The space of options to search.
    options: &'a HashMap<K, String>,
    /// The current filter string.
    filter: String,
    /// The list of filtered entries, ordered by score.
    filtered_list: Vec<FuzzyListEntry<'a, K>>,
    /// State for the `FuzzyList` widget's selection.
    pub state: ListState,
}

impl<'a, K> FuzzyFinder<'a, K>
where
    K: Copy + Eq + std::hash::Hash,
{
    pub fn new(options: &'a HashMap<K, String>) -> Self {
        Self {
            options,
            filter: String::default(),
            filtered_list: Vec::new(),
            state: ListState::default(),
        }
    }
    /// Clears the filter term.
    pub fn clear_filter(&mut self) -> &mut Self {
        self.filter = String::new();
        self.update_filtered_list();
        self
    }

    /// Resets the selected line from filtered options to the 0th.
    fn reset_selection(&mut self) -> &mut Self {
        if !self.filtered_list.is_empty() {
            self.state.select(Some(0));
        } else {
            self.state.select(None);
        }
        self
    }

    /// Select the next filtered entry.
    pub fn select_next(&mut self) -> &mut Self {
        if let Some(current) = self.state.selected() {
            self.select(current + 1);
        } else {
            self.reset_selection();
        }
        self
    }

    /// Select the previous filtered entry.
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

    /// Get the current selected entry.
    pub fn selection(&self) -> Option<FuzzyListEntry<K>> {
        self.state
            .selected()
            .and_then(|i| self.filtered_list.get(i).cloned())
    }

    /// Updates the filter term.
    pub fn set_filter(&mut self, filter: String) -> &mut Self {
        self.filter = filter;
        self.update_filtered_list();
        self
    }

    /// Sets the space of options to search.
    pub fn set_options(&mut self, options: &'a HashMap<K, String>) -> &mut Self {
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
                        v,
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

impl<'a, K: 'a> StatefulWidget for FuzzyList<'a, K> {
    type State = FuzzyFinder<'a, K>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list: Vec<ListItem> = state
            .filtered_list
            .iter()
            .filter_map(|entry| self.styled_line(entry).ok())
            .take(area.height as usize + state.state.selected().unwrap_or(0))
            .map(ListItem::new)
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

#[derive(Error, Debug)]
pub enum MatchHighlightError {
    #[error("substring {start:?}:{end:?} not found in {string}")]
    SubstringNotFound {
        start: Bound<usize>,
        end: Bound<usize>,
        string: String,
    },
}

fn get_substring<R: RangeBounds<usize> + SliceIndex<str> + Clone>(
    string: &str,
    range: R,
) -> Result<&<R as SliceIndex<str>>::Output, MatchHighlightError> {
    string.get(range.clone()).ok_or_else(|| {
        let start = range.start_bound().cloned();
        let end = range.end_bound().cloned();
        error! {"Invalid range {:?}:{:?} in `{}`", start, end, string};
        MatchHighlightError::SubstringNotFound {
            start,
            end,
            string: string.into(),
        }
    })
}

fn highlight_sections_from_stringdices<'a>(
    string: &'a str,
    indices: &'a [usize],
) -> Result<Vec<HighlightStyle<'a>>, MatchHighlightError> {
    let mut ret = Vec::new();
    let mut indices = indices.iter().peekable();
    let mut i: usize = 0;
    while let Some(m) = indices.next() {
        if *m > 0 {
            let sub = get_substring(string, i..*m)?;
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
        let sub = get_substring(string, i..j)?;
        ret.push(HighlightStyle::Matched(sub));
        i = j;
    }
    if i < string.len() {
        let sub = get_substring(string, i..)?;
        ret.push(HighlightStyle::None(sub));
    }
    Ok(ret)
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;

    #[test]
    fn no_highlight() -> Result<()> {
        assert_eq!(
            highlight_sections_from_stringdices("abc", &Vec::new())?,
            vec![HighlightStyle::None("abc")]
        );
        Ok(())
    }

    #[test]
    fn highlight_one_char_at_start() -> Result<()> {
        assert_eq!(
            highlight_sections_from_stringdices("abc", &[0])?,
            vec![HighlightStyle::Matched("a"), HighlightStyle::None("bc")]
        );
        Ok(())
    }

    #[test]
    fn highlight_one_char_at_end() -> Result<()> {
        assert_eq!(
            highlight_sections_from_stringdices("abc", &[2])?,
            vec![HighlightStyle::None("ab"), HighlightStyle::Matched("c")]
        );
        Ok(())
    }

    #[test]
    fn highlight_three_char_at_start() -> Result<()> {
        assert_eq!(
            highlight_sections_from_stringdices("abcde", &[0, 1, 2])?,
            vec![HighlightStyle::Matched("abc"), HighlightStyle::None("de")]
        );
        Ok(())
    }

    #[test]
    fn highlight_three_char_at_end() -> Result<()> {
        assert_eq!(
            highlight_sections_from_stringdices("abcde", &[2, 3, 4])?,
            vec![HighlightStyle::None("ab"), HighlightStyle::Matched("cde")]
        );
        Ok(())
    }

    #[test]
    fn highlight_fun_mixture_one() -> Result<()> {
        assert_eq!(
            highlight_sections_from_stringdices("abcdefghijk", &[1, 2, 5, 6, 7, 9])?,
            vec![
                HighlightStyle::None("a"),
                HighlightStyle::Matched("bc"),
                HighlightStyle::None("de"),
                HighlightStyle::Matched("fgh"),
                HighlightStyle::None("i"),
                HighlightStyle::Matched("j"),
                HighlightStyle::None("k")
            ]
        );
        Ok(())
    }

    // These 2 bugs occurred when starting a search with 's'
    // Note the first s in both cases is after a special character
    #[ignore]
    #[test]
    fn found_bug_1() -> Result<()> {
        // This used to error
        highlight_sections_from_stringdices("Chimay Grande Réserve", &[0, 16])?;
        Ok(())
    }

    // It looks like unicode is the cause?
    #[ignore]
    #[test]
    fn found_bug_2() -> Result<()> {
        // This used to error
        highlight_sections_from_stringdices("Bell’s Expedition", &[0, 5])?;
        Ok(())
    }

    #[test]
    fn periods_are_ok() -> Result<()> {
        highlight_sections_from_stringdices("ABC.DEF.GHI", &[0, 4])?;
        Ok(())
    }

    #[test]
    fn normal_apostrophes_are_ok() -> Result<()> {
        highlight_sections_from_stringdices("ABC'DEF.GHI", &[0, 4])?;
        Ok(())
    }

    #[test]
    fn normal_backticks_are_ok() -> Result<()> {
        highlight_sections_from_stringdices("ABC`DEF.GHI", &[0, 4])?;
        Ok(())
    }
}
