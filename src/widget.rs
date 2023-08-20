use crate::{
    highlight::{sections_from_stringdices, MatchHighlightError, Style as HighlightStyle},
    FuzzyFinder,
};
use tui::{
    prelude::*,
    widgets::{Block, List, ListItem, StatefulWidget},
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
            sections_from_stringdices(value, indices)?
                .iter()
                .map(|section| match section {
                    HighlightStyle::None(sub) => Span::styled(*sub, self.unmatched_char_style),
                    HighlightStyle::Matched(sub) => Span::styled(*sub, self.matched_char_style),
                })
                .collect::<Vec<Span>>(),
        ))
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
