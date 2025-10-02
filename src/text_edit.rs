use ratatui::prelude::*;
use ropey::Rope;

pub struct TextEditor {
    pub rope: Rope,
    pub start_line: usize,
    pub start_col: usize,
}

impl TextEditor {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            start_line: 0,
            start_col: 0,
        }
    }
}

pub struct TextEdit;

const RAINBOW_INDENT_COLORS: [Color; 6] = [
    Color::Rgb(68, 17, 10),
    Color::Rgb(70, 34, 6),
    Color::Rgb(69, 58, 2),
    Color::Rgb(7, 40, 24),
    Color::Rgb(16, 30, 51),
    Color::Rgb(26, 14, 45)
];

impl StatefulWidget for TextEdit {
    type State = TextEditor;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        for (iy, y) in (area.y..area.bottom()).enumerate() {
            let line_index = state.start_line + iy;
            if line_index >= state.rope.len_lines() {
                break;
            }
            //               //             
            let line = state.rope.line(line_index);
            if state.start_col >= line.len_chars() {
                continue;
            }
            let line_end = line.slice(state.start_col..);
            let line_len = line_end.len_chars();
            let mut indent = true;
            'inner: for (ix, x) in (area.x..area.right()).enumerate() {
                let char_index = ix;
                if char_index >= line_len {
                    break 'inner;
                }
                // ·
                const SPACE_CHAR: char = '·';
                const GROUP_CHAR: char = '┆';
                if let Some(cell) = buf.cell_mut((x, y)) {
                    match line_end.char(char_index) {
                        '\n' => continue 'inner,
                        ' ' => {
                            if indent {
                                let indent_idx = char_index / 4;
                                let indent_color = RAINBOW_INDENT_COLORS[indent_idx % RAINBOW_INDENT_COLORS.len()];
                                let cell_char = if char_index.is_multiple_of(4) {
                                    GROUP_CHAR
                                } else {
                                    SPACE_CHAR
                                };
                                cell.set_char(cell_char)
                                    .set_bg(indent_color)
                                    .set_fg(Color::DarkGray);
                            } else {
                                cell.set_char(SPACE_CHAR)
                                    .set_fg(Color::DarkGray);
                            }
                        }
                        c => {
                            indent = false;
                            cell.set_char(c);
                        }
                    }
                }
            }
        }
    }
}