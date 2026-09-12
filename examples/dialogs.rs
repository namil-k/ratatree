//! The three dialogs an application usually needs, each shown as a modal over a small host screen.
//!
//! `1` opens a file, `2` chooses a folder, `3` picks several files. The modal is 50x18 on purpose: that is the size a real application gave the picker, and it is where a long path and a big directory still have to make sense.

use std::io;
use std::path::PathBuf;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::{Frame, Terminal};

use ratatree::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatree::crossterm::execute;
use ratatree::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatree::{FilePicker, FilePickerState, PickerMode, PickerResult};

/// Which dialog is open, so the result can be labelled and the picker built to match.
#[derive(Clone, Copy)]
enum Dialog {
    OpenFile,
    ChooseFolder,
    PickFiles,
}

impl Dialog {
    fn title(self) -> &'static str {
        match self {
            Dialog::OpenFile => " Open file ",
            Dialog::ChooseFolder => " Choose folder ",
            Dialog::PickFiles => " Pick files ",
        }
    }

    fn build(self) -> FilePickerState {
        let start = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let builder = FilePickerState::builder().start_dir(start);
        match self {
            Dialog::OpenFile => builder
                .mode(PickerMode::FilesOnly)
                .filter(|path| {
                    path.extension()
                        .is_some_and(|ext| ext == "pdf" || ext == "md")
                })
                .build(),
            Dialog::ChooseFolder => builder.mode(PickerMode::DirsOnly).build(),
            Dialog::PickFiles => builder.mode(PickerMode::Both).build(),
        }
    }
}

struct App {
    picker: Option<(Dialog, FilePickerState)>,
    last: String,
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut app = App {
        picker: None,
        last: "Nothing picked yet.".to_string(),
    };
    let result = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }

        // While a dialog is open every key goes to it. Its result decides when it closes.
        if let Some((dialog, picker)) = &mut app.picker {
            picker.handle_event(Event::Key(key));
            let label = dialog.title().trim();
            let outcome = match picker.result() {
                PickerResult::Selected(paths) => {
                    let list: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
                    Some(format!("{label}: {}", list.join(", ")))
                }
                PickerResult::Cancelled => Some(format!("{label}: cancelled")),
                PickerResult::Pending => None,
            };
            if let Some(text) = outcome {
                app.last = text;
                app.picker = None;
            }
            continue;
        }

        match key.code {
            KeyCode::Char('1') => app.picker = Some((Dialog::OpenFile, Dialog::OpenFile.build())),
            KeyCode::Char('2') => {
                app.picker = Some((Dialog::ChooseFolder, Dialog::ChooseFolder.build()))
            }
            KeyCode::Char('3') => app.picker = Some((Dialog::PickFiles, Dialog::PickFiles.build())),
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
            _ => {}
        }
    }
}

fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let help = Line::from(vec![
        Span::styled("1", Style::new().bold()),
        Span::raw(": open file   "),
        Span::styled("2", Style::new().bold()),
        Span::raw(": choose folder   "),
        Span::styled("3", Style::new().bold()),
        Span::raw(": pick files   "),
        Span::styled("q", Style::new().bold()),
        Span::raw(": quit"),
    ]);
    let body = vec![help, Line::raw(""), Line::raw(app.last.as_str())];
    frame.render_widget(
        Paragraph::new(body).block(Block::bordered().title(" dialogs ")),
        area,
    );

    if let Some((dialog, picker)) = &mut app.picker {
        let modal = centered(area, 50, 18);
        frame.render_widget(Clear, modal);
        let widget = FilePicker::default().block(Block::bordered().title(dialog.title()));
        frame.render_stateful_widget(widget, modal, picker);
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}
