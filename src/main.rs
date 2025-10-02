#![allow(unused)]
use color_eyre::Result;
use std::{io::Write, sync::atomic::AtomicU64};
use hacker::{game_loop::{self, GameEvent, GameSettings, LoopContext}, text_edit::{TextEdit, TextEditor}};
use ratatui::{prelude::*, widgets::{self, Block, Borders, Paragraph}, DefaultTerminal};
use crossterm::{event::{self, Event, KeyCode, KeyModifiers, MouseEventKind}, terminal::Clear};
use crossterm::execute;
use crossterm::cursor::{
    MoveTo,
    MoveLeft, MoveRight, MoveUp, MoveDown,
    MoveToColumn, MoveToRow,
    MoveToNextLine, MoveToPreviousLine,
    SavePosition, RestorePosition,
    EnableBlinking, DisableBlinking,
    Hide, Show,
    SetCursorStyle,
    position as get_cursor_pos,
};
use crossterm::{Command, ExecutableCommand, QueueableCommand, SynchronizedUpdate};
use scopeguard::defer;
use std::time::{Duration, Instant};
use spin_sleep::sleep_until;
use dmf::extensions::*;

const ALL_BORDER: Block<'static> = Block::new().borders(Borders::all()).border_set(symbols::border::DOUBLE);
// const DESIRED_SIZE: Size = Size::new(80, 60);

fn main() -> Result<()> {
    // let terminal = ratatui::init();
    // defer!(ratatui::restore());
    // run(terminal)
    const TEXT_BUFFER_SIZE: usize = 1024*32;
    const FRAME_TIME_MS: u64 = 16;
    const FRAME_TIME: Duration = Duration::from_millis(FRAME_TIME_MS);
    
    const SHORT_SCROLL: usize = 2;
    const LONG_SCROLL: usize = 10;
    let mut last_update_time = Instant::now() - FRAME_TIME;
    let mut text_edit = TextEditor::new();
    let mut line = 0usize;
    let mut col = 0usize;
    let mut max_prev_col = col;
    game_loop::run(
        GameSettings {
            render_frametime: FRAME_TIME,
            update_frametime: FRAME_TIME,
        },
        move |terminal: &mut DefaultTerminal, event: GameEvent, context: &LoopContext| -> Result<(), std::io::Error> {
            macro_rules! execute {
                ($($tokens:tt)*) => {
                    crossterm::execute!(terminal.backend_mut(), $($tokens)*)?
                };
            }
            let term_size = terminal.size()?;
            let (cx, cy) = get_cursor_pos()?;
            match event {
                GameEvent::TermEvent(event) => {
                    match event {
                        Event::Key(key_event) if key_event.is_press() => match key_event.code {
                            KeyCode::Up => {
                                if line != 0 {
                                    line -= 1;
                                    let line_slice = text_edit.rope.line(line);
                                    let mut line_len = line_slice.len_chars();
                                    while line_len != 0 && matches!(line_slice.char(line_len - 1), '\n' | '\r') {
                                        line_len -= 1;
                                    }
                                    col = col.min(line_len.min(max_prev_col)).max(max_prev_col.min(line_len));
                                    text_edit.start_col = text_edit.start_col.min(col);
                                    let ncx = col - text_edit.start_col;
                                    if cy != 0 {
                                        execute!(MoveTo(ncx as u16, cy - 1));
                                    } else {
                                        if text_edit.start_line != 0 {
                                            text_edit.start_line -= 1;
                                        }
                                        execute!(MoveTo(ncx as u16, cy));
                                    }
                                } else {
                                    col = 0;
                                    text_edit.start_col = 0;
                                    execute!(MoveTo(0, cy));
                                }
                            },
                            KeyCode::Down => {
                                if line + 1 != text_edit.rope.len_lines() {
                                    line += 1;
                                    let line_slice = text_edit.rope.line(line);
                                    let mut line_len = line_slice.len_chars();
                                    while line_len != 0 && matches!(line_slice.char(line_len - 1), '\n' | '\r') {
                                        line_len -= 1;
                                    }
                                    col = col.min(line_len).max(line_len.min(max_prev_col));
                                    text_edit.start_col = text_edit.start_col.min(col);
                                    let ncx = col - text_edit.start_col;
                                    if cy + 1 != term_size.height {
                                        execute!(MoveTo(ncx as u16, cy + 1));
                                    } else {
                                        text_edit.start_line += 1;
                                        execute!(MoveTo(ncx as u16, cy));
                                    }
                                } else {
                                    let line_slice = text_edit.rope.line(line);
                                    let mut line_len = line_slice.len_chars();
                                    while line_len != 0 && matches!(line_slice.char(line_len - 1), '\n' | '\r') {
                                        line_len -= 1;
                                    }
                                    col = line_len;
                                    if col - text_edit.start_col >= term_size.width as usize {
                                        text_edit.start_col = col + 1 - term_size.width as usize;
                                    }
                                    let ncx = col - text_edit.start_col;
                                    execute!(MoveTo(ncx as u16, cy));
                                }
                            },
                            KeyCode::Left => {
                                if col != 0 {
                                    col -= 1;
                                    if cx != 0 {
                                        execute!(MoveLeft(1));
                                    } else {
                                        if text_edit.start_col != 0 {
                                            text_edit.start_col -= 1;
                                        }
                                    }
                                } else {
                                    if line != 0 {
                                        line -= 1;
                                        let line_slice = text_edit.rope.line(line);
                                        let line_len = line_slice.len_chars();
                                        col = line_len - 1;
                                        if cy != 0 {
                                            execute!(MoveTo(
                                                if col <= (term_size.width + 1) as usize {
                                                    col as u16
                                                } else {
                                                    term_size.width + 1
                                                },
                                                cy - 1
                                            ));
                                        } else {
                                            if text_edit.start_line != 0 {
                                                text_edit.start_line -= 1;
                                                execute!(MoveTo(
                                                    if col <= term_size.width as usize {
                                                        col as u16
                                                    } else {
                                                        term_size.width + 1
                                                    },
                                                    cy
                                                ));
                                            }
                                        }
                                    }
                                }
                                max_prev_col = col;
                            },
                            KeyCode::Right => {
                                if line < text_edit.rope.len_lines() {
                                    let line_slice = text_edit.rope.line(line);
                                    let mut line_len = line_slice.len_chars();
                                    while line_len != 0 && matches!(line_slice.char(line_len - 1), '\n' | '\r') {
                                        line_len -= 1;
                                    }
                                    if col < line_len {
                                        col += 1;
                                        if cx + 1 != term_size.width {
                                            execute!(MoveRight(1));
                                        } else {
                                            text_edit.start_col += 1;
                                        }
                                    } else if line + 1 < text_edit.rope.len_lines() {
                                        line += 1;
                                        col = 0;
                                        text_edit.start_col = 0;
                                        if cy + 1 != term_size.height {
                                            execute!(MoveTo(0, cy + 1));
                                        } else {
                                            text_edit.start_line += 1;
                                            execute!(MoveTo(0, cy));
                                        }
                                    }
                                    max_prev_col = col;
                                }
                            },
                            KeyCode::Esc => context.request_exit(game_loop::ExitRequest::Success),
                            KeyCode::Char('q') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                                context.request_exit(game_loop::ExitRequest::Success);
                            }
                            KeyCode::Delete => {
                                let line_start = text_edit.rope.line_to_char(line);
                                let char_index = line_start + col;
                                text_edit.rope.try_remove(char_index..=char_index);
                            }
                            KeyCode::Backspace => {
                                if (col > 0 || line != 0) && line < text_edit.rope.len_lines() {
                                    let line_start = text_edit.rope.line_to_char(line);
                                    let char_index = line_start + col;
                                    if char_index != 0 && char_index <= text_edit.rope.len_chars() {
                                        if col != 0 {
                                            col -= 1;
                                            if cx != 0 {
                                                execute!(MoveLeft(1));
                                            } else {
                                                if text_edit.start_col != 0 {
                                                    text_edit.start_col -= 1;
                                                }
                                            }
                                        } else {
                                            if line != 0 {
                                                line -= 1;
                                                let line_len = text_edit.rope.line(line).len_chars();
                                                col = line_len - 1;
                                                if cy != 0 {
                                                    execute!(MoveTo(
                                                        if col <= (term_size.width + 1) as usize {
                                                            col as u16
                                                        } else {
                                                            term_size.width + 1
                                                        },
                                                        cy - 1
                                                    ));
                                                } else {
                                                    if text_edit.start_line != 0 {
                                                        text_edit.start_line -= 1;
                                                        execute!(MoveTo(
                                                            if col <= term_size.width as usize {
                                                                col as u16
                                                            } else {
                                                                term_size.width + 1
                                                            },
                                                            cy,
                                                        ));
                                                    }
                                                }
                                                let new_col_start = line_len - (term_size.width as usize - 1).min(line_len);
                                                text_edit.start_col = new_col_start;
                                            }
                                        }
                                        if let Ok(()) = text_edit.rope.try_remove(char_index-1..char_index) {
                                        }
                                    }
                                }
                                max_prev_col = col;
                            }
                            KeyCode::Home => {
                                col = 0;
                                text_edit.start_col = 0;
                                max_prev_col = col;
                                execute!(MoveTo(0, cy));
                            }
                            KeyCode::End => {
                                let line_slice = text_edit.rope.line(line);
                                let mut line_len = line_slice.len_chars();
                                while line_len != 0 && matches!(line_slice.char(line_len - 1), '\n' | '\r') {
                                    line_len -= 1;
                                }
                                col = line_len;
                                if col - text_edit.start_col >= term_size.width as usize {
                                    text_edit.start_col = col + 1 - term_size.width as usize;
                                }
                                let ncx = col - text_edit.start_col;
                                max_prev_col = col;
                                execute!(MoveTo(ncx as u16, cy));
                            }
                            KeyCode::Tab => {
                                let next_indent = (col + 1).next_multiple_of(4);
                                let space_count = (next_indent - col);
                                const TAB_SPACES: &'static str = "    ";
                                let line_start = text_edit.rope.line_to_char(line);
                                let insert_idx = line_start + col;
                                if let Ok(()) = text_edit.rope.try_insert(insert_idx, &TAB_SPACES[..space_count]) {
                                    col = next_indent;
                                    if col > text_edit.start_col + term_size.width as usize {
                                        text_edit.start_col = col - term_size.width as usize;
                                    }
                                    let ncx = col - text_edit.start_col;
                                    execute!(MoveTo(ncx as u16, cy));
                                    max_prev_col = col;
                                }
                            }
                            KeyCode::Enter => {
                                let line_start = text_edit.rope.line_to_char(line);
                                let char_index = line_start + col;
                                if let Ok(()) = text_edit.rope.try_insert_char(char_index, '\n') {
                                    col = 0;
                                    text_edit.start_col = 0;
                                    line += 1;
                                    if cy + 1 != term_size.height {
                                        execute!(MoveTo(0, cy + 1));
                                    } else {
                                        text_edit.start_line += 1;
                                        execute!(MoveTo(0, cy));
                                    }
                                    max_prev_col = col;
                                }
                            }
                            KeyCode::Char(chr) if chr != '\n' => {
                                if line <= text_edit.rope.len_lines() {
                                    let line_start = text_edit.rope.line_to_char(line);
                                    let char_index = line_start + col;
                                    text_edit.rope.try_insert_char(char_index, chr);
                                    col += 1;
                                    if cx + 1 != term_size.width {
                                        execute!(MoveRight(1));
                                    } else {
                                        text_edit.start_col += 1;
                                    }
                                    max_prev_col = col;
                                }
                            }
                            _ => (),
                        }
                        Event::Mouse(mouse_event) => {
                            match mouse_event.kind {
                                MouseEventKind::Down(mouse_button) => {
                                    
                                },
                                MouseEventKind::Up(mouse_button) => {
                                    
                                },
                                MouseEventKind::Drag(mouse_button) => {
                                    
                                },
                                MouseEventKind::Moved => {
                                    
                                },
                                MouseEventKind::ScrollDown => {
                                    let scroll = if mouse_event.modifiers.contains(KeyModifiers::ALT) {
                                        LONG_SCROLL
                                    } else {
                                        SHORT_SCROLL
                                    };
                                    if mouse_event.modifiers.contains(KeyModifiers::SHIFT) {
                                        text_edit.start_col += scroll;
                                    } else {
                                        text_edit.start_line += scroll;
                                    }
                                },
                                MouseEventKind::ScrollUp => {
                                    let scroll = if mouse_event.modifiers.contains(KeyModifiers::ALT) {
                                        LONG_SCROLL
                                    } else {
                                        SHORT_SCROLL
                                    };
                                    if mouse_event.modifiers.contains(KeyModifiers::SHIFT) {
                                        if text_edit.start_col >= scroll {
                                            text_edit.start_col -= scroll;
                                        } else {
                                            text_edit.start_col = 0;
                                        }
                                    } else {
                                        if text_edit.start_line >= scroll {
                                            text_edit.start_line -= scroll;
                                        } else {
                                            text_edit.start_line = 0;
                                        }
                                    }
                                },
                                MouseEventKind::ScrollLeft => {
                                    let scroll = if mouse_event.modifiers.contains(KeyModifiers::ALT) {
                                        LONG_SCROLL
                                    } else {
                                        SHORT_SCROLL
                                    };
                                    if text_edit.start_col >= scroll {
                                        text_edit.start_col -= scroll;
                                    } else {
                                        text_edit.start_col = 0;
                                    }
                                },
                                MouseEventKind::ScrollRight => {
                                    let scroll = if mouse_event.modifiers.contains(KeyModifiers::ALT) {
                                        LONG_SCROLL
                                    } else {
                                        SHORT_SCROLL
                                    };
                                    text_edit.start_col += scroll;
                                },
                            }
                        }
                        Event::Resize(width, height) => {
                            let width = width as usize;
                            let height = height as usize;
                            if col > text_edit.start_col + width {
                                text_edit.start_col = col - width;
                            }
                            if line > text_edit.start_line + height {
                                text_edit.start_line = line - height;
                            }
                        }
                        Event::Paste(pasta) => {
                            panic!();
                            static COUNTER: AtomicU64 = AtomicU64::new(0);
                            fn next_id() -> u64 {
                                COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                            }
                            let line_start = text_edit.rope.line_to_char(line);
                            let char_index = line_start + col;
                            let pasta = format!("(Paste: {})", next_id());
                            text_edit.rope.try_insert(char_index, &pasta);
                            let mut new_line = line;
                            let mut new_col = col;
                            for chr in pasta.chars() {
                                match chr {
                                    '\n' => {
                                        new_col = 0;
                                        new_line += 1;
                                    }
                                    '\r' => continue,
                                    c => new_col += 1,
                                }
                            }
                            line = new_line;
                            col = new_col;
                            max_prev_col = col;
                            let bottom = text_edit.start_line + term_size.height as usize;
                            text_edit.start_line = text_edit.start_line
                                .min(line)
                                .max(line - term_size.height as usize);
                            text_edit.start_col = text_edit.start_col
                                .min(col)
                                .max(col + 1 - term_size.width as usize);
                            let ncx = col - text_edit.start_col;
                            let ncy = line - text_edit.start_line;
                            execute!(MoveTo(ncx as u16, ncy as u16));
                            context.request_render();
                        }
                        _ => (),
                    }
                },
                GameEvent::Begin(game_settings) => {
                    execute!(MoveTo(0, 0));
                },
                GameEvent::Update => {
                    
                },
                GameEvent::Render => {
                    terminal.draw(|frame| {
                        frame.buffer_mut().reset();
                        let area = frame.area();
                        // let text_area = Rect::new(area.x, area.y, area.width, area.height - 1);
                        // let display_area = Rect::new(area.x, text_area.bottom(), area.width, 1);
                        frame.render_stateful_widget(TextEdit, area, &mut text_edit);
                        // let info = format!("start_line: {} start_col: {} line: {} col: {}, cx: {cx}, cy: {cy}", text_edit.start_line, text_edit.start_col, line, col);
                        // frame.render_widget(info, display_area);
                        
                    })?;
                    execute!(Show);
                    execute!(MoveTo(cx, cy));
                },
                GameEvent::ExitRequested(cancellable_exit_request) => {
                    
                },
                GameEvent::Exiting => {
                    
                },
            }
            Ok(())
        }
    )?;
    Ok(())
}

struct HackerText;

impl Widget for HackerText {
    fn render(self, area: Rect, buf: &mut Buffer)
        where
            Self: Sized {
        const CHARS: &'static [char] = &[
            'a', 'b', 'c', 'd',
            'e', 'f', 'g', 'h',
            'i', 'j', 'k', 'l',
            'm', 'n', 'o', 'p',
            'q', 'r', 's', 't',
            'u', 'v', 'w', 'x',
            'y', 'z', 'A', 'B',
            'C', 'D', 'E', 'F',
            'G', 'H', 'I', 'J',
            'K', 'L', 'M', 'N',
            'O', 'P', 'Q', 'R',
            'S', 'T', 'U', 'V',
            'W', 'X', 'Y', 'Z',
            ' ',
            // '0', '1', '2', '3', '4',
            // '5', '6', '7', '8', '9',
            // '+', '=', '!', '@', '#',
            // '$', '%', '^', '&', '*',
            // ';', ':', '-', '_', '<',
            // '>', '?', '.', '/', '\\',
            // '|', '~', '`', ',', '.',
            // '\'', '"',
        ];
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    let chr_i = rand::random_range(0..CHARS.len());
                    cell.set_char(CHARS[chr_i]);
                }
            }
        }
    }
}

// struct HackerGame {
//     terminal: DefaultTerminal,
// }

fn run(mut terminal: DefaultTerminal) -> Result<()> {
    // let size = terminal.size()?;
    
    let mut counter = 0u64;
    let mut pressed_key = KeyCode::Null;
    let mut text_edit = String::with_capacity(1024);
    let mut x = 0u16;
    let mut y = 0u16;
    let mut last_frame_time = Instant::now() - Duration::from_secs(10);
    const FRAME_TIME: Duration = Duration::from_millis(16);
    'game_loop: loop {
        let next_time = last_frame_time + FRAME_TIME;
        sleep_until(next_time);
        last_frame_time = Instant::now();
        while event::poll(Duration::from_secs(0))? {
            match event::read()? {
                Event::Key(event) => {
                    if event.is_press() && let Some(chr) = event.code.as_char() {
                        text_edit.push(chr);
                    }
                    pressed_key = event.code;
                    if event.is_press() {
                        match event.code {
                            KeyCode::Esc | KeyCode::Char('q') => break 'game_loop Ok(()),
                            KeyCode::Backspace => _=text_edit.pop(),
                            KeyCode::Enter => text_edit.push('\n'),
                            KeyCode::Up if y != 0 => y -= 1,
                            KeyCode::Down if y != u16::MAX => y += 1,
                            KeyCode::Left if x != 0 => x -= 1,
                            KeyCode::Right if x != u16::MAX => x += 1,
                            _ => ()
                        
                        }
                    }
                }
                _ => (),
            }
        }
        terminal.draw(|frame: &mut Frame| {
            let area = frame.area();
            frame.render_widget(HackerText, area);
            let text = format!("(x: {}, y: {}, w: {}, h: {})\nCounter: {counter}\nKey: {pressed_key}\nText: {text_edit}", area.x, area.y, area.width, area.height);
            let para = Paragraph::new(text);
            frame.render_widget(para.block(ALL_BORDER), area);
            let blah_rect = area.inner(Margin::new(1, 0));
            frame.render_widget("Blah", blah_rect);
            x = x.min(area.right() - 1);
            y = y.min(area.bottom() - 1);
            let me_rect = Rect::new(x, y, 1, 1);
            frame.render_widget("o", me_rect);
            
        })?;
        counter += 1;
        
    }
}
