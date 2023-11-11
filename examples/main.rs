use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fakeit::beer;
use std::{error::Error, io};
use tui::{prelude::*, widgets::*};
use tui_input::{backend::crossterm::EventHandler, Input};
use tuiscope::{FuzzyFinder, FuzzyList};

#[derive(Default)]
enum InputMode {
    #[default]
    Normal,
    Editing,
}

/// App holds the state of the application
#[derive(Default)]
struct App<'a> {
    /// Current value of the input box
    input: Input,
    /// Fuzzy Finder
    pub fuzzy_finder: FuzzyFinder<'a>,
    /// Current input mode
    input_mode: InputMode,
    /// History of recorded messages
    messages: Vec<String>,
}

impl<'a> App<'a> {
    fn submit_message(&mut self) {
        if let Some(selection) = self.fuzzy_finder.selection() {
            self.messages.push(selection.value.to_string());
        }
        self.input.reset();
        self.fuzzy_finder.clear_filter();
    }
    pub fn handle_key(&mut self, key: &crossterm::event::Event) {
        self.input.handle_event(key);
        self.fuzzy_finder.set_filter(self.input.to_string());
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Error's to stderr make a mess of the interface, use this only for inspecting tracing output.
    // tracing_subscriber::fmt::init();

    let mut options = Vec::<String>::new();
    for _ in 1..100 {
        options.push(beer::name());
    }

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let mut app = App::default();
    app.fuzzy_finder.push_options(&options);
    let res = run_app(&mut terminal, app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('f') => {
                        app.input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('q') => {
                        return Ok(());
                    }
                    _ => {}
                },
                InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter => app.submit_message(),
                    KeyCode::Up => {
                        app.fuzzy_finder.select_prev();
                    }
                    KeyCode::Down => {
                        app.fuzzy_finder.select_next();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                    }
                    _ => {
                        app.handle_key(&crossterm::event::Event::Key(key));
                    }
                },
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Percentage(40),
            ]
            .as_ref(),
        )
        .split(f.size());

    let (msg, style) = match app.input_mode {
        InputMode::Normal => (
            vec![
                "Press ".into(),
                "q".bold(),
                " to exit, ".into(),
                "f".bold(),
                " to start filtering.".bold(),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        InputMode::Editing => (
            vec![
                "Press ".into(),
                "Esc".bold(),
                " to stop filtering, ".into(),
                "Up/Down".bold(),
                " to highlight selection, ".into(),
                "Enter".bold(),
                " to commit the selection.".into(),
            ],
            Style::default(),
        ),
    };
    let mut text = Text::from(Line::from(msg));
    text.patch_style(style);
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, chunks[0]);

    let input = Paragraph::new(app.input.to_string())
        .style(match app.input_mode {
            InputMode::Normal => Style::default(),
            InputMode::Editing => Style::default().fg(Color::Yellow),
        })
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[1]);

    let fuzzy_results = FuzzyList::default()
        .block(Block::default().borders(Borders::ALL).title("Options"))
        .matched_char_style(Style::default().fg(Color::Cyan))
        .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_stateful_widget(fuzzy_results, chunks[2], &mut app.fuzzy_finder);

    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let content = Line::from(Span::raw(format!("{i}: {m}")));
            ListItem::new(content)
        })
        .collect();
    let messages =
        List::new(messages).block(Block::default().borders(Borders::ALL).title("Messages"));
    f.render_widget(messages, chunks[3]);
}
