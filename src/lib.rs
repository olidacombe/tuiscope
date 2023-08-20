//! # `TuiScope`
//!
//! Inspired by [telescope](https://github.com/nvim-telescope/telescope.nvim).
//!
//! A TUI fuzzy finder for rust apps. For example usage, see [examples](https://github.com/olidacombe/tuiscope/tree/main/examples).
#![deny(clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::return_self_not_must_use)]
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use indexmap::IndexMap;
use rayon::prelude::*;
use std::{
    borrow::Cow,
    cmp::Ordering,
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
/// let fuzzy_results = FuzzyList::default()
///     .block(Block::default().borders(Borders::ALL).title("Matches"))
///     .matched_char_style(Style::default().fg(Color::Cyan))
///     .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
/// ```
#[derive(Default)]
pub struct FuzzyList<'a> {
    block: Option<Block<'a>>,
    matched_char_style: Style,
    selection_highlight_style: Style,
    unmatched_char_style: Style,
}

impl<'a> FuzzyList<'a> {
    /// Builder method to add a block specification to a `FuzzyList`
    ///
    /// # Example
    ///
    /// ```
    /// use tui::widgets::*;
    /// use tuiscope::FuzzyList;
    ///
    /// let fuzzy = FuzzyList::default().block( Block::default().borders(Borders::ALL).title("Matches"));
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
    /// let fuzzy = FuzzyList::default().block( Block::default().borders(Borders::ALL).title("Matches"));
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
    /// let fuzzy_results = FuzzyList::default()
    ///     .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
    /// ```
    pub fn selection_highlight_style(mut self, style: Style) -> Self {
        self.selection_highlight_style = style;
        self
    }

    fn styled_line(
        &self,
        value: &'a str,
        indices: &'a [usize],
    ) -> Result<Line, MatchHighlightError> {
        Ok(Line::from(
            highlight_sections_from_stringdices(value, indices)?
                .iter()
                .map(|section| match section {
                    HighlightStyle::None(sub) => Span::styled(*sub, self.unmatched_char_style),
                    HighlightStyle::Matched(sub) => Span::styled(*sub, self.matched_char_style),
                })
                .collect::<Vec<Span>>(),
        ))
    }
}

/// Type for holding fuzzy match score with corresponding indices
struct FuzzyScore {
    /// fuzzy match score
    pub score: i64,
    /// fuzzy match indices (positions in the matched string)
    pub indices: Vec<usize>,
}

impl Ord for FuzzyScore {
    fn cmp(&self, other: &Self) -> Ordering {
        // reverse so ascending order is highest score first!!!
        other.score.cmp(&self.score)
    }
}

impl PartialOrd for FuzzyScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for FuzzyScore {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl Eq for FuzzyScore {}

/// Return type for `FuzzyFinder::selection`
#[derive(Clone)]
pub struct FuzzyListEntry<'a> {
    /// value of entry
    pub value: &'a str, // TODO not a &str?
    /// fuzzy match score
    pub score: i64,
    /// fuzzy match indices (positions in `value`)
    pub indices: Vec<usize>,
}

/// State for `FuzzyList<K>`.  Hold on to one of these and pass to `render_stateful_widget`
///
/// # Example
///
/// ```
/// use tui::prelude::*;
/// use tui::widgets::*;
/// use tuiscope::{FuzzyFinder, FuzzyList};
///
/// fn ui<B: Backend>(f: &mut Frame<B>, state: &mut FuzzyFinder) {
///     let chunks = Layout::default()
///         .direction(Direction::Vertical)
///         .constraints([Constraint::Min(1)].as_ref())
///         .split(f.size());
///
///     let fuzzy_results = FuzzyList::default()
///         .block(Block::default().borders(Borders::ALL).title("Options"))
///         .matched_char_style(Style::default().fg(Color::Cyan))
///         .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
///     f.render_stateful_widget(fuzzy_results, chunks[2], state);
/// }
/// ```
#[derive(Default)]
pub struct FuzzyFinder<'a> {
    /// The current filter string.
    filter: Cow<'a, str>,
    /// IndexMap of FuzzyScore.
    matches: IndexMap<Cow<'a, str>, Option<FuzzyScore>>,
    /// State for the `FuzzyList` widget's selection.
    pub state: ListState,
}

impl<'a> FuzzyFinder<'a> {
    /// Clears the filter term.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.set_filter("foo");
    /// ff.clear_filter();
    /// ```
    pub fn clear_filter(&mut self) -> &mut Self {
        self.filter = Cow::default();
        self.update_matches(true);
        self
    }

    /// Resets the selected line from filtered options to the 0th.
    fn reset_selection(&mut self) -> &mut Self {
        if self.matches.is_empty() {
            self.state.select(None);
        } else {
            self.state.select(Some(0));
        }
        self
    }

    /// Select the next filtered entry.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.select_next();
    /// ```
    pub fn select_next(&mut self) -> &mut Self {
        if let Some(current) = self.state.selected() {
            self.select(current + 1);
        } else {
            self.reset_selection();
        }
        self
    }

    /// Select the previous filtered entry.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.select_next();
    /// ff.select_next();
    /// ff.select_prev();
    /// ```
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
        let len = self.matches.len();
        if len < 1 {
            return self.reset_selection();
        }
        self.state.select(Some(std::cmp::min(index, len - 1)));
        self
    }

    /// Get the current selected entry.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.select_next();
    /// ff.select_next();
    /// ff.select_prev();
    /// let answer = ff.selection();
    /// ```
    pub fn selection(&self) -> Option<FuzzyListEntry> {
        self.state.selected().and_then(|i| {
            self.matches.get_index(i).and_then(|(value, score)| {
                score
                    .as_ref()
                    .map(|FuzzyScore { score, indices }| FuzzyListEntry {
                        value,
                        indices: indices.clone(),
                        score: *score,
                    })
            })
        })
    }

    /// Updates the filter term.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.set_filter("foo");
    /// ```
    pub fn set_filter<T: Into<Cow<'a, str>>>(&mut self, filter: T) -> &mut Self {
        self.filter = filter.into();
        self.update_matches(true);
        self
    }

    /// Updates the set of options to search by adding from an iterator.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.push_options(["abc", "bcd", "cde"]);
    /// ```
    pub fn push_options<T: 'a + IntoIterator<Item = R>, R: Into<Cow<'a, str>>>(
        &mut self,
        options: T,
    ) -> &mut Self {
        for option in options {
            self._push_option(option);
        }
        self.update_matches(false);
        self
    }

    /// Builder method which sets search options.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let ff = FuzzyFinder::default().with_options(["one", "two", "three"]);
    /// ```
    pub fn with_options<T: 'a + IntoIterator<Item = R>, R: Into<Cow<'a, str>>>(
        mut self,
        options: T,
    ) -> Self {
        self.set_options(options);
        self
    }

    /// Sets search options.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.set_options(["one", "two", "three"]);
    /// ```
    pub fn set_options<T: 'a + IntoIterator<Item = R>, R: Into<Cow<'a, str>>>(
        &mut self,
        options: T,
    ) -> &mut Self {
        // TODO be more efficient, keep any existing scores for overlapping keys.
        self.matches.clear();
        self.push_options(options);
        self
    }

    /// Add an option to search.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.push_option("hello");
    /// ```
    pub fn push_option<R: Into<Cow<'a, str>>>(&mut self, option: R) {
        self._push_option(option);
        self.update_matches(false);
    }

    /// Adds an option to search without updating.
    fn _push_option<R: Into<Cow<'a, str>>>(&mut self, option: R) {
        // keep existing score if entry exists.
        self.matches.entry(option.into()).or_insert(None);
    }

    /// Computes new scores for all options if `new_filter_term` is true.
    /// Otherwise competes scores for all options who haven't had a calculation
    /// yet against the current filter.
    fn update_matches(&mut self, new_filter_term: bool) {
        let matcher = SkimMatcherV2::default();

        // TODO None matches were inserted last, so we should be able to iterate
        // from the end and stop early.  But I couldn't quite find the right
        // early-stopping option for an IndexedParallesIterator
        // iter = iter.rev().take_any_while... race behavior is not ideal
        self.matches
            .par_iter_mut()
            .filter(|(_, score)| new_filter_term || score.is_none())
            .for_each(|(value, score)| {
                *score = matcher
                    .fuzzy_indices(value, &self.filter)
                    .map(|(score, indices)| FuzzyScore { score, indices });
            });

        self.matches.par_sort_unstable_by(|_, v1, _, v2| match v1 {
            Some(v1) => match v2 {
                Some(v2) => v1.cmp(v2),
                None => Ordering::Less,
            },
            None => Ordering::Greater,
        });

        // TODO only if some change
        self.reset_selection();
    }
}

impl<'a> StatefulWidget for FuzzyList<'a> {
    type State = FuzzyFinder<'a>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list: Vec<ListItem> = state
            .matches
            .iter()
            .filter_map(|(value, score)| {
                score
                    .as_ref()
                    .and_then(|score| self.styled_line(value, &score.indices).ok())
            })
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
